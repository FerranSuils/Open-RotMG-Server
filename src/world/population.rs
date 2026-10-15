use super::*;

pub(super) struct Role {
    pub kind: u16,
    pub maximum: usize,
    pub initial: usize,
    pub interval: Option<u64>,
}
pub(super) struct Family {
    pub leader: u16,
    pub terrain: u8,
    pub roles: &'static [Role],
}
impl Family {
    fn size(&self) -> usize {
        1 + self.roles.iter().map(|r| r.maximum).sum::<usize>()
    }
    fn initial(&self) -> usize {
        1 + self.roles.iter().map(|r| r.initial).sum::<usize>()
    }
}
macro_rules! family {
    ($leader:expr,$terrain:expr; $( $kind:expr,$maximum:expr,$initial:expr,$interval:expr );+ $(;)?) => {
        Family { leader:$leader, terrain:$terrain, roles:&[$(Role {kind:$kind,maximum:$maximum,initial:$initial,interval:Some($interval)}),+] }
    };
}
pub(super) const FAMILIES: &[Family] = &[
    family!(0x601,11; 0x600,5,2,10000; 0x601,4,0,10000),
    family!(0x604,11; 0x603,10,5,10000),
    family!(0x953,11; 0x954,5,3,8000),
    family!(0x60d,8; 0x60a,2,2,10000; 0x60c,3,3,8000),
    family!(0x617,9; 0x616,4,2,12000; 0x615,3,2,6000),
    family!(0x61a,9; 0x619,4,2,12000; 0x618,3,2,6000),
    family!(0x609,10; 0x605,2,2,15000; 0x606,4,4,7000; 0x607,1,1,8000),
    family!(0x614,10; 0x612,7,3,12000; 0x613,7,3,12000),
    family!(0x652,1; 0x653,5,3,10000),
    Family {
        leader: 0x905,
        terrain: 1,
        roles: &[
            Role {
                kind: 0x906,
                maximum: 1,
                initial: 1,
                interval: None,
            },
            Role {
                kind: 0x907,
                maximum: 1,
                initial: 1,
                interval: None,
            },
        ],
    },
];
pub(super) struct Camp {
    pub start: usize,
    pub family: &'static Family,
    pub(super) ready: u64,
    next: Vec<Option<u64>>,
}

fn region(terrain: u8) -> u8 {
    if matches!(terrain, 11 | 12) {
        11
    } else {
        terrain
    }
}
fn formation(center: Pos, size: usize) -> Vec<Pos> {
    (0..size)
        .map(|i| {
            if i == 0 {
                center
            } else {
                let angle = (i - 1) as f32 * std::f32::consts::TAU / (size - 1) as f32;
                Pos {
                    x: center.x + 2. * angle.cos(),
                    y: center.y + 2. * angle.sin(),
                }
            }
        })
        .collect()
}

impl World {
    pub(super) fn population_count(&self, terrain: u8) -> usize {
        self.population
            .iter()
            .filter(|s| {
                region(self.content.objects[&s.kind].terrain) == region(terrain)
                    && s.entity.is_some_and(|id| self.entities.contains_key(&id))
            })
            .count()
    }
    pub(super) fn population_space(&self, terrain: u8, count: usize) -> Result<bool> {
        ensure!(
            (1..=12).contains(&terrain),
            "population has unsupported terrain {terrain}"
        );
        Ok(self.population_count(terrain) + count <= if terrain >= 11 { 24 } else { 32 })
    }
    fn family_space(&self, terrain: u8, count: usize) -> Result<bool> {
        // Reinforcements cannot consume a solo encounter's reserved respawn capacity.
        let reserved = self
            .population
            .iter()
            .filter(|s| {
                s.camp.is_none()
                    && region(self.content.objects[&s.kind].terrain) == region(terrain)
                    && s.entity.is_none_or(|id| !self.entities.contains_key(&id))
            })
            .count();
        self.population_space(terrain, count + reserved)
    }
    fn family_ground(&self, pos: Pos, terrain: u8) -> bool {
        self.walkable(pos)
            && self
                .map
                .cell(pos.x, pos.y)
                .is_some_and(|c| c.terrain == terrain && c.object.is_none())
    }
    fn place_member(&mut self, camp: usize, slot: usize, pos: Pos) -> Result<()> {
        let kind = self.population[slot].kind;
        ensure!(
            self.family_space(self.content.objects[&kind].terrain, 1)?,
            "family birth exceeds regional budget"
        );
        let id = self.spawn_entity(kind, pos, vec![])?;
        let home = self.population[self.camps[camp].start].pos;
        let e = self.entities.get_mut(&id).unwrap();
        // Group affiliation cannot use summon parenthood, which deletes surviving ordinary followers.
        e.camp = Some(camp);
        e.home = home;
        self.population[slot].entity = Some(id);
        tracing::debug!(camp, entity = id, kind, "family member created");
        Ok(())
    }
    fn place_camp(&mut self, family: &'static Family, center: Pos) -> Result<()> {
        for kind in std::iter::once(family.leader).chain(family.roles.iter().map(|r| r.kind)) {
            ensure!(
                self.content.get(kind)?.terrain == family.terrain,
                "family contradicts XML habitat"
            );
            crate::encounter::validate(&self.content, kind)?;
            crate::loot::validate(&self.content, kind)?;
        }
        ensure!(
            family
                .roles
                .iter()
                .all(|r| r.initial <= r.maximum && r.interval.is_none_or(|interval| interval > 0)),
            "invalid family role bounds"
        );
        ensure!(
            self.family_space(family.terrain, family.initial())?,
            "initial family exceeds regional budget"
        );
        let camp = self.camps.len();
        let start = self.population.len();
        self.camps.push(Camp {
            start,
            family,
            ready: 0,
            next: family
                .roles
                .iter()
                .map(|r| r.interval.map(|interval| self.now + interval))
                .collect(),
        });
        let members: Vec<_> = std::iter::once((family.leader, true))
            .chain(
                family
                    .roles
                    .iter()
                    .flat_map(|r| (0..r.maximum).map(move |i| (r.kind, i < r.initial))),
            )
            .collect();
        for ((kind, initial), pos) in members.into_iter().zip(formation(center, family.size())) {
            let slot = self.population.len();
            self.population.push(Spawn {
                kind,
                pos,
                entity: None,
                ready: 0,
                camp: Some(camp),
            });
            if initial {
                self.place_member(camp, slot, pos)?;
            }
        }
        tracing::info!(
            camp,
            leader = family.leader,
            terrain = family.terrain,
            initial = family.initial(),
            capacity = family.size(),
            "bounded family allocated"
        );
        Ok(())
    }
    pub(super) fn populate_families(
        &mut self,
        terrain: u8,
        candidates: &[Pos],
        leaders: &[u16],
    ) -> Result<()> {
        for leader in leaders {
            let family = FAMILIES
                .iter()
                .find(|f| f.leader == *leader)
                .context("population family is undefined")?;
            ensure!(family.terrain == terrain, "family placed in wrong terrain");
            let minimum = if matches!(leader, 0x604 | 0x953) {
                64.
            } else {
                12.
            };
            let center = candidates.iter().copied().find(|center| {
                self.map
                    .spawns
                    .iter()
                    .all(|(x, y)| center.distance(Pos { x: *x, y: *y }) >= minimum)
                    && formation(*center, family.size())
                        .iter()
                        .all(|p| self.family_ground(*p, terrain))
                    && self
                        .camps
                        .iter()
                        .filter(|c| c.family.terrain == terrain)
                        .all(|c| self.population[c.start].pos.distance(*center) >= 8.)
            });
            let center = center.with_context(|| {
                format!("terrain {terrain} cannot place full family {leader:#x}")
            })?;
            self.place_camp(family, center)?;
        }
        Ok(())
    }
    pub(super) fn populate_shore_population(&mut self) -> Result<()> {
        let &(x, y) = self.map.spawns.first().context("shore has no arrival")?;
        let arrival = Pos { x, y };
        let mut candidates: Vec<_> = self
            .map
            .cells
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                let pos = Pos {
                    x: (i % self.map.width as usize) as f32 + 0.5,
                    y: (i / self.map.width as usize) as f32 + 0.5,
                };
                (matches!(c.terrain, 11 | 12)
                    && self.family_ground(pos, c.terrain)
                    && pos.distance(arrival) >= 12.)
                    .then_some(pos)
            })
            .collect();
        candidates.shuffle(&mut self.rng);
        let mut arrival_sites = candidates.clone();
        arrival_sites.sort_by(|a, b| a.distance(arrival).total_cmp(&b.distance(arrival)));
        self.populate_families(11, &arrival_sites, &[0x601])?;
        self.populate_families(11, &candidates, &[0x604, 0x953])?;
        for (i, kind) in [
            0x602, 0x602, 0x60e, 0x60f, 0x610, 0x60e, 0x60f, 0x610, 0x60e, 0x60f, 0x610,
        ]
        .into_iter()
        .enumerate()
        {
            let terrain = self.content.get(kind)?.terrain;
            let sites = if i == 0 || (2..5).contains(&i) {
                &arrival_sites
            } else {
                &candidates
            };
            let pos = sites
                .iter()
                .copied()
                .find(|p| {
                    self.family_ground(*p, terrain)
                        && self.population.iter().all(|s| s.pos.distance(*p) >= 6.)
                })
                .context("shore cannot place its bounded solo roster")?;
            let entity = self.spawn_entity(kind, pos, vec![])?;
            self.population.push(Spawn {
                kind,
                pos,
                entity: Some(entity),
                ready: 0,
                camp: None,
            });
        }
        ensure!(
            self.population_count(11) == 24,
            "shore live population must start at24"
        );
        Ok(())
    }
    pub(super) fn repopulate_families(&mut self) -> Result<()> {
        for camp in 0..self.camps.len() {
            let start = self.camps[camp].start;
            let family = self.camps[camp].family;
            let slots = start..start + family.size();
            if self.population[start].entity.is_none() {
                if slots.clone().any(|i| self.population[i].entity.is_some()) {
                    continue;
                }
                if self.camps[camp].ready == 0 {
                    self.camps[camp].ready = self.now + 60000;
                    tracing::debug!(
                        camp,
                        deadline = self.camps[camp].ready,
                        "family cleared; replacement scheduled"
                    );
                }
                if self.now < self.camps[camp].ready {
                    continue;
                }
                if !self.family_space(family.terrain, family.initial())?
                    || !slots.clone().all(|i| {
                        self.family_ground(self.population[i].pos, family.terrain)
                            && self
                                .players
                                .values()
                                .all(|p| p.pos.distance(self.population[i].pos) >= 12.)
                    })
                {
                    self.camps[camp].ready = self.now + 1000;
                    tracing::debug!(
                        camp,
                        "family replacement refused: budget, terrain or nearby player"
                    );
                    continue;
                }
                for slot in slots {
                    let offset = slot - start;
                    let mut end = 1;
                    let mut initial = offset == 0;
                    for role in family.roles {
                        if (end..end + role.initial).contains(&offset) {
                            initial = true;
                        }
                        end += role.maximum;
                    }
                    if initial {
                        self.place_member(camp, slot, self.population[slot].pos)?;
                    }
                }
                self.camps[camp].ready = 0;
                self.camps[camp].next = family
                    .roles
                    .iter()
                    .map(|r| r.interval.map(|interval| self.now + interval))
                    .collect();
                continue;
            }
            let leader = self.population[start]
                .entity
                .context("family leader slot missing")?;
            let center = self
                .entities
                .get(&leader)
                .context("leader missing after reconciliation")?
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
            let mut from = start + 1;
            for (role, r) in family.roles.iter().enumerate() {
                let to = from + r.maximum;
                let Some(interval) = r.interval else {
                    from = to;
                    continue;
                };
                let deadline =
                    self.camps[camp].next[role].context("reinforcing role lacks its clock")?;
                if self.now >= deadline {
                    self.camps[camp].next[role] = Some(self.now + interval);
                    if let Some(slot) = (from..to).find(|i| self.population[*i].entity.is_none()) {
                        if !self.family_space(family.terrain, 1)? {
                            tracing::debug!(camp, role, "reinforcement refused: regional budget");
                            from = to;
                            continue;
                        }
                        let pos = (0..32)
                            .map(|i| {
                                let angle = i as f32 * std::f32::consts::TAU / 32.;
                                Pos {
                                    x: center.x + 2. * angle.cos(),
                                    y: center.y + 2. * angle.sin(),
                                }
                            })
                            .find(|p| {
                                self.family_ground(*p, family.terrain)
                                    && self.players.values().all(|q| q.pos.distance(*p) >= 0.75)
                                    && self
                                        .entities
                                        .values()
                                        .filter(|e| e.camp == Some(camp))
                                        .all(|e| e.pos.distance(*p) >= 0.75)
                            });
                        if let Some(pos) = pos {
                            self.place_member(camp, slot, pos)?;
                            tracing::debug!(
                                camp,
                                leader,
                                role,
                                kind = r.kind,
                                interval,
                                "family role reinforced"
                            );
                        } else {
                            tracing::debug!(
                                camp,
                                role,
                                "reinforcement refused: no clear habitat position"
                            );
                        }
                    }
                }
                from = to;
            }
        }
        Ok(())
    }
}
