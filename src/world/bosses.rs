use super::*;
use crate::condition;
use crate::encounter::patterns::{self, Effect};

const ANNOUNCE_RADIUS: f32 = 20.;
/// A blinking boss lands this many tiles from its target.
const BLINK_DISTANCE: f32 = 3.;
/// Summoned minions appear on a circle this wide around the boss.
const SUMMON_RADIUS: f32 = 2.5;

impl World {
    /// Names each bullet-hell phase as it begins (floating text over the boss and chat for nearby
    /// players), and announces the enrage below half health.
    pub(super) fn boss_phase_step(&mut self) {
        self.boss_phases
            .retain(|id, _| self.entities.contains_key(id));
        self.boss_minions
            .retain(|id, _| self.entities.contains_key(id));
        let bosses: Vec<(i32, u16, i32, Pos, u64)> = self
            .actors()
            .filter(|e| patterns::BOSS_KITS.iter().any(|(k, ..)| *k == e.kind))
            .filter_map(|e| {
                Some((
                    e.id,
                    e.kind,
                    e.hp,
                    e.pos,
                    self.now.checked_sub(e.activated?)?,
                ))
            })
            .collect();
        for (id, kind, hp, pos, elapsed) in bosses {
            let Some((index, name, _)) = patterns::phase(kind, elapsed) else {
                continue;
            };
            let effect = patterns::effect(kind, elapsed);
            // The shield lasts exactly as long as its phase.
            let shield = condition::bit(condition::INVULNERABLE);
            let e = self.entities.get_mut(&id).unwrap();
            if effect == Effect::Shield {
                e.condition |= shield;
            } else {
                e.condition &= !shield;
            }
            let enraged = patterns::enraged(kind, hp);
            let previous = self.boss_phases.insert(id, (index, enraged));
            if previous == Some((index, enraged)) {
                continue;
            }
            if previous.is_none_or(|(old, _)| old != index) {
                self.boss_phase_effect(id, kind, effect);
            }
            let boss = &self.content.objects[&kind].name;
            let title = if enraged {
                format!("{name} (enraged)")
            } else {
                name.to_string()
            };
            let mut note = Writer::new();
            note.i32(id)
                .utf(&title)
                .u32(if enraged { 0xffff4040 } else { 0xffffd84a });
            let chat = chat::chat_info(&format!("[{boss}] {title}"));
            for p in self.players.values() {
                if p.known.contains(&id) {
                    send(p, wire::NOTIFICATION, note.0.clone());
                }
                if p.pos.distance(pos) <= ANNOUNCE_RADIUS {
                    send(p, chat.id, chat.body.clone());
                }
            }
            tracing::info!(boss = id, kind, phase = name, enraged, "boss phase began");
        }
    }

    /// What a boss does to itself as a phase begins: blink, mend or summon.
    fn boss_phase_effect(&mut self, id: i32, kind: u16, effect: Effect) {
        let pos = self.entities[&id].pos;
        match effect {
            Effect::None | Effect::Shield => {}
            Effect::Blink => {
                let Some(target) = self
                    .players
                    .values()
                    .map(|p| p.pos)
                    .filter(|p| p.distance(pos) <= ANNOUNCE_RADIUS)
                    .min_by(|a, b| a.distance(pos).total_cmp(&b.distance(pos)))
                else {
                    return;
                };
                // Land between the boss and the player, trying other sides if that is a wall.
                let toward = (pos.y - target.y).atan2(pos.x - target.x);
                let landing = (0..8)
                    .map(|i| {
                        let angle = toward + i as f32 * std::f32::consts::FRAC_PI_4;
                        Pos {
                            x: target.x + BLINK_DISTANCE * angle.cos(),
                            y: target.y + BLINK_DISTANCE * angle.sin(),
                        }
                    })
                    .find(|p| self.walkable(*p));
                if let Some(landing) = landing {
                    if let Err(error) = self.position_entity(id, landing) {
                        tracing::error!(boss = id, %error, "boss blink refused");
                        return;
                    }
                    tracing::info!(boss = id, kind, ?landing, "boss blinked");
                }
            }
            Effect::Mend { percent } => {
                let Some(&(_, _, max)) = patterns::BOSS_KITS.iter().find(|(k, ..)| *k == kind)
                else {
                    return;
                };
                let e = self.entities.get_mut(&id).unwrap();
                let healed = (max * percent / 100).min(max - e.hp).max(0);
                e.hp += healed;
                let mut note = Writer::new();
                note.i32(id).utf(&format!("+{healed}")).u32(0xff40ff60);
                for p in self.players.values() {
                    if p.known.contains(&id) {
                        send(p, wire::NOTIFICATION, note.0.clone());
                    }
                }
                tracing::info!(boss = id, kind, healed, "boss mended");
            }
            Effect::Summon { minion, count } => {
                let Some(minion_kind) = self
                    .content
                    .objects
                    .iter()
                    .find(|(_, o)| o.name == minion)
                    .map(|(k, _)| *k)
                else {
                    tracing::error!(boss = id, minion, "summoned minion is not in objects.xml");
                    return;
                };
                let entities = &self.entities;
                let alive = self.boss_minions.entry(id).or_default();
                alive.retain(|m| entities.contains_key(m));
                let missing = count.saturating_sub(alive.len());
                for i in 0..missing {
                    let angle = i as f32 * std::f32::consts::TAU / missing as f32;
                    let at = Pos {
                        x: pos.x + SUMMON_RADIUS * angle.cos(),
                        y: pos.y + SUMMON_RADIUS * angle.sin(),
                    };
                    if let Ok(spawned) = self.spawn_entity(minion_kind, at, vec![]) {
                        self.boss_minions.entry(id).or_default().push(spawned);
                    }
                }
                tracing::info!(boss = id, kind, minion, missing, "boss summoned minions");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::tests::{arena, ch};

    #[test]
    fn bosses_fill_the_arena_strike_at_the_player_and_name_their_phases() {
        for &(kind, key, _) in patterns::BOSS_KITS.iter() {
            let mut w = arena(0);
            let p = w.enter(ch(&w), None).unwrap();
            let at = w.players[&p].pos;
            let boss = w
                .spawn_entity(
                    kind,
                    Pos {
                        x: at.x + 6.,
                        y: at.y,
                    },
                    vec![],
                )
                .unwrap();
            w.entities.get_mut(&boss).unwrap().hp = 1_000_000;
            let (mut most_bullets, mut strikes) = (0, 0);
            for _ in 0..400 {
                // Keep the target alive so the boss runs its whole cycle.
                w.apply_condition(p, crate::condition::INVULNERABLE, 200)
                    .unwrap();
                w.step();
                most_bullets = most_bullets.max(w.bullets.len());
                strikes = strikes.max(w.grenades.len());
            }
            assert!(
                most_bullets >= 12,
                "{key}: patterns fill the screen ({most_bullets} bullets)"
            );
            assert!(w.boss_phases.contains_key(&boss), "{key}: phase announced");
            let uses_strikes = patterns::kit(key)
                .unwrap()
                .iter()
                .flat_map(|ph| ph.patterns)
                .any(|p| matches!(p, patterns::Pattern::Strike { .. }));
            if uses_strikes {
                assert!(strikes > 0, "{key}: AoE strikes target the player");
            }
        }
    }

    fn boss_in_phase(effect: fn(&Effect) -> bool) -> (World, i32, i32, u16, u64) {
        let (kind, key, _) = *patterns::BOSS_KITS
            .iter()
            .find(|(_, key, _)| {
                patterns::kit(key)
                    .unwrap()
                    .iter()
                    .any(|ph| effect(&ph.effect))
            })
            .expect("some boss uses the effect");
        let phases = patterns::kit(key).unwrap();
        let start: u64 = phases
            .iter()
            .take_while(|ph| !effect(&ph.effect))
            .map(|ph| ph.ms)
            .sum();
        let mut w = arena(0);
        let p = w.enter(ch(&w), None).unwrap();
        let at = w.players[&p].pos;
        let boss = w
            .spawn_entity(
                kind,
                Pos {
                    x: at.x + 8.,
                    y: at.y,
                },
                vec![],
            )
            .unwrap();
        (w, p, boss, kind, start)
    }

    /// Runs the world until the boss has spent `ms` of its fight.
    fn fight_until(w: &mut World, p: i32, boss: i32, ms: u64) {
        while w.entities[&boss]
            .activated
            .is_none_or(|t| w.now.saturating_sub(t) < ms)
        {
            w.apply_condition(p, crate::condition::INVULNERABLE, 200)
                .unwrap();
            w.entities.get_mut(&boss).unwrap().hp = 1_000_000;
            w.step();
        }
    }

    #[test]
    fn shield_phases_make_the_boss_immune_only_while_they_last() {
        let (mut w, p, boss, kind, start) = boss_in_phase(|e| *e == Effect::Shield);
        let shield = condition::bit(condition::INVULNERABLE);
        fight_until(&mut w, p, boss, start.saturating_sub(500));
        assert_eq!(
            w.entities[&boss].condition & shield,
            0,
            "open before the shield"
        );
        fight_until(&mut w, p, boss, start + 500);
        assert_ne!(
            w.entities[&boss].condition & shield,
            0,
            "{kind:#x} shielded"
        );
        let ms = patterns::kit(patterns::BOSS_KITS.iter().find(|k| k.0 == kind).unwrap().1)
            .unwrap()
            .iter()
            .find(|ph| ph.effect == Effect::Shield)
            .unwrap()
            .ms;
        fight_until(&mut w, p, boss, start + ms + 500);
        assert_eq!(w.entities[&boss].condition & shield, 0, "the shield drops");
    }

    #[test]
    fn summoners_call_their_minions_and_blinkers_land_beside_the_player() {
        let (mut w, p, boss, _, start) = boss_in_phase(|e| matches!(e, Effect::Summon { .. }));
        fight_until(&mut w, p, boss, start + 200);
        let minions = w.boss_minions.get(&boss).map_or(0, Vec::len);
        assert!(minions >= 3, "minions summoned: {minions}");
        assert!(minions <= 5, "never more than the phase count");

        let (mut w, p, boss, _, start) = boss_in_phase(|e| *e == Effect::Blink);
        fight_until(&mut w, p, boss, start + 100);
        let gap = w.entities[&boss].pos.distance(w.players[&p].pos);
        assert!(
            gap < 5.,
            "the boss blinked next to the player ({gap} tiles)"
        );
    }

    #[test]
    fn feasting_bosses_mend_part_of_their_health() {
        let (mut w, p, boss, kind, start) = boss_in_phase(|e| matches!(e, Effect::Mend { .. }));
        let max = patterns::BOSS_KITS.iter().find(|k| k.0 == kind).unwrap().2;
        while w.entities[&boss]
            .activated
            .is_none_or(|t| w.now.saturating_sub(t) + 100 < start)
        {
            w.apply_condition(p, crate::condition::INVULNERABLE, 200)
                .unwrap();
            w.entities.get_mut(&boss).unwrap().hp = max / 2;
            w.step();
        }
        for _ in 0..10 {
            w.apply_condition(p, crate::condition::INVULNERABLE, 200)
                .unwrap();
            w.step();
        }
        assert!(w.entities[&boss].hp > max / 2, "the boss healed");
        assert!(w.entities[&boss].hp <= max);
    }
}
