use super::*;
use crate::encounter::attacks::{Action, Aim, Prediction, Schedule};

impl World {
    pub(super) fn suspend_npc(&mut self, id: i32, delta: u64) {
        let e = self.entities.get_mut(&id).expect("suspended NPC missing");
        if let Some(activated) = &mut e.activated {
            *activated += delta;
        }
        if e.next_shot >= self.now.saturating_sub(delta) {
            e.next_shot += delta;
        }
        if e.next_aux >= self.now.saturating_sub(delta) {
            e.next_aux += delta;
        }
        if let Some(since) = &mut e.blocked_since {
            *since += delta;
        }
        if e.reaction_until > self.now.saturating_sub(delta) {
            e.reaction_until += delta;
        }
        e.attacks.suspend(delta);
    }
    pub(super) fn legacy_enemy_shots(
        &self,
        id: i32,
        target: Pos,
        elapsed: u64,
    ) -> Result<Vec<(u8, f32)>> {
        let e = &self.entities[&id];
        let kind = e.kind;
        let heading = (target.y - e.pos.y).atan2(target.x - e.pos.x);
        if e.quest.is_some() {
            self.quest_shots(id, target, elapsed)
        } else if crate::endgame::ROSTER.contains(&kind) {
            self.finale_shots(id, heading, elapsed)
        } else if matches!(kind, 0x900..=0x902) {
            self.shield_shots(id, heading)
        } else if gods::GODS.contains(&kind)
            || gods::CONSTRUCTS.contains(&kind)
            || midlands::AIMED.contains(&kind)
        {
            self.god_shots(id, target, heading, elapsed)
        } else {
            let elapsed = if kind == 0x909 && e.stage == Some(1) {
                5000
            } else {
                elapsed
            };
            Ok(crate::encounter::phased_attacks(
                kind, e.hp, heading, elapsed,
            ))
        }
    }
    pub(super) fn scheduled_attacks(
        &mut self,
        id: i32,
        target: Option<Pos>,
        schedule: &Schedule,
    ) -> Result<Vec<(u8, f32)>> {
        let due = self
            .entities
            .get_mut(&id)
            .context("scheduled attacker missing")?
            .attacks
            .due(schedule, self.now);
        let pos = self.entities[&id].pos;
        let mut shots = vec![];
        for slot in due {
            let attack = schedule.attacks[slot];
            let heading = match attack.aim {
                Aim::Fixed(angle) => angle,
                Aim::Target => {
                    let Some(target) = target.filter(|p| pos.distance(*p) <= attack.range) else {
                        continue;
                    };
                    (target.y - pos.y).atan2(target.x - pos.x)
                }
            };
            match attack.action {
                Action::Grenade { radius, damage } => {
                    self.throw_grenade(
                        id,
                        target.context("targeted grenade has no target")?,
                        radius,
                        damage,
                    )?;
                }
                Action::Shoot { projectile, choice } => {
                    let projectile = match choice {
                        Some(other) if self.rng.gen_bool(0.5) => other,
                        _ => projectile,
                    };
                    let mut volley = attack.shots(projectile, heading);
                    let factor = match attack.predict {
                        Prediction::None => 0.,
                        Prediction::Lead(factor) => factor,
                        Prediction::Chance(chance) => {
                            if self.rng.gen_bool(chance) {
                                1.
                            } else {
                                0.
                            }
                        }
                    };
                    if factor > 0. {
                        self.predict_enemy_shots(
                            id,
                            target.context("predictive attack has no target")?,
                            &mut volley,
                            |_| factor,
                        )?;
                    }
                    shots.extend(volley);
                }
            }
            tracing::trace!(
                entity = id,
                slot,
                now = self.now,
                "NPC weapon deadline fired"
            );
        }
        Ok(shots)
    }
}
