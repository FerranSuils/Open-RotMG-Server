use super::*;
use crate::encounter::{Anchor, Motion};

pub(super) const KINDS: [u16; 4] = [0x609, 0x605, 0x606, 0x607];
const MEMBERS: [u16; 8] = [0x609, 0x605, 0x605, 0x606, 0x606, 0x606, 0x606, 0x607];
const ROLES: [(usize, usize, u64); 3] = [(1, 3, 15000), (3, 7, 7000), (7, 8, 8000)];

pub(super) struct Camp {
    pub start: usize,
    ready: u64,
    next: [u64; 3],
}

fn formation(center: Pos) -> [Pos; 8] {
    std::array::from_fn(|i| {
        if i == 0 {
            center
        } else {
            let angle = (i - 1) as f32 * std::f32::consts::TAU / 7.;
            Pos {
                x: center.x + 2. * angle.cos(),
                y: center.y + 2. * angle.sin(),
            }
        }
    })
}

impl World {
    fn elf_ground(&self, pos: Pos) -> bool {
        self.walkable(pos)
            && self
                .map
                .cell(pos.x, pos.y)
                .is_some_and(|cell| cell.terrain == 10 && cell.object.is_none())
    }

    fn place_elf(&mut self, camp: usize, slot: usize, pos: Pos) -> Result<()> {
        let id = self.spawn_entity(self.population[slot].kind, pos, vec![])?;
        let home = self.population[self.elf_camps[camp].start].pos;
        let e = self.entities.get_mut(&id).unwrap();
        // Ordinary followers must survive leader loss; summon parenthood deletes its children.
        e.elf_camp = Some(camp);
        e.home = home;
        self.population[slot].entity = Some(id);
        tracing::debug!(camp, entity = id, kind = e.kind, "Elf camp member created");
        Ok(())
    }

    pub(super) fn populate_elves(&mut self, candidates: Vec<Pos>) -> Result<()> {
        for center in candidates {
            let positions = formation(center);
            if !positions.iter().all(|p| self.elf_ground(*p))
                || self.population.iter().any(|s| s.pos.distance(center) < 8.)
            {
                continue;
            }
            let camp = self.elf_camps.len();
            let start = self.population.len();
            self.elf_camps.push(Camp {
                start,
                ready: 0,
                next: ROLES.map(|(_, _, delay)| self.now + delay),
            });
            for (kind, pos) in MEMBERS.into_iter().zip(positions) {
                let slot = self.population.len();
                self.population.push(Spawn {
                    kind,
                    pos,
                    entity: None,
                    ready: 0,
                });
                self.place_elf(camp, slot, pos)?;
            }
            if self.elf_camps.len() == 4 {
                break;
            }
        }
        tracing::info!(
            camps = self.elf_camps.len(),
            reserved = self.elf_camps.len() * 8,
            "bounded LowForest Elf population allocated"
        );
        Ok(())
    }

    pub(super) fn repopulate_elves(&mut self) -> Result<()> {
        for camp in 0..self.elf_camps.len() {
            let start = self.elf_camps[camp].start;
            let slots = start..start + 8;
            if self.population[start].entity.is_none() {
                if slots.clone().any(|i| self.population[i].entity.is_some()) {
                    continue;
                }
                let ready = &mut self.elf_camps[camp].ready;
                if *ready == 0 {
                    *ready = self.now + 60000;
                    tracing::debug!(
                        camp,
                        deadline = *ready,
                        "Elf camp cleared; replacement scheduled"
                    );
                }
                if self.now < *ready {
                    continue;
                }
                let positions: Vec<_> = slots.clone().map(|i| self.population[i].pos).collect();
                if !positions.iter().all(|p| {
                    self.elf_ground(*p)
                        && self
                            .players
                            .values()
                            .all(|player| player.pos.distance(*p) >= 12.)
                }) {
                    self.elf_camps[camp].ready = self.now + 1000;
                    tracing::debug!(
                        camp,
                        "Elf camp replacement refused: terrain or nearby player"
                    );
                    continue;
                }
                for (slot, pos) in slots.zip(positions) {
                    self.place_elf(camp, slot, pos)?;
                }
                self.elf_camps[camp].ready = 0;
                self.elf_camps[camp].next = ROLES.map(|(_, _, delay)| self.now + delay);
                tracing::debug!(camp, "cleared Elf camp replaced");
                continue;
            }
            let leader = self.population[start]
                .entity
                .context("live Elf camp has no leader")?;
            let center = self
                .entities
                .get(&leader)
                .context("Elf leader missing after population reconciliation")?
                .pos;
            if !self.players.values().any(|p| {
                p.character.alive
                    && !p.death_pending
                    && !p.departing
                    && !p.paused
                    && p.pos.distance(center) < 20.
            }) {
                continue;
            }
            for (role, (from, to, delay)) in ROLES.into_iter().enumerate() {
                if self.now < self.elf_camps[camp].next[role] {
                    continue;
                }
                self.elf_camps[camp].next[role] = self.now + delay;
                let Some(slot) =
                    (start + from..start + to).find(|i| self.population[*i].entity.is_none())
                else {
                    continue;
                };
                let position = (0..32)
                    .map(|i| {
                        let angle = i as f32 * std::f32::consts::TAU / 32.;
                        Pos {
                            x: center.x + 2. * angle.cos(),
                            y: center.y + 2. * angle.sin(),
                        }
                    })
                    .find(|p| {
                        self.elf_ground(*p)
                            && self
                                .players
                                .values()
                                .all(|player| player.pos.distance(*p) >= 0.75)
                            && self
                                .entities
                                .values()
                                .filter(|e| e.elf_camp == Some(camp))
                                .all(|e| e.pos.distance(*p) >= 0.75)
                    });
                let Some(position) = position else {
                    tracing::debug!(
                        camp,
                        role,
                        "Elf reinforcement refused: no clear LowForest position"
                    );
                    continue;
                };
                self.place_elf(camp, slot, position)?;
                tracing::debug!(camp, leader, role, delay, "Elf role reinforced");
            }
        }
        Ok(())
    }

    pub(super) fn elf_movement(
        &mut self,
        id: i32,
        target: Pos,
        elapsed: u64,
    ) -> Result<(Motion, f32)> {
        let e = &self.entities[&id];
        let camp = self
            .elf_camps
            .get(e.elf_camp.context("Elf movement lacks camp")?)
            .context("Elf camp missing")?;
        let leader = self.population[camp.start]
            .entity
            .and_then(|id| self.entities.get(&id));
        if e.pos.distance(e.home) > 12. {
            return Ok((
                Motion::Protect {
                    anchor: Anchor::Home,
                    distance: 1.,
                },
                6.,
            ));
        }
        if e.kind == 0x609 {
            if elapsed % 6000 < 2000 && e.pos.distance(target) >= 11. {
                self.entities.get_mut(&id).unwrap().activated = None;
                return Ok((Motion::Stationary, 0.));
            }
            return Ok(match elapsed % 6000 {
                0..2000 => (Motion::Wiggle, 8.),
                2000..4000 => (
                    Motion::Chase {
                        sight: 10.5,
                        stop: 3.,
                    },
                    6.,
                ),
                _ => (Motion::Retreat { distance: 5. }, 6.),
            });
        }
        let Some(leader) = leader else {
            return Ok((Motion::Wander, 4.));
        };
        if e.pos.distance(leader.pos) > 10. {
            return Ok((
                Motion::Protect {
                    anchor: Anchor::ElfLeader,
                    distance: 1.,
                },
                12.,
            ));
        }
        Ok(match e.kind {
            0x605 => (
                Motion::Orbit {
                    anchor: Anchor::ElfLeader,
                    radius: 5.,
                },
                6.,
            ),
            0x607 => (
                Motion::Orbit {
                    anchor: Anchor::ElfLeader,
                    radius: 6.,
                },
                5.,
            ),
            0x606 if e.pos.distance(target) < 3. => (
                Motion::Chase {
                    sight: 3.,
                    stop: 1.,
                },
                4.,
            ),
            0x606 => (
                Motion::Protect {
                    anchor: Anchor::ElfLeader,
                    distance: 6.,
                },
                6.,
            ),
            _ => bail!("unsupported Elf camp member {:#x}", e.kind),
        })
    }
}
