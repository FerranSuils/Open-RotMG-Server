use crate::{
    content::{Content, WIZARD},
    server::{self, App},
    wire::{self, Packet, Reader},
    world::{
        Action, Output, Pos, Slot,
        view::{OutputState, PlayerView},
    },
};
use anyhow::{Context, Result, ensure};
use rand::{Rng, SeedableRng};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    time::Duration,
};
use tokio::sync::{mpsc, watch};

#[derive(Clone, Copy)]
struct Profile {
    name: &'static str,
    recovery: f32,
    resources: f32,
    companions: f32,
    reaction: u64,
}
const PROFILES: [Profile; 3] = [
    Profile {
        name: "Bayleaf",
        recovery: 0.62,
        resources: 4.,
        companions: 3.,
        reaction: 6,
    },
    Profile {
        name: "Mossy",
        recovery: 0.35,
        resources: 8.,
        companions: 3.,
        reaction: 4,
    },
    Profile {
        name: "Trellis",
        recovery: 0.48,
        resources: 3.,
        companions: 8.,
        reaction: 5,
    },
];
#[derive(Clone, Debug, PartialEq, Eq)]
enum Goal {
    Recover,
    Dodge,
    Fight(i32),
    Loot(i32),
    Follow(i32),
    Explore(i16, i16),
    Wait,
}
struct Decision {
    goal: Goal,
    movement: Option<Pos>,
    aim: Option<f32>,
    item: Option<Action>,
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
    next_decision: u64,
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
            next_decision: view.tick + seed % profile.reaction,
        }
    }
    fn receive(&mut self, packet: Packet, view: &PlayerView) -> Result<bool> {
        if packet.id != wire::TEXT {
            return Ok(false);
        }
        let mut r = Reader::new(&packet.body);
        let _name = r.utf()?;
        let actor = r.i32()?;
        r.i32()?;
        r.u8()?;
        let recipient = r.utf()?;
        let text = r.utf()?;
        r.utf()?;
        r.done()?;
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
        self.last_tick = Some(v.tick);
        self.next_decision = v.tick + self.profile.reaction;
        self.blocked.retain(|_, until| *until > v.tick);
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
        };
        if v.lifecycle.busy {
            return Ok(decision);
        }
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
        let health = v.character.hp as f32 / v.character.stats[0] as f32;
        let threatened = v.projectiles.iter().any(|b| {
            hostile(v, b.owner)
                && (0..=4).any(|i| predicted(b, i as f32 * 0.1).distance(v.position) < 1.05)
        });
        let retreat = health < self.profile.recovery;
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
            let mut safest = None;
            for i in 0..16 {
                let angle = i as f32 * std::f32::consts::TAU / 16.;
                let pos = Pos {
                    x: v.position.x + angle.cos() * 1.25,
                    y: v.position.y + angle.sin() * 1.25,
                };
                if !grid.segment(v.position, pos) {
                    continue;
                }
                let clearance = v
                    .projectiles
                    .iter()
                    .filter(|b| hostile(v, b.owner))
                    .map(|b| {
                        (0..=4)
                            .map(|i| predicted(b, i as f32 * 0.1).distance(pos))
                            .fold(20., f32::min)
                    })
                    .fold(20., f32::min);
                let distance = nearest.map_or(20., |e| e.position.distance(pos));
                let score = clearance.min(4.) * 4. + distance.min(12.);
                if safest.as_ref().is_none_or(|(old, _)| score > *old) {
                    safest = Some((score, pos));
                }
            }
            decision.movement = safest.map(|(_, pos)| pos);
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
                    if item.activate == "Heal"
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
            if let Some(enemy) = nearest {
                decision.aim = Some(angle(v.position, enemy.position));
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
            if let Some(to) = class
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
        let mut choices = Vec::new();
        let weapon = content.get(v.character.items[0] as u16)?;
        let projectile = weapon
            .projectiles
            .get(&0)
            .context("weapon projectile missing")?;
        let range = projectile.speed * projectile.lifetime / 10000.;
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
        for player in v
            .objects
            .iter()
            .filter(|o| content.objects[&o.kind].class == "Player")
        {
            let d = player.position.distance(v.position);
            if d > 4.
                && (self.companion == Some(player.id)
                    || PROFILES.iter().any(|p| p.name == player.name))
            {
                let score = if self.companion == Some(player.id) {
                    70.
                } else {
                    self.profile.companions * (d - 4.).min(4.)
                };
                choices.push((score, Goal::Follow(player.id), player.position));
            }
        }
        if let Goal::Explore(x, y) = self.goal {
            let pos = center((x, y));
            if pos.distance(v.position) > 0.7 {
                choices.push((12., self.goal.clone(), pos));
            }
        }
        if !choices
            .iter()
            .any(|(_, g, _)| matches!(g, Goal::Explore(..)))
        {
            let next = grid
                .walkable
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
                        (p.x - v.position.x) * self.heading.cos()
                            + (p.y - v.position.y) * self.heading.sin()
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
                Goal::Fight(_) if v.position.distance(destination) < range * 0.7 => None,
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
            decision.goal = goal;
            decision.movement = movement;
            if let Some(enemy) = nearest
                && v.position.distance(enemy.position) < range
            {
                decision.aim = Some(angle(v.position, enemy.position));
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
fn predicted(b: &crate::world::view::VisibleProjectile, t: f32) -> Pos {
    Pos {
        x: b.position.x + b.angle.cos() * b.speed / 10. * t,
        y: b.position.y + b.angle.sin() * b.speed / 10. * t,
    }
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

pub async fn start(app: &App) -> Result<()> {
    for profile in PROFILES {
        let Some(ch) = app
            .store
            .shore_character(profile.name.into(), app.content.get(WIZARD)?)
            .await?
        else {
            tracing::info!(
                name = profile.name,
                "permanently dead shore bot remains retired"
            );
            continue;
        };
        launch(app, ch, -3, None, profile).await?;
    }
    Ok(())
}
pub fn validate_test_class(class: u16) -> Result<()> {
    ensure!(
        class == WIZARD,
        "this source revision only implements the Wizard gameplay controller"
    );
    Ok(())
}
pub async fn start_test(
    app: &App,
    ch: crate::store::Character,
    world: i32,
    pos: Pos,
) -> Result<()> {
    validate_test_class(ch.class)?;
    launch(app, ch, world, Some(pos), PROFILES[1]).await
}
async fn launch(
    app: &App,
    ch: crate::store::Character,
    world: i32,
    pos: Option<Pos>,
    profile: Profile,
) -> Result<()> {
    let owner = app.claim(ch.account)?;
    let (packets, mut output) = mpsc::channel(64);
    let (lost, _) = watch::channel(false);
    let id = app
        .world
        .in_world(world)
        .call(move |w| match pos {
            None => w.enter_autonomous(ch, Output { packets, lost }),
            Some(pos) => {
                let id = w.enter_at(ch, Some(Output { packets, lost }), Some(pos))?;
                w.players
                    .get_mut(&id)
                    .context("new bot missing")?
                    .controller = crate::world::Controller::Autonomous;
                Ok(id)
            }
        })
        .await?;
    let view = app.world.directory(move |d| d.player_view(id)).await?;
    let mut brain = Brain::new(profile, &view);
    let app = app.clone();
    tokio::spawn(async move {
        let mut shutdown = app.shutdown.subscribe();
        let result:Result<()> = async {
                loop {
                    tokio::select! { _=tokio::time::sleep(Duration::from_millis(50))=>{},_=shutdown.changed()=>break }
                    let view=app.world.directory(move |d|d.player_view(id)).await?;
                    if view.lifecycle.death_pending || !view.lifecycle.alive {
                        if !view.lifecycle.alive && !view.lifecycle.death_pending { break; }
                        while output.try_recv().is_ok() {}
                        continue;
                    }
                    ensure!(matches!(view.lifecycle.output,OutputState::Live) && !view.lifecycle.departing && !view.lifecycle.quarantined,"controller is not live");
                    while let Ok(packet)=output.try_recv() {
                        if brain.receive(packet,&view)? {
                            let received = app.world.clock.now();
                            if let Err(error)=app.world.directory(move |d|d.chat(id,"omw".into(),received)).await {
                                tracing::warn!(player=id,%error,"invitation acknowledgement rejected");
                            }
                            tracing::info!(player=id,companion=brain.companion,"visible invitation accepted");
                        }
                    }
                    if view.tick<brain.next_decision { continue; }
                    let decision=brain.decide(&view,&app.content)?;
                    if decision.goal!=brain.goal {
                        tracing::info!(player=id,name=profile.name,tick=view.tick,hp=view.character.hp,
                            experience=view.character.experience,x=view.position.x,y=view.position.y,
                            goal=?decision.goal,"autonomous decision");
                    }
                    brain.goal=decision.goal;
                    brain.destination=decision.movement;
                    let mut actions=vec![Action::Stop];
                    if let Some(pos)=decision.movement { actions.push(Action::Move{x:pos.x,y:pos.y}); }
                    if let Some(angle)=decision.aim { actions.push(Action::Shoot{angle}); }
                    if let Some(action)=decision.item { actions.push(action); }
                    let diagnostic=format!("{actions:?}");
                    match app.world.for_player(id).call(move |w| {
                        let mut plan=None;
                        for action in actions { if let Some(item)=w.autonomous_action(id,action)? { plan=Some(item); } }
                        Ok(plan)
                    }).await {
                        Ok(Some(plan))=>server::persist(&app,plan).await?,
                        Ok(None)=>{},
                        Err(error)=>tracing::warn!(player=id,%error,actions=diagnostic,"autonomous commands rejected; will reassess"),
                    }
                }
                server::depart(&app,id).await
            }.await;
        if let Err(error) = result {
            tracing::error!(player=id,name=profile.name,%error,"autonomous controller stopped; account retained for diagnosis");
            let reason = error.to_string();
            if let Err(error) = app
                .world
                .for_player(id)
                .call(move |w| w.poison(id, reason))
                .await
            {
                tracing::error!(player=id,%error,"autonomous quarantine failed");
            }
            app.quarantined
                .lock()
                .expect("quarantine registry poisoned")
                .push(owner);
        }
    });
    Ok(())
}
