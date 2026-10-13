use super::*;
use std::time::Duration;

const ACK_LIMIT: usize = 64;
const ACK_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, PartialEq, Eq)]
pub enum NativeInput {
    Applied,
    DiscardedUntilGotoAck,
}

pub(super) fn goto(p: &mut Player, own: bool, body: Vec<u8>) {
    let Some(output) = &p.tx else { return };
    if p.goto_acks.len() == ACK_LIMIT {
        tracing::error!(
            character = p.character.id,
            "GOTO acknowledgement backlog exceeded"
        );
        output.lost.send_replace(true);
        return;
    }
    // ACK has no object ID, so other players' GOTOs must share the same FIFO.
    p.goto_acks.push_back((own, Instant::now()));
    send(p, wire::GOTO, body);
}

impl Player {
    pub(super) fn correcting(&self) -> Result<bool> {
        ensure!(
            self.character.alive
                && !self.death_pending
                && !self.departing
                && self.persistence_failure.is_none(),
            "inactive player cannot reconcile position"
        );
        if let Some((_, sent)) = self.goto_acks.iter().find(|(own, _)| *own) {
            ensure!(
                sent.elapsed() <= ACK_TIMEOUT,
                "GOTO acknowledgement timed out"
            );
            return Ok(true);
        }
        Ok(false)
    }
}

impl World {
    pub fn acknowledge_goto(&mut self, id: i32, time: i32) -> Result<()> {
        let p = self
            .players
            .get_mut(&id)
            .context("GOTO ACK player missing")?;
        let (own, sent) = *p
            .goto_acks
            .front()
            .context("unsolicited GOTO acknowledgement")?;
        ensure!(time >= 0, "negative GOTO acknowledgement time");
        ensure!(
            sent.elapsed() <= ACK_TIMEOUT,
            "GOTO acknowledgement timed out"
        );
        if own {
            ensure!(time >= p.last_move, "GOTO acknowledgement clock regressed");
            // The native client ACKs with lastUpdate_, then moves to the supplied position.
            p.last_move = time;
        }
        p.goto_acks.pop_front();
        tracing::debug!(player = id, client_time = time, own, "GOTO acknowledged");
        Ok(())
    }

    pub(super) fn discard_correcting_move(&self, id: i32, pos: Pos, time: i32) -> Result<bool> {
        let p = self.players.get(&id).context("player missing")?;
        ensure!(
            time >= 0 && pos.x.is_finite() && pos.y.is_finite(),
            "invalid correcting MOVE"
        );
        let correcting = p.correcting()?;
        if correcting {
            tracing::warn!(
                player = id,
                client_time = time,
                "discarded MOVE awaiting GOTO acknowledgement"
            );
        }
        Ok(correcting)
    }

    pub(super) fn discard_correcting_shot(
        &mut self,
        id: i32,
        time: i32,
        bullet: u8,
        kind: u16,
        start: Pos,
        angle: f32,
    ) -> Result<bool> {
        let p = self.players.get_mut(&id).context("player missing")?;
        let correcting = p.correcting()?;
        let continuing = p.discarded_attack.is_some_and(|(last, _)| time == last);
        if let Some((last, _)) = p.discarded_attack {
            ensure!(time >= last, "discarded attack clock regressed");
        }
        if !correcting && !continuing {
            if let Some((last, _)) = p.discarded_attack {
                ensure!(time > last, "recovered attack clock regressed");
            }
            return Ok(false);
        }
        ensure!(
            time >= 0
                && time >= p.last_attack
                && start.x.is_finite()
                && start.y.is_finite()
                && angle.is_finite(),
            "invalid correcting SHOOT"
        );
        ensure!(
            p.character.items[0] == i32::from(kind),
            "weapon does not match equipped item"
        );
        ensure!(bullet == p.next_bullet, "unexpected player bullet counter");
        let count = if continuing {
            p.discarded_attack.unwrap().1
        } else if time == p.last_attack {
            p.attack_count
        } else {
            0
        };
        ensure!(
            count < self.content.get(kind)?.shots,
            "too many discarded projectiles in attack"
        );
        p.discarded_attack = Some((time, count + 1));
        // Ruffle allocated these bullet IDs before receiving the correction; authority stays unchanged.
        p.next_bullet = (bullet + 1) % 128;
        tracing::warn!(
            player = id,
            client_time = time,
            bullet,
            correcting,
            "discarded SHOOT across GOTO acknowledgement"
        );
        Ok(true)
    }

    pub(super) fn expire_goto_acks(&mut self) {
        for (id, p) in &mut self.players {
            if !p.departing
                && p.goto_acks
                    .front()
                    .is_some_and(|(_, sent)| sent.elapsed() > ACK_TIMEOUT)
            {
                if let Some(output) = &p.tx {
                    output.lost.send_replace(true);
                }
                p.departing = true;
                tracing::error!(
                    player = id,
                    "GOTO acknowledgement timed out; connection closed"
                );
            }
        }
    }
}
