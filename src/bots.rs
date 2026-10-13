use crate::{
    content::Content,
    wire::{self, Packet, Reader},
    world::{
        Action, Pos, Slot,
        view::{OutputState, PlayerView},
    },
};
use anyhow::{Context, Result, ensure};
use rand::{Rng, SeedableRng};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

mod combat;
mod social;

#[derive(Clone, Copy)]
struct Profile {
    name: &'static str,
    recovery: f32,
    resources: f32,
    companions: f32,
    reaction: u64,
    leader: &'static str,
    healer: bool,
}
const PROFILES: [Profile; 6] = [
    Profile {
        name: "Bayleaf",
        recovery: 0.62,
        resources: 4.,
        companions: 3.,
        reaction: 6,
        leader: "Mossy",
        healer: true,
    },
    Profile {
        name: "Mossy",
        recovery: 0.35,
        resources: 8.,
        companions: 3.,
        reaction: 4,
        leader: "Mossy",
        healer: false,
    },
    Profile {
        name: "Trellis",
        recovery: 0.48,
        resources: 3.,
        companions: 8.,
        reaction: 5,
        leader: "Mossy",
        healer: false,
    },
    Profile {
        name: "Juniper",
        recovery: 0.55,
        resources: 3.,
        companions: 7.,
        reaction: 5,
        leader: "Reed",
        healer: true,
    },
    Profile {
        name: "Reed",
        recovery: 0.42,
        resources: 4.,
        companions: 5.,
        reaction: 6,
        leader: "Reed",
        healer: false,
    },
    Profile {
        name: "Bramble",
        recovery: 0.38,
        resources: 6.,
        companions: 6.,
        reaction: 4,
        leader: "Reed",
        healer: false,
    },
];
#[derive(Clone, Debug, PartialEq, Eq)]
enum Goal {
    Recover,
    Dodge,
    Fight(i32),
    Loot(i32),
    Follow(i32),
    Gather(i32),
    Portal(i32),
    Revisit(i16, i16),
    Explore(i16, i16),
    Wait,
}
#[derive(Debug)]
enum Journey {
    Portal(i32),
    Nexus,
    Regroup(i32),
}
struct Decision {
    goal: Goal,
    movement: Option<Pos>,
    aim: Option<f32>,
    item: Option<Action>,
    journey: Option<Journey>,
    chat: Option<&'static str>,
}
struct Brain {
    profile: Profile,
    session: (i32, i32, i32),
    last_tick: Option<u64>,
    goal: Goal,
    rng: rand::rngs::StdRng,
    heading: f32,
    previous: Pos,
    stationary: u64,
    destination: Option<Pos>,
    blocked: BTreeMap<(i16, i16), u64>,
    visits: BTreeMap<(i16, i16), u64>,
    companion: Option<i32>,
    invitation_until: u64,
    next_decision: u64,
    recover_until: u64,
    tracks: BTreeMap<i32, (Pos, u64, Pos)>,
    next_teleport: u64,
    regroup_call: Option<(Journey, u64)>,
    nexus_until: u64,
    next_call: u64,
    requested_until: u64,
    portal: Option<(i32, u64)>,
    leader_seen: Option<(i32, Pos, u64)>,
    contacts: BTreeMap<i32, (Pos, u64)>,
    hunting: Option<Pos>,
    last_encounter: u64,
}
impl Brain {
    fn new(profile: Profile, view: &PlayerView) -> Self {
        let seed = view.session.account as u64;
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let heading = rng.gen_range(0.0..std::f32::consts::TAU);
        Self {
            profile,
            session: (
                view.session.world,
                view.session.player,
                view.session.character,
            ),
            last_tick: None,
            goal: Goal::Wait,
            rng,
            heading,
            previous: view.position,
            stationary: view.tick,
            destination: None,
            blocked: BTreeMap::new(),
            visits: BTreeMap::new(),
            companion: None,
            invitation_until: 0,
            next_decision: view.tick + seed % profile.reaction,
            recover_until: 0,
            tracks: BTreeMap::new(),
            next_teleport: 0,
            regroup_call: None,
            nexus_until: if view.map.name == "Nexus" {
                view.time_ms + 10000
            } else {
                0
            },
            next_call: view.time_ms + 10000,
            requested_until: 0,
            portal: None,
            leader_seen: None,
            contacts: BTreeMap::new(),
            hunting: None,
            last_encounter: view.time_ms,
        }
    }
    fn receive(&mut self, packet: Packet, view: &PlayerView) -> Result<bool> {
        if packet.id != wire::TEXT {
            return Ok(false);
        }
        let mut r = Reader::new(&packet.body);
        let name = r.utf()?;
        let actor = r.i32()?;
        r.i32()?;
        r.u8()?;
        let recipient = r.utf()?;
        let text = r.utf()?;
        r.utf()?;
        r.done()?;
        if actor > 0 && actor != self.session.1 && recipient.is_empty() {
            let leader = self.profile.name == self.profile.leader;
            if (!leader
                && name == self.profile.leader
                && self.companion.is_none()
                && (text == "nexus"
                    || (text == "tp here"
                        && view.objects.iter().any(|o| o.id == actor)
                        && self.requested_until > view.time_ms
                        && self.leader_seen.is_none_or(|(_, _, seen)| {
                            view.time_ms.saturating_sub(seen) >= 10000
                        }))))
                || (leader
                    && text == "tp?"
                    && PROFILES
                        .iter()
                        .any(|p| p.leader == self.profile.name && p.name == name))
            {
                let journey = if text == "nexus" {
                    Journey::Nexus
                } else {
                    Journey::Regroup(actor)
                };
                self.regroup_call = Some((journey, view.time_ms + 5000));
            }
        }
        if recipient.is_empty()
            && name == self.profile.leader
            && text == "pc here"
            && self.leader_seen.is_some_and(|(id, _, seen)| {
                id == actor && view.time_ms.saturating_sub(seen) < 2000
            })
            && let Some(portal) = view.objects.iter().find(|o| o.kind == 0x717)
        {
            self.portal = Some((portal.id, view.time_ms + 30000));
        }
        if actor > 0
            && actor != self.session.1
            && recipient.is_empty()
            && view
                .objects
                .iter()
                .any(|o| o.id == actor && o.hp > 0 && !o.equipped.is_empty())
            && text.eq_ignore_ascii_case(&format!("{} follow me", self.profile.name))
        {
            self.companion = Some(actor);
            self.invitation_until = view.time_ms + 60000;
            return Ok(true);
        }
        Ok(false)
    }
    fn decide(&mut self, v: &PlayerView, content: &Content) -> Result<Decision> {
        ensure!(
            self.session == (v.session.world, v.session.player, v.session.character),
            "controller session changed"
        );
        ensure!(
            self.last_tick.is_none_or(|last| v.tick > last),
            "controller observation clock did not advance"
        );
        ensure!(
            v.lifecycle.alive
                && !v.lifecycle.death_pending
                && !v.lifecycle.departing
                && !v.lifecycle.quarantined
                && matches!(v.lifecycle.output, OutputState::Live),
            "controller is not live"
        );
        ensure!(
            v.tiles.iter().any(|t| (t.x, t.y) == tile(v.position)),
            "controller own terrain missing"
        );
        ensure!(
            v.projectiles
                .iter()
                .all(|b| b.speed > 0. && b.lifetime_ms > 0.),
            "projectile trajectory cannot be inferred from observation"
        );
        self.last_tick = Some(v.tick);
        self.next_decision = v.tick + self.profile.reaction;
        self.blocked.retain(|_, until| *until > v.tick);
        if v.time_ms >= self.invitation_until {
            self.companion = None;
        }
        if self.previous.distance(v.position) > 0.15 {
            self.stationary = v.tick;
        }
        if v.tick.saturating_sub(self.stationary) > 30
            && let Some(destination) = self.destination
        {
            self.blocked.insert(tile(destination), v.tick + 100);
            tracing::warn!(player=v.session.player,tick=v.tick,goal=?self.goal,x=destination.x,y=destination.y,"autonomous route made no progress; temporarily excluded");
            self.heading = self.rng.gen_range(0.0..std::f32::consts::TAU);
            self.stationary = v.tick;
        }
        self.previous = v.position;
        self.visits.insert(tile(v.position), v.tick);
        let mut decision = Decision {
            goal: Goal::Wait,
            movement: None,
            aim: None,
            item: None,
            journey: None,
            chat: None,
        };
        if v.lifecycle.busy {
            return Ok(decision);
        }
        let party = social::companions(v, self.profile);
        self.contacts
            .retain(|_, (_, seen)| v.time_ms.saturating_sub(*seen) < 10000);
        for peer in &party {
            self.contacts.insert(peer.id, (peer.position, v.time_ms));
            if peer.name == self.profile.leader {
                self.leader_seen = Some((peer.id, peer.position, v.time_ms));
            }
        }
        if v.map.name == "Realm"
            && self.regroup_call.as_ref().is_some_and(|(journey, until)| {
                matches!(journey, Journey::Nexus) && *until > v.time_ms
            })
        {
            self.regroup_call = None;
            decision.journey = Some(Journey::Nexus);
            return Ok(decision);
        }
        decision.item = social::healing(v, content)?;
        let enemies: Vec<_> = v
            .objects
            .iter()
            .filter(|o| content.objects[&o.kind].enemy && o.hp > 0)
            .collect();
        let nearest = enemies.iter().min_by(|a, b| {
            a.position
                .distance(v.position)
                .total_cmp(&b.position.distance(v.position))
        });
        if let Some(enemy) = nearest {
            self.hunting = Some(enemy.position);
            self.last_encounter = v.time_ms;
        }
        self.tracks
            .retain(|id, _| enemies.iter().any(|e| e.id == *id));
        for enemy in &enemies {
            let mut velocity = Pos { x: 0., y: 0. };
            if let Some((previous, time, _)) = self.tracks.get(&enemy.id) {
                let elapsed = v.time_ms.saturating_sub(*time) as f32 / 1000.;
                if elapsed > 0. && elapsed < 1. && previous.distance(enemy.position) < elapsed * 8.
                {
                    velocity = Pos {
                        x: (enemy.position.x - previous.x) / elapsed,
                        y: (enemy.position.y - previous.y) / elapsed,
                    };
                }
            }
            self.tracks
                .insert(enemy.id, (enemy.position, v.time_ms, velocity));
        }
        let weapon = content.get(v.character.items[0] as u16)?;
        let projectile = weapon
            .projectiles
            .get(&0)
            .context("weapon projectile missing")?;
        let range = projectile.speed * projectile.lifetime / 10000.;
        let health = v.character.hp as f32 / v.character.stats[0] as f32;
        let threatened = combat::clearance(v, v.position) < 1.05;
        if health < self.profile.recovery {
            self.recover_until = v.tick + 60;
        }
        let retreat = v.tick < self.recover_until
            || (matches!(self.goal, Goal::Recover)
                && health < (self.profile.recovery + 0.15).min(0.9));
        let mut grid = Grid::new(v);
        for point in self
            .blocked
            .keys()
            .filter(|point| **point != tile(v.position))
        {
            grid.walkable.remove(point);
        }
        if retreat || threatened {
            decision.goal = if retreat { Goal::Recover } else { Goal::Dodge };
            let held_heal = v
                .character
                .items
                .iter()
                .filter(|kind| **kind >= 0)
                .any(|kind| content.objects[&(*kind as u16)].activate == "Heal");
            if health < (self.profile.recovery * 0.6).max(0.25)
                && !held_heal
                && decision.item.is_none()
                && v.map.name != "Nexus"
            {
                decision.journey = Some(Journey::Nexus);
                return Ok(decision);
            }
            if threatened || nearest.is_some_and(|e| e.position.distance(v.position) < range * 1.3)
            {
                decision.movement =
                    combat::movement(v, &grid, &enemies, nearest.map(|e| e.position), range, true);
            }
            if retreat {
                for (slot, kind) in v
                    .character
                    .items
                    .iter()
                    .enumerate()
                    .skip(4)
                    .filter(|(_, k)| **k >= 0)
                {
                    let item = content.get(*kind as u16)?;
                    if decision.item.is_none()
                        && item.activate == "Heal"
                        && v.character.stats[0] - v.character.hp >= (item.heal as f32 * 0.35) as i32
                    {
                        decision.item = Some(Action::Use {
                            slot,
                            x: v.position.x,
                            y: v.position.y,
                        });
                        break;
                    }
                }
            }
            if !retreat && let Some(enemy) = nearest {
                let target = combat::lead(v, enemy, self.tracks[&enemy.id].2, projectile);
                if v.position.distance(target) < range
                    && shot_clear(v, &grid, target, projectile.passes_cover)
                {
                    decision.aim = Some(angle(v.position, target));
                }
            }
            return Ok(decision);
        }
        let class = content.get(v.character.class)?;
        for (from, kind) in v
            .character
            .items
            .iter()
            .enumerate()
            .skip(4)
            .filter(|(_, k)| **k >= 0)
        {
            let item = content.get(*kind as u16)?;
            if decision.item.is_none()
                && let Some(to) = class
                    .slots
                    .iter()
                    .take(4)
                    .position(|slot| *slot == item.slot_type)
                && upgrade(content, *kind, v.character.items[to])?
            {
                decision.item = Some(Action::Swap {
                    from: own(v, from),
                    to: own(v, to),
                });
                return Ok(decision);
            }
        }
        if v.map.name == "Nexus" && v.time_ms < self.nexus_until {
            return Ok(decision);
        }
        if v.map.name == "Realm"
            && health >= 0.75
            && decision.item.is_none()
            && nearest.is_none_or(|e| e.position.distance(v.position) > 5.)
        {
            if let Some((journey, _)) = self.regroup_call.take().filter(|(_, t)| *t > v.time_ms) {
                if self.profile.name == self.profile.leader {
                    if v.time_ms >= self.next_call {
                        let Journey::Regroup(actor) = journey else {
                            anyhow::bail!("leader received an invalid reunion request");
                        };
                        if v.objects.iter().any(|o| o.id == actor) {
                            decision.chat = Some("tp here");
                        } else {
                            decision.chat = Some("nexus");
                            decision.journey = Some(Journey::Nexus);
                        }
                        self.next_call = v.time_ms + 10000;
                    }
                } else if v.time_ms >= self.next_teleport {
                    decision.journey = Some(journey);
                    self.next_teleport = v.time_ms + 10000;
                }
                if decision.journey.is_some() {
                    return Ok(decision);
                }
            } else if self.profile.name != self.profile.leader
                && self
                    .leader_seen
                    .is_none_or(|(_, _, seen)| v.time_ms.saturating_sub(seen) >= 10000)
                && v.time_ms >= self.next_call
            {
                decision.chat = Some("tp?");
                self.requested_until = v.time_ms + 5000;
                self.next_call = v.time_ms + 30000;
            }
        }
        let mut choices = Vec::new();
        for enemy in &enemies {
            let d = enemy.position.distance(v.position);
            let score = 45. - d * 2.;
            choices.push((score, Goal::Fight(enemy.id), enemy.position));
        }
        if v.character.items.iter().skip(4).any(|k| *k == -1) {
            for bag in v
                .objects
                .iter()
                .filter(|o| content.objects[&o.kind].class == "Container")
            {
                let relevant = bag
                    .contents
                    .iter()
                    .filter(|k| **k >= 0)
                    .map(|k| useful(content, v, *k))
                    .collect::<Result<Vec<_>>>()?;
                if relevant.contains(&true) {
                    let score =
                        16. * self.profile.resources / (1. + v.position.distance(bag.position));
                    choices.push((score, Goal::Loot(bag.id), bag.position));
                }
            }
        }
        let peaceful = nearest.is_none_or(|e| e.position.distance(v.position) > 5.);
        for portal in v.objects.iter().filter(|p| match v.map.name.as_str() {
            "Nexus" => matches!(p.kind, 0x704 | 0x70e | 0x71c),
            // The committed exit is public even when a straggler missed personal clear credit.
            "Pirate Cave" => p.kind == 0x71d,
            "Realm" => {
                p.kind == 0x717
                    && v.character.level >= 2
                    && (self
                        .portal
                        .is_some_and(|(id, until)| id == p.id && until > v.time_ms)
                        || (self.profile.name == self.profile.leader
                            && party.len() >= 2
                            && party.iter().all(|q| q.position.distance(p.position) < 8.)))
            }
            _ => false,
        }) {
            if peaceful && health >= 0.75 {
                choices.push((80., Goal::Portal(portal.id), portal.position));
            }
        }
        if self.profile.name == self.profile.leader
            && let Some(straggler) = party
                .iter()
                .filter(|p| p.position.distance(v.position) > 10. || p.hp * 10 < p.max_hp * 7)
                .max_by(|a, b| {
                    a.position
                        .distance(v.position)
                        .total_cmp(&b.position.distance(v.position))
                })
        {
            choices.push((35., Goal::Gather(straggler.id), v.position));
        }
        for player in v
            .objects
            .iter()
            .filter(|o| content.objects[&o.kind].class == "Player")
        {
            let d = player.position.distance(v.position);
            if d > 4.
                && (self.companion == Some(player.id)
                    || (self.profile.name != self.profile.leader
                        && player.name == self.profile.leader))
            {
                let score = if self.companion == Some(player.id) {
                    70.
                } else {
                    45. + self.profile.companions * (d - 4.).min(4.)
                };
                choices.push((score, Goal::Follow(player.id), player.position));
            }
        }
        if self.profile.name == self.profile.leader {
            for (id, (position, _)) in &self.contacts {
                if !party.iter().any(|p| p.id == *id) && v.position.distance(*position) > 3. {
                    choices.push((35., Goal::Gather(*id), *position));
                }
            }
        } else if let Some((id, position, seen)) = self.leader_seen
            && v.time_ms.saturating_sub(seen) < 10000
            && !party.iter().any(|p| p.id == id)
            && v.position.distance(position) > 1.
        {
            choices.push((70., Goal::Follow(id), position));
        }
        if let Some(place) = self.hunting
            && v.time_ms.saturating_sub(self.last_encounter)
                > (240000. / self.profile.resources) as u64
        {
            if v.position.distance(place) > 3. {
                let (x, y) = tile(place);
                choices.push((30., Goal::Revisit(x, y), center((x, y))));
            } else {
                self.last_encounter = v.time_ms;
            }
        }
        let reachable = grid.reachable(v.position);
        if let Goal::Explore(x, y) = self.goal {
            let pos = center((x, y));
            if pos.distance(v.position) > 0.7 && reachable.contains(&(x, y)) {
                choices.push((12., self.goal.clone(), pos));
            }
        }
        if !choices
            .iter()
            .any(|(_, g, _)| matches!(g, Goal::Explore(..)))
        {
            let next = reachable
                .iter()
                .filter(|t| !self.blocked.contains_key(t))
                .filter(|t| {
                    center(**t).distance(v.position) > 3. && center(**t).distance(v.position) < 24.
                })
                .max_by(|a, b| {
                    let frontier = |t| grid.frontier(t, v.map.width, v.map.height);
                    let priority = frontier(**a)
                        .cmp(&frontier(**b))
                        .then_with(|| self.visits.get(*b).cmp(&self.visits.get(*a)));
                    if !priority.is_eq() {
                        return priority;
                    }
                    let value = |t| {
                        let p = center(t);
                        nearest.map_or_else(
                            || {
                                (p.x - v.position.x) * self.heading.cos()
                                    + (p.y - v.position.y) * self.heading.sin()
                            },
                            |enemy| -p.distance(enemy.position),
                        )
                    };
                    value(**a).total_cmp(&value(**b))
                });
            if let Some((x, y)) = next {
                choices.push((12., Goal::Explore(*x, *y), center((*x, *y))));
            }
        }
        for (score, goal, _) in &mut choices {
            if *goal == self.goal {
                *score += 8.;
            }
        }
        choices.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (_, goal, destination) in choices {
            if self.blocked.contains_key(&tile(destination)) {
                continue;
            }
            let movement = match goal {
                Goal::Gather(_) if v.position.distance(destination) < 0.7 => None,
                Goal::Portal(id) if v.position.distance(destination) <= 2. => {
                    decision.journey = Some(Journey::Portal(id));
                    None
                }
                Goal::Fight(_)
                    if v.position.distance(destination) < range
                        && shot_clear(v, &grid, destination, projectile.passes_cover) =>
                {
                    combat::movement(v, &grid, &enemies, Some(destination), range, false)
                }
                Goal::Follow(_) if v.position.distance(destination) < 4.5 => None,
                Goal::Loot(id) if v.position.distance(destination) < 1.4 => {
                    let bag = v
                        .objects
                        .iter()
                        .find(|o| o.id == id)
                        .context("selected bag missing")?;
                    let mut selected = None;
                    for (slot, kind) in bag.contents.iter().enumerate().filter(|(_, k)| **k >= 0) {
                        if useful(content, v, *kind)? {
                            selected = Some((slot, *kind));
                            break;
                        }
                    }
                    let (slot, kind) = selected.context("useful bag item missing")?;
                    let to = v
                        .character
                        .items
                        .iter()
                        .enumerate()
                        .skip(4)
                        .find(|(_, k)| **k == -1)
                        .context("inventory full")?
                        .0;
                    decision.item = Some(Action::Swap {
                        from: Slot {
                            object: id,
                            slot,
                            kind,
                        },
                        to: own(v, to),
                    });
                    None
                }
                _ => match grid.next(v.position, destination) {
                    Some(pos) => Some(pos),
                    None => {
                        self.blocked.insert(tile(destination), v.tick + 100);
                        continue;
                    }
                },
            };
            if matches!(goal, Goal::Portal(_))
                && self.goal != goal
                && v.map.name == "Realm"
                && self.profile.name == self.profile.leader
            {
                decision.chat = Some("pc here");
            }
            if let Goal::Follow(id) = goal
                && v.position.distance(destination) > 8.
                && v.time_ms >= self.next_teleport
                && decision.item.is_none()
                && v.objects
                    .iter()
                    .any(|p| p.id == id && p.hp * 4 >= p.max_hp * 3)
                && enemies
                    .iter()
                    .all(|e| e.position.distance(destination) > 3.)
            {
                decision.journey = Some(Journey::Regroup(id));
                self.next_teleport = v.time_ms + 10000;
            }
            decision.goal = goal;
            decision.movement = movement;
            if let Goal::Fight(id) = decision.goal {
                let enemy = enemies
                    .iter()
                    .find(|e| e.id == id)
                    .context("selected enemy missing")?;
                let target = combat::lead(v, enemy, self.tracks[&id].2, projectile);
                if v.position.distance(target) < range
                    && shot_clear(v, &grid, target, projectile.passes_cover)
                {
                    decision.aim = Some(angle(v.position, target));
                }
                if v.character.items[1] >= 0 && v.time_ms >= v.character.ability_ready_ms {
                    let ability = content.get(v.character.items[1] as u16)?;
                    if ability.activate == "BulletNova"
                        && v.character.mp >= ability.mana_cost
                        && enemy.hp > projectile.max * 2
                        && v.position.distance(target) <= 15.
                        && shot_clear(v, &grid, target, false)
                    {
                        decision.item = Some(Action::Use {
                            slot: 1,
                            x: target.x,
                            y: target.y,
                        });
                    }
                }
            }
            return Ok(decision);
        }
        self.heading += 0.7;
        Ok(decision)
    }
}
fn angle(a: Pos, b: Pos) -> f32 {
    (b.y - a.y).atan2(b.x - a.x)
}
fn tile(p: Pos) -> (i16, i16) {
    (p.x.floor() as i16, p.y.floor() as i16)
}
fn center(t: (i16, i16)) -> Pos {
    Pos {
        x: f32::from(t.0) + 0.5,
        y: f32::from(t.1) + 0.5,
    }
}
fn hostile(v: &PlayerView, owner: i32) -> bool {
    owner != v.session.player
        && !v
            .objects
            .iter()
            .any(|o| o.id == owner && !o.equipped.is_empty())
}
fn shot_clear(v: &PlayerView, grid: &Grid, target: Pos, passes_cover: bool) -> bool {
    let steps = (v.position.distance(target) * 8.).ceil().max(1.) as u32;
    (0..=steps).all(|step| {
        let f = step as f32 / steps as f32;
        let point = tile(Pos {
            x: v.position.x + (target.x - v.position.x) * f,
            y: v.position.y + (target.y - v.position.y) * f,
        });
        grid.known.contains(&point)
            && (passes_cover
                || !v
                    .objects
                    .iter()
                    .any(|o| o.blocks && tile(o.position) == point))
    })
}
fn own(v: &PlayerView, slot: usize) -> Slot {
    Slot {
        object: v.session.player,
        slot,
        kind: v.character.items[slot],
    }
}
fn upgrade(content: &Content, new: i32, old: i32) -> Result<bool> {
    let item = content.get(new as u16)?;
    if item.unsupported_equipment {
        return Ok(false);
    }
    if old < 0 {
        return Ok(true);
    }
    Ok(matches!((item.tier,content.get(old as u16)?.tier),(Some(a),Some(b)) if a>b))
}
fn useful(content: &Content, v: &PlayerView, kind: i32) -> Result<bool> {
    let item = content.get(kind as u16)?;
    if item.activate == "Heal" {
        return Ok(v.character.items.iter().filter(|k| **k == kind).count() < 2);
    }
    let class = content.get(v.character.class)?;
    match class
        .slots
        .iter()
        .take(4)
        .position(|s| *s == item.slot_type)
    {
        Some(slot) => upgrade(content, kind, v.character.items[slot]),
        None => Ok(false),
    }
}
struct Grid {
    walkable: BTreeSet<(i16, i16)>,
    known: BTreeSet<(i16, i16)>,
}
impl Grid {
    fn new(v: &PlayerView) -> Self {
        let mut walkable: BTreeSet<_> = v
            .tiles
            .iter()
            .filter(|t| t.ground_speed > 0.)
            .map(|t| (t.x, t.y))
            .collect();
        for object in v.objects.iter().filter(|o| o.blocks) {
            walkable.remove(&tile(object.position));
        }
        Self {
            walkable,
            known: v.tiles.iter().map(|t| (t.x, t.y)).collect(),
        }
    }
    fn reachable(&self, from: Pos) -> BTreeSet<(i16, i16)> {
        let start = tile(from);
        let mut queue = VecDeque::from([start]);
        let mut reached = BTreeSet::from([start]);
        while let Some((x, y)) = queue.pop_front() {
            for next in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                if self.walkable.contains(&next) && reached.insert(next) {
                    queue.push_back(next);
                }
            }
        }
        reached
    }
    fn frontier(&self, (x, y): (i16, i16), width: i32, height: i32) -> bool {
        [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]
            .iter()
            .any(|n| {
                n.0 >= 0
                    && n.1 >= 0
                    && i32::from(n.0) < width
                    && i32::from(n.1) < height
                    && !self.known.contains(n)
            })
    }
    fn segment(&self, from: Pos, to: Pos) -> bool {
        let steps = (from.distance(to) * 4.).ceil().max(1.) as i32;
        (0..=steps).all(|i| {
            let f = i as f32 / steps as f32;
            self.walkable.contains(&tile(Pos {
                x: from.x + (to.x - from.x) * f,
                y: from.y + (to.y - from.y) * f,
            }))
        })
    }
    fn next(&self, from: Pos, to: Pos) -> Option<Pos> {
        let start = tile(from);
        let finish = tile(to);
        if !self.walkable.contains(&finish) {
            return None;
        }
        if self.segment(from, to) {
            return Some(to);
        }
        if start == finish {
            return None;
        }
        let mut queue = VecDeque::from([start]);
        let mut parent = HashMap::from([(start, start)]);
        while let Some((x, y)) = queue.pop_front() {
            if (x, y) == finish {
                break;
            }
            for next in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                if self.walkable.contains(&next) && !parent.contains_key(&next) {
                    parent.insert(next, (x, y));
                    queue.push_back(next);
                }
            }
        }
        let mut next = finish;
        while *parent.get(&next)? != start {
            next = *parent.get(&next)?;
        }
        let target = center(next);
        if self.segment(from, target) {
            Some(target)
        } else {
            self.segment(from, center(start)).then_some(center(start))
        }
    }
}

#[cfg(test)]
mod tests;

mod runtime;
pub(crate) const OUTPUT_CAPACITY: usize = 512;
pub use runtime::start;
