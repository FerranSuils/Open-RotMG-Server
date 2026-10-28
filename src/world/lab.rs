use super::*;
use crate::{
    condition,
    dungeon::lab::{
        self, Act, BOSS, BUBBLE, CAGE, CAGED, CRUSHER, ENFORCER, GAS, GATE, MINI_BOT, POTION, TESLA,
        TURRET,
    },
    encounter::Motion,
};

impl World {
    pub(super) fn lab_mechanics(&mut self, id: i32, target: Option<Pos>, elapsed: u64) -> Result<()> {
        let e = &self.entities[&id];
        let (kind, hp, pos) = (e.kind, e.hp, e.pos);
        let max = self.content.objects[&kind].hp;
        match kind {
            BOSS => {
                let rage = hp * 5 < max;
                let (stage, act, gas) = lab::act(elapsed, rage);
                let e = self.entities.get_mut(&id).unwrap();
                let entered = e.stage != Some(stage);
                if entered {
                    e.stage = Some(stage);
                    e.next_aux = 0;
                    tracing::info!(entity = id, stage, ?act, "Dr Terrible phase entered");
                }
                e.condition &= !condition::bit(condition::INVINCIBLE);
                if act == Act::Hide {
                    e.condition |= condition::bit(condition::INVINCIBLE);
                }
                let emitters: Vec<_> = self
                    .actors()
                    .filter_map(|g| GAS.iter().position(|k| *k == g.kind).map(|i| (g.id, gas[i])))
                    .collect();
                for (emitter, on) in emitters {
                    self.entities.get_mut(&emitter).unwrap().stage = Some(u64::from(on));
                }
                if entered && stage == 0 {
                    self.broadcast_text("Dr Terrible", "For Science!");
                }
                if entered {
                    let bubbles: Vec<_> = self.actors().filter(|b| b.kind == BUBBLE).map(|b| b.id).collect();
                    for bubble in bubbles {
                        self.remove_entity(bubble);
                    }
                    if act == Act::Hide {
                        let home = self.entities[&id].home;
                        let bubble = self.spawn_entity(BUBBLE, home, vec![])?;
                        let e = self.entities.get_mut(&bubble).unwrap();
                        e.parent.set(Some(id), bubble, &mut self.timed_ids);
                    }
                }
                if act == Act::Hide {
                    let cage = self.actors().find(|c| c.kind == CAGE).map(|c| c.id);
                    if entered && let Some(cage) = cage {
                        for caged in CAGED {
                            self.spawn_children(cage, caged, 1, 2.)?;
                        }
                    }
                } else if self.now >= self.entities[&id].next_aux {
                    if act == Act::Potions {
                        if let Some(at) = target {
                            self.manor_toss(id, POTION, at, 2050)?;
                        }
                    } else {
                        let angle = self.rng.gen_range(0f32..std::f32::consts::TAU);
                        let at = Pos {
                            x: pos.x + 3. * angle.cos(),
                            y: pos.y + 3. * angle.sin(),
                        };
                        self.manor_toss(id, TURRET, at, 8000)?;
                    }
                    self.entities.get_mut(&id).unwrap().next_aux =
                        self.now + if rage { 1500 } else { 2000 };
                }
            }
            lab::CYBORG | 0x1e24 => {
                let stage = if hp * 5 < max {
                    2
                } else {
                    u64::from(elapsed >= 10000)
                };
                let e = self.entities.get_mut(&id).unwrap();
                if e.stage != Some(stage) {
                    e.stage = Some(stage);
                    if stage == 2 {
                        // The blinking cyborg detonates two seconds after its fuse is lit.
                        e.activated = Some(self.now);
                        e.expires.set(Some(self.now + 2050), id, &mut self.timed_ids);
                    }
                }
            }
            CRUSHER => {
                let stage = lab::crusher_stage(hp, max);
                let e = self.entities.get_mut(&id).unwrap();
                if e.stage != Some(stage) {
                    e.stage = Some(stage);
                    e.size = [100, 150, 175, 200][stage as usize];
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn lab_movement(&self, id: i32) -> (Motion, f32) {
        let e = &self.entities[&id];
        if e.kind == BOSS {
            let hiding = e.stage.is_some_and(|s| s < 1000 && s % 3 == 2);
            return lab::boss_motion(if hiding { Act::Hide } else { Act::Potions });
        }
        let p = lab::profile(e.kind).expect("validated Lab profile");
        (p.motion, p.speed)
    }
    pub(super) fn lab_death(&mut self, defeated: &Entity) -> Result<()> {
        match defeated.kind {
            TESLA if !self.actors().any(|e| e.kind == TESLA) => {
                let gate: Vec<_> = (0..self.map.cells.len())
                    .filter(|i| self.map.cells[*i].region == GATE)
                    .flat_map(|i| self.cells.ids(i).to_vec())
                    .filter(|id| self.entities[id].kind == lab::WALL)
                    .collect();
                for id in gate {
                    self.remove_entity(id);
                }
                self.broadcast_text("Dr Terrible", "No! Not my Tesla Coils!");
            }
            ENFORCER => {
                let count = self.rng.gen_range(1..=3);
                for i in 0..count {
                    let angle = i as f32 * std::f32::consts::TAU / 3.;
                    let pos = Pos {
                        x: defeated.pos.x + 0.3 * angle.cos(),
                        y: defeated.pos.y + 0.3 * angle.sin(),
                    };
                    if self.walkable(pos) {
                        self.spawn_entity(MINI_BOT, pos, vec![])?;
                    }
                }
            }
            BOSS => {
                let leftovers: Vec<_> = self
                    .actors()
                    .filter(|e| matches!(e.kind, CAGE | POTION | TURRET | BUBBLE) || GAS.contains(&e.kind))
                    .map(|e| e.id)
                    .collect();
                for id in leftovers {
                    self.remove_entity(id);
                }
                self.manor_births.retain(|t| t.parent != defeated.id);
                self.broadcast_text("Oryx the Mad God", "The doctor's experiments are over.");
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn ids(w: &World, kind: u16) -> Vec<i32> {
        w.actors().filter(|e| e.kind == kind).map(|e| e.id).collect()
    }
    fn gas_states(w: &World) -> [bool; 4] {
        GAS.map(|g| w.entities[&ids(w, g)[0]].stage == Some(1))
    }

    #[test]
    fn doctor_cycles_gas_throws_hides_behind_caged_minions_and_rages() {
        let content = Arc::new(Content::load(Path::new("data")).unwrap());
        let map = Arc::new(lab::generate(&content, 3).unwrap());
        let mut w = World::new(content, map).unwrap();
        w.name = lab::NAME.into();
        w.encounter = true;
        let p = w.enter(super::super::tests::ch(&w), None).unwrap();
        let boss = ids(&w, BOSS)[0];
        let home = w.entities[&boss].home;
        let near = Pos {
            x: home.x,
            y: home.y + 3.,
        };
        w.players.get_mut(&p).unwrap().pos = near;
        w.lab_mechanics(boss, Some(near), 0).unwrap();
        assert_eq!(w.entities[&boss].stage, Some(0));
        assert_eq!(gas_states(&w), [false, true, true, true]);
        assert_eq!(w.manor_births.iter().filter(|t| t.kind == POTION).count(), 1);
        w.now += 2000;
        w.lab_mechanics(boss, Some(near), 15000).unwrap();
        assert_eq!(w.manor_births.iter().filter(|t| t.kind == TURRET).count(), 1);
        w.lab_mechanics(boss, Some(near), 30000).unwrap();
        assert_eq!(gas_states(&w), [false; 4]);
        assert_ne!(w.entities[&boss].condition & condition::bit(condition::INVINCIBLE), 0);
        assert_eq!(ids(&w, BUBBLE).len(), 1, "hiding bubble");
        for caged in CAGED {
            assert_eq!(ids(&w, caged).len(), 1, "{caged:#x}");
        }
        assert_eq!(w.lab_movement(boss).0, lab::boss_motion(Act::Hide).0);
        w.lab_mechanics(boss, Some(near), 37000).unwrap();
        assert_eq!(gas_states(&w), [true, false, true, true]);
        assert_eq!(w.entities[&boss].condition & condition::bit(condition::INVINCIBLE), 0);
        assert!(ids(&w, BUBBLE).is_empty(), "bubble pops when he comes out");
        let gate = |w: &World| {
            w.entities
                .values()
                .filter(|e| e.kind == lab::WALL && w.map.cell(e.pos.x, e.pos.y).unwrap().region == GATE)
                .count()
        };
        assert!(gate(&w) >= 4);
        for coil in ids(&w, TESLA) {
            let hp = w.entities[&coil].hp;
            assert!(w.damage_enemy(p, coil, hp + 100, true, &[], None).unwrap() > 0);
        }
        assert_eq!(gate(&w), 0, "gate falls with the coils");
        w.entities.get_mut(&boss).unwrap().hp = 1000;
        w.lab_mechanics(boss, Some(near), 50000).unwrap();
        assert!(w.entities[&boss].stage.unwrap() >= 1000);
        assert_eq!(gas_states(&w), [false; 4]);
        let enforcers = ids(&w, ENFORCER);
        let bots = ids(&w, MINI_BOT).len();
        if let Some(enforcer) = enforcers.first() {
            let hp = w.entities[enforcer].hp;
            assert!(w.damage_enemy(p, *enforcer, hp + 1000, true, &[], None).unwrap() > 0);
            assert!(ids(&w, MINI_BOT).len() > bots);
        }
        assert!(w.damage_enemy(p, boss, 5000, true, &[], None).unwrap() > 0);
        assert!(w.cleared);
        assert_eq!(w.entities[&w.clear_exit.unwrap()].kind, 0x71d);
        assert!(ids(&w, CAGE).is_empty() && ids(&w, GAS[0]).is_empty());
    }
}
