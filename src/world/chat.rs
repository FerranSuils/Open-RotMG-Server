use super::*;

pub(super) struct ChatBudget {
    updated: Instant,
    tokens: f32,
}

pub(crate) fn chat_private(text: &str) -> Packet {
    text_packet("*Discord*", -1, 0, text)
}

pub(crate) fn chat_error(reason: &str) -> Packet {
    text_packet("*Error*", -1, 0, reason)
}

fn text_packet(name: &str, object: i32, bubble: u8, text: &str) -> Packet {
    let mut out = Writer::new();
    out.utf(name)
        .i32(object)
        .i32(0)
        .u8(bubble)
        .utf("")
        .utf(text)
        .utf("");
    Packet {
        id: wire::TEXT,
        body: out.0,
    }
}

fn validate(text: &str) -> Result<()> {
    ensure!(!text.trim().is_empty(), "Chat message is empty.");
    ensure!(
        text.encode_utf16().count() <= 128,
        "Chat messages are limited to 128 characters."
    );
    ensure!(
        !text
            .chars()
            .any(|c| c.is_control()
                || matches!(c, '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}')),
        "Chat message contains control characters."
    );
    Ok(())
}

impl Directory {
    pub fn chat(&mut self, id: i32, text: String, now: Instant) -> Result<()> {
        validate(&text)?;
        let world = self.resolve(Target::Player(id))?;
        let sender = &world.players[&id];
        ensure!(
            sender.character.alive
                && sender.character.hp > 0
                && !sender.death_pending
                && !sender.departing
                && sender.persistence_failure.is_none(),
            "Dead, departing or quarantined players cannot chat."
        );
        if text.trim().eq_ignore_ascii_case("/pause") {
            let paused = !sender.paused;
            if paused {
                // Historical reference-server policy; see docs/pause.md.
                ensure!(
                    !world.entities.values().any(|e| e.hp > 0
                        && world.content.objects[&e.kind].enemy
                        && sender.pos.distance(e.pos) <= 8.),
                    "Not safe enough to pause!"
                );
            }
            let p = world.players.get_mut(&id).unwrap();
            p.paused = paused;
            p.last_move_at = world.now;
            let reply = text_packet(
                "",
                -1,
                0,
                if paused {
                    "Game paused."
                } else {
                    "Game resumed."
                },
            );
            send(p, reply.id, reply.body);
            tracing::info!(player = id, paused, "player pause changed");
            return Ok(());
        }
        ensure!(
            !text.trim_start().starts_with('/'),
            "Unknown chat command. Use /pause to pause or resume."
        );
        let account = sender.character.account;
        let packet = text_packet(&sender.character.name, id, 5, &text);
        // Account budgets survive world transfers and reconnects without touching durable game saves.
        self.chat_budgets
            .retain(|_, b| now.saturating_duration_since(b.updated).as_secs() < 3);
        ensure!(
            self.chat_budgets.len() < 4096 || self.chat_budgets.contains_key(&account),
            "Chat rate registry is full."
        );
        let budget = self.chat_budgets.entry(account).or_insert(ChatBudget {
            updated: now,
            tokens: 3.,
        });
        let elapsed = now
            .checked_duration_since(budget.updated)
            .context("chat receive clock regressed")?;
        budget.tokens = (budget.tokens + elapsed.as_secs_f32()).min(3.);
        budget.updated = now;
        ensure!(
            budget.tokens >= 1.,
            "Chat rate limit: wait one second before sending again."
        );
        budget.tokens -= 1.;
        let world = self.resolve(Target::Player(id))?;
        let mut recipients = 0;
        for player in world.players.values().filter(|p| {
            p.character.alive
                && p.character.hp > 0
                && !p.death_pending
                && !p.departing
                && p.persistence_failure.is_none()
        }) {
            send(player, packet.id, packet.body.clone());
            recipients += 1;
        }
        tracing::info!(player=id, account, world=%world.name, recipients, bytes=text.len(), "local chat delivered");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::Reader;

    fn receive(d: &mut Directory, world: i32, account: i32) -> (i32, mpsc::Receiver<Packet>) {
        let w = d.worlds.get_mut(&world).unwrap();
        let mut ch = super::super::tests::ch(w);
        ch.id = account;
        ch.account = account;
        ch.name = format!("Guest{account}");
        let (packets, mut rx) = mpsc::channel(64);
        let (lost, _) = watch::channel(false);
        let id = w.enter(ch, Some(Output { packets, lost })).unwrap();
        while rx.try_recv().is_ok() {}
        (id, rx)
    }
    fn message(packet: Packet, name: &str, id: i32, text: &str) {
        assert_eq!(packet.id, wire::TEXT);
        let mut r = Reader::new(&packet.body);
        assert_eq!(r.utf().unwrap(), name);
        assert_eq!(r.i32().unwrap(), id);
        assert_eq!(r.i32().unwrap(), 0);
        assert_eq!(r.u8().unwrap(), if id < 0 { 0 } else { 5 });
        assert_eq!(r.utf().unwrap(), "");
        assert_eq!(r.utf().unwrap(), text);
        assert_eq!(r.utf().unwrap(), "");
        r.done().unwrap();
    }
    #[test]
    fn selected_chat_layout_identity_and_world_routing() {
        let mut d = super::super::tests::directory();
        let (a, mut ar) = receive(&mut d, -2, 1);
        let (_, mut br) = receive(&mut d, -2, 2);
        let (_, mut other) = receive(&mut d, -3, 3);
        d.chat(a, "<Guest2>: café & hello".into(), Instant::now())
            .unwrap();
        message(
            ar.try_recv().unwrap(),
            "Guest1",
            a,
            "<Guest2>: café & hello",
        );
        message(
            br.try_recv().unwrap(),
            "Guest1",
            a,
            "<Guest2>: café & hello",
        );
        assert!(other.try_recv().is_err());
        message(chat_error("Wait."), "*Error*", -1, "Wait.");
    }
    #[test]
    fn lifecycle_and_world_changes_do_not_replay_or_leak_chat() {
        let mut d = super::super::tests::directory();
        let now = Instant::now();
        let (a, mut ar) = receive(&mut d, -2, 1);
        let (b, mut br) = receive(&mut d, -2, 2);
        d.worlds
            .get_mut(&-2)
            .unwrap()
            .players
            .get_mut(&b)
            .unwrap()
            .departing = true;
        d.chat(a, "old world".into(), now).unwrap();
        assert!(ar.try_recv().is_ok());
        assert!(br.try_recv().is_err());
        d.worlds.get_mut(&-2).unwrap().players.remove(&b);
        let (b, mut br) = receive(&mut d, -3, 2);
        assert!(br.try_recv().is_err());
        d.chat(b, "new world".into(), now).unwrap();
        assert!(br.try_recv().is_ok());
        assert!(ar.try_recv().is_err());
        for mode in 0..3 {
            let p = d.worlds.get_mut(&-3).unwrap().players.get_mut(&b).unwrap();
            p.character.alive = mode != 0;
            p.death_pending = mode == 1;
            p.persistence_failure = (mode == 2).then(|| "uncertain save".into());
            assert!(d.chat(b, "unsafe sender".into(), now).is_err());
            assert!(br.try_recv().is_err());
        }
    }
    #[test]
    fn chat_validation_utf16_boundary_and_commands_never_broadcast() {
        let mut d = super::super::tests::directory();
        let (a, mut rx) = receive(&mut d, -2, 1);
        for text in [
            "",
            "  ",
            "\nGuest2: spoof",
            "hello\0",
            "\u{202e}spoof",
            "\u{2028}new line",
            " /tell Guest2 hi",
            "/g hi",
        ] {
            assert!(d.chat(a, text.into(), Instant::now()).is_err());
            assert!(rx.try_recv().is_err());
        }
        assert!(validate(&"a".repeat(129)).is_err());
        assert!(validate(&"😀".repeat(65)).is_err());
        validate(&"😀".repeat(64)).unwrap();
        validate(&"a".repeat(128)).unwrap();
    }
    #[test]
    fn chat_throttle_survives_transfers_reconnects_and_rejects_departed_senders() {
        let mut d = super::super::tests::directory();
        let now = Instant::now();
        let (a, mut rx) = receive(&mut d, -2, 1);
        for _ in 0..3 {
            d.chat(a, "hi".into(), now).unwrap();
        }
        assert!(d.chat(a, "burst".into(), now).is_err());
        assert_eq!(std::iter::from_fn(|| rx.try_recv().ok()).count(), 3);
        let p = d.worlds.get_mut(&-2).unwrap().players.get_mut(&a).unwrap();
        p.departing = true;
        assert!(d.chat(a, "departing".into(), now).is_err());
        d.worlds.get_mut(&-2).unwrap().players.remove(&a);
        assert!(d.chat(a, "disconnected".into(), now).is_err());
        let (b, mut rx) = receive(&mut d, -3, 1);
        assert!(d.chat(b, "reconnect".into(), now).is_err());
        assert!(rx.try_recv().is_err());
        d.chat(
            b,
            "refilled".into(),
            now + std::time::Duration::from_secs(1),
        )
        .unwrap();
        let p = d.worlds.get_mut(&-3).unwrap().players.get_mut(&b).unwrap();
        p.death_pending = true;
        assert!(
            d.chat(b, "dead".into(), now + std::time::Duration::from_secs(2))
                .is_err()
        );
    }
}
