use super::*;
use crate::augments::{AUGMENTS, Effect, level};

fn line(p: &Player, text: &str) {
    let packet = chat::chat_info(text);
    send(p, packet.id, packet.body);
}
fn describe(id: usize, next: u8) -> String {
    format!("{} lv {} — {}", AUGMENTS[id].name, next, AUGMENTS[id].text)
}
fn offer_lines(p: &Player) {
    let ch = &p.character;
    line(
        p,
        &format!(
            "Augment available! ({} to choose) Type /augment 1, 2 or 3:",
            crate::augments::pending(ch)
        ),
    );
    for (slot, id) in ch.augment_offer.iter().enumerate() {
        let id = usize::from(*id);
        line(
            p,
            &format!("{}) {}", slot + 1, describe(id, level(ch, id) + 1)),
        );
    }
}
/// Tells the player about a newly gained augment level and a newly dealt offer.
pub(super) fn announce(p: &Player, previous: &[u8], previous_offer: &[u8]) {
    let ch = &p.character;
    for id in 0..AUGMENTS.len() {
        let now = level(ch, id);
        if now > previous.get(id).copied().unwrap_or(0) {
            line(p, &format!("Augment gained: {}", describe(id, now)));
            tracing::info!(
                character = ch.id,
                augment = AUGMENTS[id].name,
                level = now,
                "augment gained"
            );
        }
    }
    if !ch.augment_offer.is_empty() && ch.augment_offer != previous_offer {
        offer_lines(p);
    }
}

/// Tells the player the card overlay preference just saved.
pub(super) fn announce_panel(p: &Player) {
    line(
        p,
        if p.character.augment_panel {
            "Augment cards enabled: they appear once you are out of combat (mouse or keys 1-3)."
        } else {
            "Augment cards disabled: choose with /augment 1, 2 or 3."
        },
    );
}
/// Calm after the last nearby enemy before the overlay may open.
const CALM_MS: u64 = 3000;
const COMBAT_RADIUS: f32 = 9.;
/// The overlay counts as present only while its page keeps polling.
const OVERLAY_ALIVE_MS: u64 = 2000;

impl World {
    pub(crate) fn augment_command(&mut self, id: i32, argument: &str) -> Result<()> {
        let p = self
            .players
            .get_mut(&id)
            .context("augment player missing")?;
        if argument.eq_ignore_ascii_case("panel") {
            ensure!(
                p.augment_panel_toggle.is_none(),
                controller("Already switching the augment cards; wait a moment.")
            );
            p.augment_panel_toggle = Some(!p.character.augment_panel);
            return Ok(());
        }
        if argument.is_empty() {
            let ch = &p.character;
            let owned: Vec<String> = (0..AUGMENTS.len())
                .filter(|a| level(ch, *a) > 0)
                .map(|a| format!("{} lv {}", AUGMENTS[a].name, level(ch, a)))
                .collect();
            line(
                p,
                &format!(
                    "Augments: {}",
                    if owned.is_empty() {
                        "none yet".into()
                    } else {
                        owned.join(", ")
                    }
                ),
            );
            let next = crate::augments::milestone(
                crate::augments::spent(ch) + crate::augments::pending(ch) + 1,
            );
            line(
                p,
                &format!("Fame {} · next augment at {} fame", ch.fame, next),
            );
            if !ch.augment_offer.is_empty() {
                offer_lines(p);
            }
            let panel = if ch.augment_panel {
                "enabled"
            } else {
                "disabled"
            };
            line(
                p,
                &format!("Augment cards: {panel} (/augment panel to toggle)"),
            );
            return Ok(());
        }
        let slot: usize = argument
            .parse::<usize>()
            .ok()
            .filter(|n| (1..=p.character.augment_offer.len()).contains(n))
            .ok_or_else(|| controller("Usage: /augment 1, 2 or 3 when an augment is available (/augments to list them)."))?;
        // Also reject while a progress save is in flight: pending_progress_where takes
        // augment_choice immediately, so is_none alone leaves a window where a second pick
        // queues against the old (or redealt) offer before finish_progress publishes.
        ensure!(
            p.augment_choice.is_none() && !p.pending,
            controller("Already applying an augment; wait a moment.")
        );
        p.augment_choice = Some(slot - 1);
        tracing::info!(player = id, slot, "augment chosen");
        Ok(())
    }
    fn enemy_near(&self, pos: Pos, radius: f32) -> bool {
        // Spatial scan — panel_step only needs emptiness; avoid a full-map actors() walk.
        // actors_near uses strict <; query radius+1 then keep the historical inclusive <=.
        self.actors_near(pos, radius + 1.).any(|e| {
            e.hp > 0
                && self.content.objects[&e.kind].enemy
                && self.content.objects[&e.kind].class != "Wall"
                && e.pos.distance(pos) <= radius
        })
    }
    fn augment_targets(&self, pos: Pos, radius: f32) -> Vec<i32> {
        self.actors_near(pos, radius + 1.)
            .filter(|e| {
                e.hp > 0
                    && self.content.objects[&e.kind].enemy
                    && self.content.objects[&e.kind].class != "Wall"
                    && e.pos.distance(pos) <= radius
            })
            .map(|e| e.id)
            .collect()
    }
    fn augment_effect(&self, kind: u8, owner: i32, a: Pos, b: Pos, color: u32) {
        let mut out = Writer::new();
        out.u8(kind)
            .i32(owner)
            .f32(a.x)
            .f32(a.y)
            .f32(b.x)
            .f32(b.y)
            .u32(color);
        for (pid, p) in &self.players {
            if *pid == owner || p.known.contains(&owner) {
                send(p, wire::SHOW_EFFECT, out.0.clone());
            }
        }
    }
    /// Card overlay for opted-in players with a live overlay page: opens only after combat,
    /// hides if combat resumes, and keeps its owner invulnerable for as long as it is shown.
    fn panel_step(&mut self) -> Result<()> {
        let ids: Vec<i32> = self.players.keys().copied().collect();
        for id in ids {
            let pos = self.players[&id].pos;
            let fighting = self.enemy_near(pos, COMBAT_RADIUS);
            let now = self.now;
            let p = self.players.get_mut(&id).unwrap();
            if fighting {
                p.last_combat_at = now;
            }
            let overlay = p
                .panel_seen_at
                .is_some_and(|seen| now <= seen + OVERLAY_ALIVE_MS);
            let wants = p.character.augment_panel
                && overlay
                && p.active()
                && (!p.character.augment_offer.is_empty() || p.augment_choice.is_some());
            let calm = now >= p.last_combat_at + CALM_MS;
            if wants && calm && !p.panel_open {
                p.panel_open = true;
                tracing::info!(player = id, "augment panel opened after combat");
            } else if p.panel_open && (!wants || fighting) {
                p.panel_open = false;
                if fighting {
                    tracing::info!(
                        player = id,
                        "augment panel hidden: combat resumed while it was shown"
                    );
                }
            }
            if self.players[&id].panel_open {
                self.apply_condition(id, crate::condition::INVULNERABLE, 250)?;
            }
        }
        Ok(())
    }
    /// What the overlay page shows for this player; each call proves the overlay is alive.
    pub(crate) fn panel_state(&mut self, id: i32) -> serde_json::Value {
        let now = self.now;
        let Some(p) = self.players.get_mut(&id) else {
            return serde_json::json!({"open": false});
        };
        p.panel_seen_at = Some(now);
        let ch = &p.character;
        let cards: Vec<_> = ch
            .augment_offer
            .iter()
            .enumerate()
            .map(|(slot, a)| {
                let a = usize::from(*a);
                let augment = &AUGMENTS[a];
                serde_json::json!({"slot": slot + 1, "augment": a, "name": augment.name,
                    "text": augment.text, "level": level(ch, a) + 1, "max": augment.max,
                    "weapon": !matches!(augment.effect, Effect::Stat { .. })})
            })
            .collect();
        // Sticky choosing while p.pending: after take() and before finish_progress the live
        // offer is still the old cards but augment_choice is None — without this the overlay
        // would show them clickable again.
        serde_json::json!({"open": p.panel_open, "enabled": ch.augment_panel,
            "choosing": p.augment_choice.is_some() || p.pending,
            "pending": crate::augments::pending(ch), "cards": cards})
    }
    /// Swarm-style weapons fire on their own; stat augments live in Character::effective.
    pub(super) fn augments_step(&mut self) -> Result<()> {
        self.panel_step()?;
        let ids: Vec<i32> = self.players.keys().copied().collect();
        for id in ids {
            let p = &self.players[&id];
            if !p.active() || p.paused || p.pending || p.character.augments.is_empty() {
                continue;
            }
            let (pos, levels) = (p.pos, p.character.augments.clone());
            let phase = self.now + (id as u64 % 997) * 50;
            let due = move |period: u64| phase % period < 50;
            for (augment, lvl) in levels.iter().enumerate().filter(|(_, l)| **l > 0) {
                let lvl = i32::from(*lvl);
                match AUGMENTS[augment].effect {
                    Effect::Stat { .. } => {}
                    Effect::Orb if due(1500) => {
                        let radius = 2.5;
                        let targets = self.augment_targets(pos, radius);
                        if !targets.is_empty() {
                            self.augment_effect(
                                8,
                                id,
                                pos,
                                Pos {
                                    x: pos.x + radius,
                                    y: pos.y,
                                },
                                0xffb070ff,
                            );
                        }
                        for target in targets {
                            self.damage_enemy(id, target, 30 + 25 * lvl, false, &[], None)?;
                        }
                    }
                    Effect::Chain if due(1800) => {
                        let mut from = pos;
                        let mut hit = Vec::new();
                        for jump in 0..=lvl {
                            let reach = if jump == 0 { 7. } else { 4. };
                            let next = self
                                .augment_targets(from, reach)
                                .into_iter()
                                .filter(|t| !hit.contains(t))
                                .min_by(|a, b| {
                                    let (da, db) = (
                                        self.entities[a].pos.distance(from),
                                        self.entities[b].pos.distance(from),
                                    );
                                    da.total_cmp(&db)
                                });
                            let Some(target) = next else { break };
                            let to = self.entities[&target].pos;
                            self.augment_effect(7, id, to, from, 0xff7fe3ff);
                            self.damage_enemy(id, target, 40 + 30 * lvl, false, &[], None)?;
                            hit.push(target);
                            from = to;
                        }
                    }
                    Effect::Frost if due(4000) => {
                        let radius = 3. + lvl as f32;
                        let targets = self.augment_targets(pos, radius);
                        if !targets.is_empty() {
                            self.augment_effect(10, id, Pos { x: radius, y: 0. }, pos, 0xffaee8ff);
                        }
                        for target in targets {
                            self.apply_condition(target, crate::condition::SLOWED, 2000)?;
                        }
                    }
                    Effect::Blessing if due(4000) => {
                        self.augment_effect(5, id, Pos { x: 4., y: 0. }, pos, 0xff60e070);
                        let allies: Vec<i32> = self
                            .players
                            .iter()
                            .filter(|(_, o)| o.active() && o.pos.distance(pos) <= 4.)
                            .map(|(oid, _)| *oid)
                            .collect();
                        for ally in allies {
                            let max = self.players[&ally].effective(&self.content, self.now)?[0];
                            let o = self.players.get_mut(&ally).unwrap();
                            let healed = (max * 5 * lvl / 100).min(max - o.character.hp).max(0);
                            if healed > 0 {
                                o.character.hp += healed;
                                let mut out = Writer::new();
                                out.i32(ally).utf(&format!("+{healed}")).u32(0xff40ff40);
                                for viewer in self.players.values() {
                                    if viewer.known.contains(&ally)
                                        || viewer.character.id == self.players[&ally].character.id
                                    {
                                        send(viewer, wire::NOTIFICATION, out.0.clone());
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::tests::{arena, ch};

    fn augmented(levels: &[(usize, u8)]) -> (World, i32) {
        let mut w = arena(0);
        let mut c = ch(&w);
        c.fame = i32::MAX;
        for (id, lvl) in levels {
            if c.augments.len() <= *id {
                c.augments.resize(id + 1, 0);
            }
            c.augments[*id] = *lvl;
        }
        let p = w.enter(c, None).unwrap();
        (w, p)
    }

    #[test]
    fn orb_and_chain_damage_enemies_and_frost_slows_them() {
        let (mut w, p) = augmented(&[(8, 3), (9, 2), (10, 1)]);
        let at = w.players[&p].pos;
        let near = w
            .spawn_entity(
                0x600,
                Pos {
                    x: at.x + 1.,
                    y: at.y,
                },
                vec![],
            )
            .unwrap();
        w.entities.get_mut(&near).unwrap().hp = 100_000;
        let far = w
            .spawn_entity(
                0x600,
                Pos {
                    x: at.x + 5.,
                    y: at.y,
                },
                vec![],
            )
            .unwrap();
        w.entities.get_mut(&far).unwrap().hp = 100_000;
        for _ in 0..90 {
            w.now += 50;
            w.augments_step().unwrap();
        }
        assert!(
            w.entities[&near].hp < 100_000,
            "orb and chain reach the adjacent enemy"
        );
        assert!(
            w.entities[&far].hp < 100_000,
            "the chain jumps beyond the orb radius"
        );
        assert!(
            w.entities[&near].conditions(w.now) & crate::condition::bit(crate::condition::SLOWED)
                != 0
                || {
                    w.now += 50;
                    (0..80).any(|_| {
                        w.now += 50;
                        w.augments_step().unwrap();
                        w.entities[&near].conditions(w.now)
                            & crate::condition::bit(crate::condition::SLOWED)
                            != 0
                    })
                }
        );
    }

    #[test]
    fn stat_augments_and_blessing_reach_live_stats() {
        let (mut w, p) = augmented(&[(0, 2), (2, 3), (11, 2)]);
        let values = w.players[&p].effective(&w.content, w.now).unwrap();
        let base = w.players[&p].character.base;
        assert_eq!(values[0] - base[0], 40, "Vitality adds max HP");
        assert_eq!(values[2] - base[2], 6, "Fury adds attack");
        w.players.get_mut(&p).unwrap().character.hp = 10;
        for _ in 0..90 {
            w.now += 50;
            w.augments_step().unwrap();
        }
        assert!(w.players[&p].character.hp > 10, "Blessing heals its owner");
    }

    #[test]
    fn augment_command_validates_slots_and_progress_applies_the_pick() {
        let mut w = arena(0);
        let mut c = ch(&w);
        c.exp = 60_000; // fame follows experience (exp / 1000)
        c.fame = 60;
        let p = w.enter(c, None).unwrap();
        assert!(
            w.augment_command(p, "1").is_err(),
            "no offer has been dealt yet"
        );
        let plans = w.pending_progress_where(|_, _| true);
        let (pid, snapshot) = plans
            .into_iter()
            .next()
            .expect("a pending pick queues a save");
        assert_eq!(snapshot.augment_offer.len(), crate::augments::OFFER);
        w.finish_progress(pid, snapshot).unwrap();
        assert!(w.augment_command(p, "4").is_err());
        w.augment_command(p, "").unwrap();
        let chosen = usize::from(w.players[&p].character.augment_offer[1]);
        w.augment_command(p, "2").unwrap();
        let (pid, snapshot) = w
            .pending_progress_where(|_, _| true)
            .into_iter()
            .next()
            .unwrap();
        w.finish_progress(pid, snapshot).unwrap();
        assert_eq!(level(&w.players[&p].character, chosen), 1);
        assert_eq!(
            w.players[&p].character.augment_offer.len(),
            crate::augments::OFFER,
            "second pick dealt"
        );
    }

    fn invulnerable(w: &World, p: i32) -> bool {
        w.players[&p].conditions(w.now) & crate::condition::bit(crate::condition::INVULNERABLE) != 0
    }
    fn tick(w: &mut World, ms: u64) {
        for _ in 0..ms / 50 {
            w.now += 50;
            w.augments_step().unwrap();
        }
    }

    #[test]
    fn in_flight_pick_rejects_a_second_choice_and_keeps_choosing_sticky() {
        let mut w = arena(0);
        let mut c = ch(&w);
        c.exp = 60_000;
        c.fame = 60;
        let p = w.enter(c, None).unwrap();
        let (pid, snapshot) = w
            .pending_progress_where(|_, _| true)
            .into_iter()
            .next()
            .unwrap();
        w.finish_progress(pid, snapshot).unwrap();
        assert_eq!(
            w.players[&p].character.augment_offer.len(),
            crate::augments::OFFER
        );
        w.augment_command(p, "1").unwrap();
        let (pid, snapshot) = w
            .pending_progress_where(|_, _| true)
            .into_iter()
            .next()
            .unwrap();
        // Between take and finish: choice is gone, offer still old, progress pending.
        assert!(w.players[&p].pending);
        assert!(w.players[&p].augment_choice.is_none());
        assert!(
            w.augment_command(p, "2").is_err(),
            "second pick must not queue while the first save is in flight"
        );
        let state = w.panel_state(p);
        assert_eq!(
            state["choosing"], true,
            "overlay stays busy so cards are not clickable during the gap"
        );
        assert_eq!(
            state["cards"].as_array().unwrap().len(),
            crate::augments::OFFER
        );
        w.finish_progress(pid, snapshot).unwrap();
        assert!(!w.players[&p].pending);
        assert_eq!(w.panel_state(p)["choosing"], false);
    }

    #[test]
    fn card_panel_needs_opt_in_and_a_live_overlay_and_waits_for_calm() {
        let mut w = arena(0);
        let mut c = ch(&w);
        c.exp = 60_000;
        c.fame = 60;
        let p = w.enter(c, None).unwrap();
        let (pid, snapshot) = w
            .pending_progress_where(|_, _| true)
            .into_iter()
            .next()
            .unwrap();
        w.finish_progress(pid, snapshot).unwrap();
        w.panel_state(p);
        tick(&mut w, 4000);
        assert!(!w.players[&p].panel_open, "default is chat only");
        w.augment_command(p, "panel").unwrap();
        let (pid, snapshot) = w
            .pending_progress_where(|_, _| true)
            .into_iter()
            .next()
            .unwrap();
        w.finish_progress(pid, snapshot).unwrap();
        assert!(
            w.players[&p].character.augment_panel,
            "preference persisted by the save"
        );
        tick(&mut w, 4000);
        assert!(!w.players[&p].panel_open, "no overlay page is polling");
        let at = w.players[&p].pos;
        let enemy = w
            .spawn_entity(
                0x600,
                Pos {
                    x: at.x + 2.,
                    y: at.y,
                },
                vec![],
            )
            .unwrap();
        w.panel_state(p);
        tick(&mut w, 1000);
        assert!(!w.players[&p].panel_open, "never opens while fighting");
        w.remove_entity(enemy);
        w.panel_state(p);
        tick(&mut w, 1500);
        assert!(!w.players[&p].panel_open, "waits for calm after combat");
        w.panel_state(p);
        tick(&mut w, 1600);
        assert!(w.players[&p].panel_open, "opens once combat is over");
        assert!(
            invulnerable(&w, p),
            "invulnerable while the cards are shown"
        );
        assert_eq!(
            w.panel_state(p)["cards"].as_array().unwrap().len(),
            crate::augments::OFFER
        );
        w.spawn_entity(
            0x600,
            Pos {
                x: at.x + 2.,
                y: at.y,
            },
            vec![],
        )
        .unwrap();
        tick(&mut w, 50);
        assert!(
            !w.players[&p].panel_open,
            "fallback: combat hides the cards at once"
        );
        tick(&mut w, 300);
        assert!(!invulnerable(&w, p), "immunity lasts only while shown");
    }
}
