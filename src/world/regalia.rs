use super::*;
use crate::regalia::{CROWNS, RANKS};

/// Rank insignia and crown objects that follow each ranked player in this world.
#[derive(Default)]
pub(super) struct Followers(BTreeMap<i32, Badges>);
#[derive(Default)]
struct Badges {
    insignia: Option<(i32, u16)>,
    crown: Option<(i32, u16)>,
    rank: Option<usize>,
}
const INSIGNIA_OFFSET: (f32, f32) = (0.55, 0.0);

impl World {
    fn unix_now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }
    /// Keeps one follower object of `kind` beside `at`, replacing it when the kind changes.
    fn follow(
        &mut self,
        slot: Option<(i32, u16)>,
        kind: Option<u16>,
        at: Pos,
    ) -> Result<Option<(i32, u16)>> {
        let at = if self.map.cell(at.x, at.y).is_some() {
            at
        } else {
            return Ok(slot);
        };
        match (slot, kind) {
            (Some((id, current)), Some(want))
                if current == want && self.entities.contains_key(&id) =>
            {
                if self.entities[&id].pos.distance(at) > 0.001 {
                    self.position_entity(id, at)?;
                }
                Ok(slot)
            }
            (slot, want) => {
                if let Some((id, _)) = slot.filter(|(id, _)| self.entities.contains_key(id)) {
                    self.remove_entity(id);
                }
                match want {
                    Some(kind) => Ok(Some((self.insert_entity(kind, at, vec![])?, kind))),
                    None => Ok(None),
                }
            }
        }
    }
    fn regalia_send(&self, subject: i32, packet: u8, body: &[u8]) {
        for (id, p) in &self.players {
            if *id == subject || p.known.contains(&subject) {
                send(p, packet, body.to_vec());
            }
        }
    }
    pub(super) fn regalia_step(&mut self) -> Result<()> {
        let gone: Vec<i32> = self
            .regalia
            .0
            .keys()
            .filter(|id| !self.players.contains_key(id))
            .copied()
            .collect();
        for id in gone {
            let badges = self.regalia.0.remove(&id).unwrap();
            for (entity, _) in [badges.insignia, badges.crown].into_iter().flatten() {
                if self.entities.contains_key(&entity) {
                    self.remove_entity(entity);
                }
            }
        }
        let standings = self.standings.clone();
        let reign = Self::unix_now().saturating_sub(standings.reign_since);
        let ids: Vec<i32> = self.players.keys().copied().collect();
        for id in ids {
            let p = &self.players[&id];
            let (account, pos, fame) = (p.character.account, p.pos, p.character.fame);
            let rank = standings
                .positions
                .get(&account)
                .map(|n| crate::regalia::rank(*n));
            let crowned = standings.leader == Some(account);
            let level = crate::regalia::crown_level(reign);
            let mut badges = self.regalia.0.remove(&id).unwrap_or_default();
            let beside = Pos {
                x: pos.x + INSIGNIA_OFFSET.0,
                y: pos.y + INSIGNIA_OFFSET.1,
            };
            badges.insignia =
                self.follow(badges.insignia, rank.map(|r| RANKS[r].insignia), beside)?;
            badges.crown = self.follow(badges.crown, crowned.then_some(CROWNS[level]), pos)?;
            if rank != badges.rank {
                if let Some(r) = rank {
                    let mut out = Writer::new();
                    out.i32(id)
                        .utf(RANKS[r].name)
                        .u32(0xff00_0000 | RANKS[r].color);
                    self.regalia_send(id, wire::NOTIFICATION, &out.0);
                    tracing::info!(player = id, account, rank = RANKS[r].name, "rank shown");
                }
                badges.rank = rank;
            }
            self.regalia.0.insert(id, badges);
            // Staggered per player so auras pulse independently; effects need no follower object.
            let phase = self.now + (id as u64 % 997) * 50;
            if let Some(aura) = crate::regalia::aura(fame)
                && phase % aura.every_ms < 50
            {
                let mut out = Writer::new();
                out.u8(5)
                    .i32(id)
                    .f32(aura.radius)
                    .f32(0.)
                    .f32(0.)
                    .f32(0.)
                    .u32(aura.color);
                self.regalia_send(id, wire::SHOW_EFFECT, &out.0);
            }
            if crowned && phase % crate::regalia::sparkle_every_ms(level) < 50 {
                let mut out = Writer::new();
                out.u8(5)
                    .i32(id)
                    .f32(0.4 + 0.12 * level as f32)
                    .f32(0.)
                    .f32(0.)
                    .f32(0.)
                    .u32(0xffd84a);
                self.regalia_send(id, wire::SHOW_EFFECT, &out.0);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regalia::Standings;
    use crate::world::tests::{arena, ch};

    #[test]
    fn ranked_players_carry_insignia_and_the_leader_a_crown_until_they_leave() {
        let mut w = arena(0);
        let a = w.enter(ch(&w), None).unwrap();
        let mut other = ch(&w);
        other.account += 1;
        other.id += 1;
        other.fame = 600;
        let b = w.enter(other, None).unwrap();
        let (acc_a, acc_b) = (
            w.players[&a].character.account,
            w.players[&b].character.account,
        );
        w.standings = Arc::new(Standings {
            positions: [(acc_b, 1), (acc_a, 9)].into(),
            leader: Some(acc_b),
            reign_since: World::unix_now() - 1000,
        });
        w.regalia_step().unwrap();
        let kinds = |w: &World| -> Vec<u16> { w.entities.values().map(|e| e.kind).collect() };
        assert!(kinds(&w).contains(&RANKS[0].insignia));
        assert!(kinds(&w).contains(&RANKS[crate::regalia::rank(9)].insignia));
        assert!(kinds(&w).contains(&CROWNS[crate::regalia::crown_level(1000)]));
        let crown = w.regalia.0[&b].crown.unwrap().0;
        let target = Pos { x: 4.5, y: 4.5 };
        w.players.get_mut(&b).unwrap().pos = target;
        w.regalia_step().unwrap();
        assert!(
            w.entities[&crown].pos.distance(target) < 0.001,
            "crown follows its wearer"
        );
        w.standings = Arc::new(Standings {
            positions: [(acc_a, 1), (acc_b, 2)].into(),
            leader: Some(acc_a),
            reign_since: World::unix_now(),
        });
        w.regalia_step().unwrap();
        assert!(
            !w.entities.contains_key(&crown),
            "a dethroned player loses the crown"
        );
        assert_eq!(w.regalia.0[&a].crown.map(|c| c.1), Some(CROWNS[0]));
        w.leave(b).unwrap();
        w.regalia_step().unwrap();
        assert!(!w.regalia.0.contains_key(&b));
        assert_eq!(
            w.entities
                .values()
                .filter(|e| RANKS.iter().any(|r| r.insignia == e.kind))
                .count(),
            1,
            "followers of departed players are removed"
        );
    }
}
