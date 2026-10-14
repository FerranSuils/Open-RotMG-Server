use crate::{
    content::{Content, Map, Projectile},
    store::{Character, Vault},
    wire::{self, Packet, Writer},
};
use anyhow::{Context, Result, bail, ensure};
use rand::{Rng, SeedableRng, seq::SliceRandom};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicI32, Ordering},
    },
    time::Instant,
};
use tokio::sync::{mpsc, oneshot, watch};
mod chat;
#[cfg(test)]
mod pause_tests;
pub(crate) use chat::{chat_error, chat_private};
#[cfg(test)]
mod bot_tests;
mod cooperative;
#[cfg(test)]
mod cooperative_tests;
mod trade;
pub(crate) mod view;
#[cfg(test)]
mod view_tests;
pub use trade::{TradeAction, TradePlan};
#[derive(Clone)]
pub struct Output {
    pub packets: mpsc::Sender<Packet>,
    pub lost: watch::Sender<bool>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Pos {
    pub x: f32,
    pub y: f32,
}
impl Pos {
    pub fn distance(self, p: Self) -> f32 {
        ((self.x - p.x).powi(2) + (self.y - p.y).powi(2)).sqrt()
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Entity {
    pub id: i32,
    pub kind: u16,
    pub name: String,
    pub pos: Pos,
    pub hp: i32,
    pub size: i32,
    pub condition: i32,
    pub items: Vec<i32>,
    #[serde(skip)]
    drop_slots: BTreeSet<usize>,
    pub extra_stats: Vec<(u8, i32)>,
    pub reserved: bool,
    published: bool,
    damage: BTreeMap<i32, i32>,
    home: Pos,
    goal: Option<Pos>,
    phase: f32,
    next_shot: u64,
    bullet: u8,
}
impl Entity {
    fn accessible(&self, account: i32) -> bool {
        if !self.published {
            return false;
        }
        match self.extra_stats.iter().find(|(stat, _)| *stat == 54) {
            Some((_, owner)) if *owner >= 0 => *owner == account,
            _ => true,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Controller {
    Human,
    Qa,
    Autonomous,
}
pub struct Player {
    pub character: Character,
    pub pos: Pos,
    pub tx: Option<Output>,
    pub departing: bool,
    paused: bool,
    controller: Controller,
    pub known: BTreeSet<i32>,
    known_since: BTreeMap<i32, u64>,
    tiles: BTreeSet<(i16, i16)>,
    pub death_started: bool,
    killer: String,
    movement_credit: f32,
    hp_fraction: f32,
    mp_fraction: f32,
    pub pending: bool,
    pub experience_queued: i32,
    caves_queued: u64,
    progress_started: bool,
    pub death_pending: bool,
    persistence_failure: Option<String>,
    pub bot_goal: Option<Pos>,
    pub bot_aim: Option<f32>,
    pub next_bullet: u8,
    server_bullet: u8,
    seed: u32,
    last_attack: i32,
    last_attack_at: u64,
    attack_credit: f32,
    attack_count: i32,
    last_move: i32,
    last_move_at: u64,
    tick: i32,
    last_ability: u64,
}
#[derive(Clone)]
pub struct Bullet {
    pub owner: i32,
    pub source_kind: u16,
    pub id: u8,
    pub friendly: bool,
    pub start: Pos,
    pub pos: Pos,
    pub angle: f32,
    pub born: u64,
    pub damage: i32,
    pub spec: Projectile,
    pub hit: BTreeSet<i32>,
}
impl Bullet {
    pub fn position(&self, now: u64) -> Pos {
        let age = (now - self.born) as f32;
        let d = age * self.spec.speed / 10000.;
        let phase = if self.id.is_multiple_of(2) {
            0.
        } else {
            std::f32::consts::PI
        };
        let wave = self.spec.amplitude
            * (phase + age / self.spec.lifetime * self.spec.frequency * std::f32::consts::TAU)
                .sin();
        Pos {
            x: self.start.x + d * self.angle.cos() - wave * self.angle.sin(),
            y: self.start.y + d * self.angle.sin() + wave * self.angle.cos(),
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Slot {
    pub object: i32,
    pub slot: usize,
    pub kind: i32,
}
#[derive(Clone)]
pub struct InventoryPlan {
    pub player: i32,
    pub items: Vec<i32>,
    pub revision: i32,
    pub account: i32,
    pub character: i32,
    pub bag: Option<(i32, Vec<i32>)>,
    pub healing: i32,
    pub mana: i32,
    pub vault: Option<Vault>,
    pub dropped: Option<i32>,
    pub collected: Option<u16>,
    loot_slot: Option<(i32, usize)>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Action {
    Move { x: f32, y: f32 },
    Shoot { angle: f32 },
    Stop,
    Use { slot: usize, x: f32, y: f32 },
    Swap { from: Slot, to: Slot },
    Drop { slot: Slot },
}

struct Spawn {
    kind: u16,
    pos: Pos,
    entity: Option<i32>,
    ready: u64,
}
pub struct World {
    pub content: Arc<Content>,
    pub map: Arc<Map>,
    pub entities: BTreeMap<i32, Entity>,
    pub players: BTreeMap<i32, Player>,
    pub bullets: Vec<Bullet>,
    cells: Vec<BTreeSet<i32>>,
    rng: rand::rngs::StdRng,
    pub now: u64,
    attack_epoch: Instant,
    pub tick: u64,
    ids: Arc<AtomicI32>,
    pub name: String,
    pub safe: bool,
    pub owner: Option<i32>,
    pub max_players: usize,
    encounter: bool,
    population: Vec<Spawn>,
    portal_requests: Vec<Pos>,
    bag_expiry: BTreeMap<i32, u64>,
    cleared: bool,
    clear_exit: Option<i32>,
    vault: Option<(i32, Vault)>,
    trades: BTreeMap<i32, trade::Trade>,
    trade_requests: BTreeMap<i32, (i32, u64)>,
}
fn allocate_id(ids: &AtomicI32) -> Result<i32> {
    let mut id = ids.load(Ordering::Relaxed);
    loop {
        let next = id
            .checked_add(1)
            .context("entity identity space exhausted")?;
        match ids.compare_exchange_weak(id, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return Ok(id),
            Err(current) => id = current,
        }
    }
}
#[derive(Clone, Copy)]
enum Target {
    World(i32),
    Player(i32),
}
struct Grant {
    account: i32,
    character: i32,
    destination: i32,
    key_time: i32,
    expires: u64,
}
struct Portal {
    origin: i32,
    destination: i32,
    expires: u64,
}
pub struct Directory {
    pub worlds: BTreeMap<i32, World>,
    templates: BTreeMap<String, Arc<Map>>,
    content: Arc<Content>,
    ids: Arc<AtomicI32>,
    grants: BTreeMap<Vec<u8>, Grant>,
    portals: BTreeMap<i32, Portal>,
    chat_budgets: BTreeMap<i32, chat::ChatBudget>,
    teleports: BTreeMap<i32, Instant>,
    idle: BTreeMap<i32, u64>,
    next_world: i32,
    pub tick: u64,
    pub max_tick_us: u64,
    max_players: usize,
}
impl Directory {
    pub fn competitive_characters(&self) -> Vec<Character> {
        self.worlds
            .values()
            .flat_map(|w| w.players.values())
            .filter(|p| p.controller == Controller::Human && p.persistence_failure.is_none())
            .map(|p| p.character.clone())
            .collect()
    }
    pub fn health(&self, started: Instant) -> serde_json::Value {
        let players: usize = self.worlds.values().map(|w| w.players.len()).sum();
        let qa_actors: usize = self
            .worlds
            .values()
            .flat_map(|w| w.players.values())
            .filter(|p| p.controller == Controller::Qa)
            .count();
        let bots = self
            .worlds
            .values()
            .flat_map(|w| w.players.values())
            .filter(|p| p.controller == Controller::Autonomous)
            .count();
        serde_json::json!({
            "status": "development", "uptime_seconds": started.elapsed().as_secs(),
            "worlds": self.worlds.len(), "players": players - qa_actors - bots, "bots": bots, "qa_actors": qa_actors,
            "entities": self.worlds.values().map(|w| w.entities.len()).sum::<usize>(),
            "projectiles": self.worlds.values().map(|w| w.bullets.len()).sum::<usize>(),
            "tick": self.tick, "max_tick_us": self.max_tick_us,
        })
    }
    #[cfg(test)]
    pub fn load(path: &std::path::Path, content: Arc<Content>, max_players: usize) -> Result<Self> {
        Self::with_seed(path, content, max_players, 0)
    }
    pub fn with_seed(
        path: &std::path::Path,
        content: Arc<Content>,
        max_players: usize,
        seed: u64,
    ) -> Result<Self> {
        let mut templates = BTreeMap::new();
        for name in ["tutorial", "nexus", "nexus-limbo", "vault", "pirate-cave"] {
            templates.insert(
                name.to_string(),
                Arc::new(Map::load(&path.join(format!("{name}.wmap")), &content)?),
            );
        }
        templates.insert(
            "realm-shore".into(),
            Arc::new(crate::terrain::generate(&content, seed, 512)?),
        );
        let mut d = Self {
            worlds: BTreeMap::new(),
            templates,
            content,
            ids: Arc::new(AtomicI32::new(1000)),
            grants: BTreeMap::new(),
            portals: BTreeMap::new(),
            chat_budgets: BTreeMap::new(),
            teleports: BTreeMap::new(),
            idle: BTreeMap::new(),
            next_world: 1,
            tick: 0,
            max_tick_us: 0,
            max_players,
        };
        for (id, template, name, safe) in [
            (-1, "tutorial", "Tutorial", false),
            (-2, "nexus", "Nexus", true),
            (-3, "realm-shore", "Realm", false),
            (-4, "nexus-limbo", "Nexus Limbo", true),
        ] {
            let world = d.make(template, name, safe, None)?;
            d.worlds.insert(id, world);
        }
        let nexus = d.worlds.get_mut(&-2).expect("constructed Nexus");
        let pos = nexus
            .map
            .cells
            .iter()
            .enumerate()
            .find_map(|(i, c)| {
                let pos = Pos {
                    x: (i % nexus.map.width as usize) as f32 + 0.5,
                    y: (i / nexus.map.width as usize) as f32 + 0.5,
                };
                (c.region == 2 && c.object.is_none() && nexus.walkable(pos)).then_some(pos)
            })
            .context("Nexus has no walkable Realm portal region")?;
        nexus.spawn_entity(0x704, pos, vec![])?;
        Ok(d)
    }
    fn make(&self, template: &str, name: &str, safe: bool, owner: Option<i32>) -> Result<World> {
        let mut world = World::with_ids(
            self.content.clone(),
            self.templates
                .get(template)
                .context("world template missing")?
                .clone(),
            self.ids.clone(),
        )?;
        world.name = name.into();
        world.safe = safe;
        world.owner = owner;
        world.max_players = self.max_players;
        world.encounter = matches!(template, "realm-shore" | "pirate-cave");
        if template == "realm-shore" {
            world.populate_shore()?;
        }
        if world.encounter {
            for e in world
                .entities
                .values()
                .filter(|e| world.content.objects[&e.kind].enemy)
            {
                ensure!(
                    crate::encounter::profile(e.kind).is_some(),
                    "missing encounter profile for {:#x}",
                    e.kind
                );
                crate::loot::validate(&world.content, e.kind)?;
            }
        }
        Ok(world)
    }
    fn resolve(&mut self, target: Target) -> Result<&mut World> {
        match target {
            Target::World(id) => self.worlds.get_mut(&id).context("world no longer exists"),
            Target::Player(id) => self
                .worlds
                .values_mut()
                .find(|w| w.players.contains_key(&id))
                .context("player missing from world directory"),
        }
    }
    pub fn location(&self, player: i32) -> Result<i32> {
        self.worlds
            .iter()
            .find(|(_, w)| w.players.contains_key(&player))
            .map(|(id, _)| *id)
            .context("player missing from world directory")
    }
    pub fn initial(&self, game: i32) -> Result<i32> {
        let destination = match game {
            0 | -2 => -2,
            -1 => -1,
            -3 => -3,
            -4 => -4,
            _ => bail!("world entry {game} requires a transfer grant"),
        };
        ensure!(
            self.worlds.contains_key(&destination),
            "entry world missing"
        );
        Ok(destination)
    }
    pub fn instance(&mut self, template: &str, account: Option<i32>) -> Result<i32> {
        ensure!(
            matches!(
                (template, account),
                ("vault", Some(_)) | ("pirate-cave", None)
            ),
            "invalid private/dungeon instance policy"
        );
        if let Some(account) = account
            && let Some((id, _)) = self.worlds.iter().find(|(_, w)| w.owner == Some(account))
        {
            return Ok(*id);
        }
        ensure!(self.worlds.len() < 12, "world instance limit reached");
        let world = self.make(
            template,
            if template == "vault" {
                "Vault"
            } else {
                "Pirate Cave"
            },
            template == "vault",
            account,
        )?;
        let id = self.next_world;
        self.next_world = id
            .checked_add(1)
            .context("world identity space exhausted")?;
        self.worlds.insert(id, world);
        self.idle.insert(id, self.tick);
        tracing::info!(world = id, template, account, "world instance created");
        Ok(id)
    }
    pub fn destination(&mut self, player: i32, portal: Option<i32>) -> Result<(i32, i32, i32)> {
        let origin = self.location(player)?;
        let world = &self.worlds[&origin];
        let p = &world.players[&player];
        ensure!(
            p.character.alive && p.character.hp > 0 && !p.death_pending && !p.departing,
            "character cannot transfer"
        );
        let (account, character) = (p.character.account, p.character.id);
        let kind = if let Some(id) = portal {
            let e = world.entities.get(&id).context("portal missing")?;
            ensure!(e.published, "portal not published");
            ensure!(p.pos.distance(e.pos) <= 2., "portal out of reach");
            e.kind
        } else {
            0x712
        };
        let destination = match kind {
            0x703 | 0x70d | 0x712 | 0x71d => -2,
            0x704 | 0x70e | 0x71c => -3,
            0x720 => self.instance("vault", Some(account))?,
            0x717 => {
                let binding = self
                    .portals
                    .get(&portal.context("dungeon portal identity missing")?)
                    .context("dropped portal has no live instance binding")?;
                ensure!(
                    binding.origin == origin && binding.expires > self.tick,
                    "dungeon portal binding expired or belongs to another world"
                );
                binding.destination
            }
            _ => bail!("unsupported portal destination {kind:#x}"),
        };
        ensure!(destination != origin, "already in destination world");
        Ok((account, character, destination))
    }
    pub fn issue(
        &mut self,
        account: i32,
        character: i32,
        destination: i32,
    ) -> Result<(i32, Vec<u8>, String)> {
        ensure!(self.grants.len() < 64, "transfer grant limit reached");
        let world = self
            .worlds
            .get(&destination)
            .context("transfer destination missing")?;
        ensure!(
            world.owner.is_none_or(|owner| owner == account),
            "private destination belongs to another account"
        );
        let name = world.name.clone();
        let mut key = vec![0; 16];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut key);
        let key_time = i32::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs(),
        )?;
        self.grants.insert(
            key.clone(),
            Grant {
                account,
                character,
                destination,
                key_time,
                expires: self.tick + 2400,
            },
        );
        tracing::info!(
            account,
            character,
            destination,
            expires_tick = self.tick + 2400,
            "transfer grant issued"
        );
        Ok((key_time, key, name))
    }
    pub fn consume(
        &mut self,
        account: i32,
        destination: i32,
        key_time: i32,
        key: Vec<u8>,
    ) -> Result<i32> {
        let grant = self
            .grants
            .get(&key)
            .context("unknown or already used transfer grant")?;
        ensure!(grant.expires > self.tick, "transfer grant expired");
        ensure!(
            grant.account == account
                && grant.destination == destination
                && grant.key_time == key_time,
            "transfer grant binding mismatch"
        );
        ensure!(
            self.worlds.contains_key(&destination),
            "transfer destination expired"
        );
        let character = grant.character;
        self.grants.remove(&key);
        if destination > 0 {
            self.idle.insert(destination, self.tick);
        }
        tracing::info!(account, character, destination, "transfer grant consumed");
        Ok(character)
    }
    fn drop_portal(&mut self, origin: i32, pos: Pos) -> Result<i32> {
        ensure!(
            origin == -3 && self.worlds[&origin].walkable(pos),
            "dungeon portal must be earned on the walkable Realm shore"
        );
        let destination = self.instance("pirate-cave", None)?;
        let id = self
            .worlds
            .get_mut(&origin)
            .unwrap()
            .spawn_entity(0x717, pos, vec![])?;
        self.portals.insert(
            id,
            Portal {
                origin,
                destination,
                expires: self.tick + 600,
            },
        );
        tracing::info!(
            portal = id,
            origin,
            destination,
            "earned Pirate Cave portal dropped"
        );
        Ok(id)
    }
    fn step(&mut self) {
        self.tick += 1;
        self.grants.retain(|_, grant| grant.expires > self.tick);
        for (id, world) in &mut self.worlds {
            world.step();
            if *id > 0 {
                if world.players.is_empty() {
                    self.idle.entry(*id).or_insert(self.tick);
                } else {
                    self.idle.remove(id);
                }
            }
        }
        let requests: Vec<_> = self
            .worlds
            .iter_mut()
            .flat_map(|(id, world)| {
                std::mem::take(&mut world.portal_requests)
                    .into_iter()
                    .map(|pos| (*id, pos))
            })
            .collect();
        for (origin, pos) in requests {
            if let Err(error) = self.drop_portal(origin, pos) {
                tracing::error!(origin,%error,"earned dungeon portal creation failed");
            }
        }
        let expired_portals: Vec<_> = self
            .portals
            .iter()
            .filter(|(_, binding)| binding.expires <= self.tick)
            .map(|(id, binding)| (*id, binding.origin))
            .collect();
        for (id, origin) in expired_portals {
            self.worlds
                .get_mut(&origin)
                .expect("portal origin exists")
                .remove_entity(id);
            self.portals.remove(&id);
            tracing::info!(portal = id, origin, "dungeon portal expired");
        }
        let expired: Vec<_> = self
            .idle
            .iter()
            .filter(|(id, since)| {
                self.tick - **since > 6000
                    && !self.grants.values().any(|grant| grant.destination == **id)
                    && !self
                        .portals
                        .values()
                        .any(|portal| portal.destination == **id)
            })
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            self.worlds.remove(&id);
            self.idle.remove(&id);
            tracing::info!(world = id, "idle world removed");
        }
    }
}
type Job = Box<dyn FnOnce(&mut Directory) + Send>;
#[derive(Clone)]
pub struct Handle {
    tx: mpsc::Sender<Job>,
    target: Target,
}
impl Handle {
    pub fn spawn(directory: Directory) -> Self {
        let (tx, mut rx) = mpsc::channel::<Job>(256);
        tokio::spawn(async move {
            let mut directory = directory;
            let mut timer = tokio::time::interval(std::time::Duration::from_millis(50));
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = timer.tick() => {
                        let start=Instant::now(); directory.step();
                        let us=start.elapsed().as_micros() as u64;
                        directory.max_tick_us=directory.max_tick_us.max(us);
                        if us>25_000 {tracing::warn!(tick=directory.tick,elapsed_us=us,"slow aggregate simulation tick");}
                        if directory.tick.is_multiple_of(100) {
                            tracing::info!(tick=directory.tick,worlds=directory.worlds.len(),
                                players=directory.worlds.values().map(|w|w.players.len()).sum::<usize>(),
                                entities=directory.worlds.values().map(|w|w.entities.len()).sum::<usize>(),
                                max_tick_us=directory.max_tick_us,"directory health");
                            directory.max_tick_us=0;
                        }
                    },
                    job=rx.recv()=>match job{Some(f)=>f(&mut directory),None=>break}
                }
            }
        });
        Self {
            tx,
            target: Target::World(-1),
        }
    }
    pub fn in_world(&self, id: i32) -> Self {
        Self {
            tx: self.tx.clone(),
            target: Target::World(id),
        }
    }
    pub fn for_player(&self, id: i32) -> Self {
        Self {
            tx: self.tx.clone(),
            target: Target::Player(id),
        }
    }
    pub async fn directory<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Directory) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Box::new(move |d| {
                let _ = tx.send(f(d));
            }))
            .await
            .map_err(|_| anyhow::anyhow!("world directory stopped"))?;
        rx.await.context("world directory worker failed")?
    }
    pub async fn collect<T: Send + 'static>(
        &self,
        mut f: impl FnMut(&mut World) -> Result<Vec<T>> + Send + 'static,
    ) -> Result<Vec<T>> {
        self.directory(move |d| {
            let mut values = vec![];
            for world in d.worlds.values_mut() {
                values.extend(f(world)?);
            }
            Ok(values)
        })
        .await
    }
    pub async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut World) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let target = self.target;
        self.directory(move |d| f(d.resolve(target)?)).await
    }
}
fn send(p: &Player, id: u8, body: Vec<u8>) {
    if let Some(tx) = &p.tx
        && tx.packets.try_send(Packet { id, body }).is_err()
    {
        tx.lost.send_replace(true);
        tracing::warn!(
            character = p.character.id,
            "slow/closed connection; packet queue full"
        );
    }
}
fn stat(out: &mut Writer, id: u8, v: i32) {
    out.u8(id).i32(v);
}
fn text_stat(out: &mut Writer, id: u8, v: &str) {
    out.u8(id).utf(v);
}
impl World {
    #[cfg(test)]
    pub fn new(content: Arc<Content>, map: Arc<Map>) -> Result<Self> {
        Self::with_ids(content, map, Arc::new(AtomicI32::new(1000)))
    }
    fn with_ids(content: Arc<Content>, map: Arc<Map>, ids: Arc<AtomicI32>) -> Result<Self> {
        let mut entities = BTreeMap::new();
        let mut cells = vec![BTreeSet::new(); map.cells.len()];
        for (y, row) in map.cells.chunks(map.width as usize).enumerate() {
            for (x, c) in row.iter().enumerate() {
                if let Some(kind) = c.object {
                    let id = allocate_id(&ids)?;
                    let o = content.get(kind)?;
                    let pos = Pos {
                        x: x as f32 + 0.5,
                        y: y as f32 + 0.5,
                    };
                    let mut e = Entity {
                        id,
                        kind,
                        name: String::new(),
                        pos,
                        home: pos,
                        goal: None,
                        phase: 0.,
                        hp: o.hp,
                        size: 100,
                        condition: 0,
                        items: o.equipment.clone(),
                        drop_slots: BTreeSet::new(),
                        extra_stats: if o.class == "Portal" {
                            vec![(37, 1)]
                        } else {
                            vec![]
                        },
                        reserved: false,
                        published: true,
                        damage: BTreeMap::new(),
                        next_shot: 0,
                        bullet: 0,
                    };
                    for setting in c.config.split(';').filter(|s| !s.is_empty()) {
                        let (k, v) = setting.split_once(':').context("malformed map setting")?;
                        match k {
                            "name" => e.name = v.into(),
                            "size" => e.size = v.parse()?,
                            "eff" => {
                                e.condition = i32::from_str_radix(v.trim_start_matches("0x"), 16)?
                            }
                            "conn" | "mtype" | "mcount" | "mtime" | "nstar" => {
                                let value = if let Some(hex) = v.strip_prefix("0x") {
                                    u32::from_str_radix(hex, 16)? as i32
                                } else {
                                    v.parse()?
                                };
                                let stat = match k {
                                    "conn" => 41,
                                    "mtype" => 34,
                                    "mcount" => 42,
                                    "mtime" => 43,
                                    "nstar" => 55,
                                    _ => unreachable!(),
                                };
                                e.extra_stats.push((stat, value));
                            }
                            _ => bail!("unsupported map setting {k}"),
                        }
                    }
                    cells[y * map.width as usize + x].insert(id);
                    entities.insert(id, e);
                }
            }
        }
        Ok(Self {
            content,
            map,
            entities,
            players: BTreeMap::new(),
            bullets: Vec::new(),
            cells,
            rng: rand::rngs::StdRng::from_entropy(),
            now: 0,
            attack_epoch: Instant::now(),
            tick: 0,
            ids,
            name: "Development tutorial".into(),
            safe: false,
            owner: None,
            max_players: 85,
            encounter: false,
            population: vec![],
            portal_requests: vec![],
            bag_expiry: BTreeMap::new(),
            cleared: false,
            clear_exit: None,
            vault: None,
            trades: BTreeMap::new(),
            trade_requests: BTreeMap::new(),
        })
    }
    pub fn load_vault(&mut self, vault: Vault) -> Result<()> {
        ensure!(
            self.name == "Vault" && self.owner == Some(vault.account),
            "vault world owner mismatch"
        );
        ensure!(
            vault.items.len() == 8,
            "vault must contain exactly eight slots"
        );
        if let Some((id, loaded)) = &self.vault {
            ensure!(
                loaded.revision == vault.revision && self.entities[id].items == vault.items,
                "loaded vault disagrees with durable revision"
            );
            return Ok(());
        }
        let (x, y) = self.map.spawns[0];
        let spawn = Pos { x, y };
        let pos = self
            .map
            .cells
            .iter()
            .enumerate()
            .filter(|(_, c)| c.region == 12)
            .map(|(i, _)| Pos {
                x: (i % self.map.width as usize) as f32 + 0.5,
                y: (i / self.map.width as usize) as f32 + 0.5,
            })
            .min_by(|a, b| a.distance(spawn).total_cmp(&b.distance(spawn)))
            .context("sourced Vault has no chest region")?;
        let id = self.spawn_entity(0x504, pos, vault.items.clone())?;
        self.entities
            .get_mut(&id)
            .unwrap()
            .extra_stats
            .push((54, vault.account));
        tracing::info!(
            entity = id,
            account = vault.account,
            revision = vault.revision,
            "private eight-slot chest loaded"
        );
        self.vault = Some((id, vault));
        Ok(())
    }
    pub fn spawn_entity(&mut self, kind: u16, pos: Pos, items: Vec<i32>) -> Result<i32> {
        ensure!(self.walkable(pos), "entity spawn is blocked");
        let cell = pos.y.floor() as usize * self.map.width as usize + pos.x.floor() as usize;
        let definition = self.content.get(kind)?;
        let id = allocate_id(&self.ids)?;
        let extra_stats = if definition.class == "Portal" {
            vec![(37, 1)]
        } else {
            vec![]
        };
        self.entities.insert(
            id,
            Entity {
                id,
                kind,
                name: String::new(),
                pos,
                home: pos,
                goal: None,
                phase: 0.,
                hp: definition.hp,
                size: 100,
                condition: 0,
                items,
                drop_slots: BTreeSet::new(),
                extra_stats,
                reserved: false,
                published: true,
                damage: BTreeMap::new(),
                next_shot: 0,
                bullet: 0,
            },
        );
        self.cells[cell].insert(id);
        Ok(id)
    }
    fn remove_entity(&mut self, id: i32) -> Entity {
        let entity = self.entities.remove(&id).expect("indexed entity missing");
        assert!(!entity.reserved, "cannot remove a reserved container");
        self.bag_expiry.remove(&id);
        let cell = entity.pos.y as usize * self.map.width as usize + entity.pos.x as usize;
        assert!(
            self.cells[cell].remove(&id),
            "entity absent from spatial index"
        );
        entity
    }
    pub fn map_info(&self) -> Vec<u8> {
        let mut o = Writer::new();
        o.i32(self.map.width)
            .i32(self.map.height)
            .utf(&self.name)
            .u32(1)
            .i32(0)
            .u8(u8::from(self.allow_teleport()))
            .u8(1)
            .i16(0)
            .i16(0);
        o.0
    }
    pub fn enter(&mut self, ch: Character, tx: Option<Output>) -> Result<i32> {
        ensure!(ch.alive && ch.hp > 0, "dead character");
        ch.effective(&self.content)?;
        ensure!(
            self.owner.is_none_or(|owner| owner == ch.account),
            "private world belongs to another account"
        );
        ensure!(
            self.players.len() < self.max_players,
            "development world full"
        );
        ensure!(
            !self
                .players
                .values()
                .any(|p| p.character.account == ch.account),
            "account already in this world"
        );
        let id = allocate_id(&self.ids)?;
        let (x, y) = self.map.spawns[self.players.len() % self.map.spawns.len()];
        let char_id = ch.id;
        self.players.insert(
            id,
            Player {
                character: ch,
                pos: Pos { x, y },
                departing: false,
                paused: false,
                controller: if tx.is_none() {
                    Controller::Qa
                } else {
                    Controller::Human
                },
                tx,
                known: BTreeSet::new(),
                known_since: BTreeMap::new(),
                tiles: BTreeSet::new(),
                death_started: false,
                killer: String::new(),
                movement_credit: 0.78,
                hp_fraction: 0.,
                mp_fraction: 0.,
                pending: false,
                experience_queued: 0,
                caves_queued: 0,
                progress_started: false,
                death_pending: false,
                persistence_failure: None,
                bot_goal: None,
                bot_aim: None,
                next_bullet: 1,
                server_bullet: 128,
                seed: 1,
                last_attack: -1000,
                last_attack_at: 0,
                attack_credit: 1000.,
                attack_count: 0,
                last_move: 0,
                last_move_at: self.now,
                tick: 0,
                last_ability: 0,
            },
        );
        let mut o = Writer::new();
        o.i32(id).i32(char_id);
        send(&self.players[&id], wire::CREATE_SUCCESS, o.0);
        self.updates(id, true)?;
        tracing::info!(
            player = id,
            character = char_id,
            world = %self.name,
            "player entered development world"
        );
        Ok(id)
    }
    pub fn enter_autonomous(&mut self, ch: Character, tx: Output) -> Result<i32> {
        let id = self.enter(ch, Some(tx))?;
        self.players
            .get_mut(&id)
            .context("new controller missing")?
            .controller = Controller::Autonomous;
        Ok(id)
    }
    pub fn leave(&mut self, id: i32) -> Result<Character> {
        self.cancel_trade(id, "Trade cancelled by departure.");
        let p = self.players.get(&id).context("player not found")?;
        ensure!(
            !p.pending && !p.death_pending,
            "inventory or death operation pending"
        );
        let p = self.players.remove(&id).unwrap();
        self.bullets.retain(|b| b.owner != id);
        tracing::info!(player = id, character = p.character.id, "player left");
        Ok(p.character)
    }
    pub fn stats(&self, id: i32) -> Result<Vec<u8>> {
        let mut o = Writer::new();
        if let Some(p) = self.players.get(&id) {
            let values = p.character.effective(&self.content)?;
            o.i32(id).f32(p.pos.x).f32(p.pos.y).i16(37);
            for (k, v) in [
                (0, values[0]),
                (1, p.character.hp),
                (2, 100),
                (3, values[1]),
                (4, p.character.mp),
                (5, crate::store::experience_goal(p.character.level)?),
                (
                    6,
                    p.character.exp - crate::store::level_start(p.character.level)?,
                ),
                (7, p.character.level),
            ] {
                stat(&mut o, k, v);
            }
            for (i, item) in p.character.items.iter().enumerate() {
                stat(&mut o, 8 + i as u8, *item);
            }
            for (k, v) in [
                (20, values[2]),
                (21, values[3]),
                (22, values[4]),
                (26, values[6]),
                (27, values[7]),
                (28, values[5]),
                (29, if p.paused { 1 << 20 } else { 0 }), // Selected 123.5.1 Paused mask.
                (30, 0),
            ] {
                stat(&mut o, k, v);
            }
            text_stat(&mut o, 31, &p.character.name);
            stat(&mut o, 32, 0);
            stat(&mut o, 33, 0);
            stat(&mut o, 38, p.character.account);
            stat(
                &mut o,
                53,
                i32::from(crate::store::name_chosen(
                    p.character.account,
                    &p.character.name,
                )),
            );
            stat(&mut o, 57, p.character.fame);
            stat(
                &mut o,
                58,
                [20, 150, 400, 800, 2000]
                    .into_iter()
                    .find(|n| *n > p.character.fame)
                    .unwrap_or(0),
            );
            text_stat(&mut o, 62, "");
            stat(&mut o, 63, 0);
        } else {
            let e = self.entities.get(&id).context("entity missing")?;
            o.i32(id).f32(e.pos.x).f32(e.pos.y).i16(
                (4 + usize::from(!e.name.is_empty()) + e.items.len() + e.extra_stats.len()) as i16,
            );
            stat(&mut o, 0, self.content.get(e.kind)?.hp);
            stat(&mut o, 1, e.hp);
            stat(&mut o, 2, e.size);
            stat(&mut o, 29, e.condition);
            if !e.name.is_empty() {
                text_stat(&mut o, 31, &e.name);
            }
            for (i, item) in e.items.iter().enumerate() {
                stat(&mut o, 8 + i as u8, *item);
            }
            for (id, value) in &e.extra_stats {
                stat(&mut o, *id, *value);
            }
        }
        Ok(o.0)
    }
    fn defense(&self, id: i32) -> Result<i32> {
        let p = &self.players[&id];
        Ok(p.character.effective(&self.content)?[3])
    }
    fn visible(&self, id: i32) -> BTreeSet<i32> {
        let pos = self.players[&id].pos;
        self.entities
            .values()
            .filter(|e| {
                e.pos.distance(pos) < 20. && e.accessible(self.players[&id].character.account)
            })
            .map(|e| e.id)
            .chain(
                self.players
                    .iter()
                    .filter(|(_, p)| p.pos.distance(pos) < 20.)
                    .map(|(i, _)| *i),
            )
            .collect()
    }
    pub fn updates(&mut self, id: i32, initial: bool) -> Result<()> {
        let visible = self.visible(id);
        let previous = &self.players[&id].known;
        let mut added: Vec<i32> = visible.difference(previous).copied().collect();
        // Private-container owner stats dereference the native map's self player.
        added.sort_by_key(|entity| (*entity != id, *entity));
        let dropped: Vec<i32> = previous.difference(&visible).copied().collect();
        let pos = self.players[&id].pos;
        let mut tiles = Vec::new();
        for y in ((pos.y as i32 - 20).max(0))..=(pos.y as i32 + 20).min(self.map.height - 1) {
            for x in ((pos.x as i32 - 20).max(0))..=(pos.x as i32 + 20).min(self.map.width - 1) {
                if !self.players[&id].tiles.contains(&(x as i16, y as i16)) {
                    tiles.push((
                        x as i16,
                        y as i16,
                        self.map.cells[(y * self.map.width + x) as usize].ground,
                    ));
                }
            }
        }
        if initial || !tiles.is_empty() || !added.is_empty() || !dropped.is_empty() {
            let mut o = Writer::new();
            o.i16(i16::try_from(tiles.len())?);
            let p = self.players.get_mut(&id).unwrap();
            for (x, y, ground) in tiles {
                p.tiles.insert((x, y));
                p.character.tiles += 1;
                o.i16(x).i16(y).u8(ground);
            }
            o.i16(added.len() as i16);
            for entity in added {
                let kind = if self.players.contains_key(&entity) {
                    self.players[&entity].character.class
                } else {
                    self.entities[&entity].kind
                };
                o.i16(kind as i16);
                o.0.extend(self.stats(entity)?);
            }
            o.i16(dropped.len() as i16);
            for entity in dropped {
                o.i32(entity);
            }
            send(&self.players[&id], wire::UPDATE, o.0);
        }
        let p = self.players.get_mut(&id).unwrap();
        p.known_since.retain(|entity, _| visible.contains(entity));
        for entity in &visible {
            p.known_since.entry(*entity).or_insert(self.now);
        }
        p.known = visible;
        Ok(())
    }
    fn validate_move(&self, id: i32, pos: Pos, time: i32) -> Result<f32> {
        ensure!(
            pos.x.is_finite() && pos.y.is_finite(),
            "non-finite movement"
        );
        let p = self.players.get(&id).context("player missing")?;
        ensure!(
            p.character.alive
                && !p.death_pending
                && !p.departing
                && p.persistence_failure.is_none(),
            "dead, departing or quarantined player cannot move"
        );
        ensure!(time >= 0, "negative movement time");
        let elapsed = if p.last_move == 0 {
            100.
        } else {
            (i64::from(time) - i64::from(p.last_move)) as f32
        };
        ensure!(
            elapsed >= 0.,
            "movement time reversed: client_time={time} previous_client_time={}",
            p.last_move
        );
        let cell = self
            .map
            .cell(p.pos.x, p.pos.y)
            .context("player outside map")?;
        let speed = (0.004
            + (0.0096 - 0.004) * p.character.effective(&self.content)?[4] as f32 / 75.)
            * self.content.grounds[&cell.ground];
        let distance = p.pos.distance(pos);
        ensure!(
            distance <= speed * elapsed + 0.3,
            "movement exceeds speed: from=({}, {}) to=({}, {}) distance={distance} allowed={} speed={speed} client_elapsed_ms={elapsed}",
            p.pos.x,
            p.pos.y,
            pos.x,
            pos.y,
            speed * elapsed + 0.3
        );
        let server_elapsed = self
            .now
            .checked_sub(p.last_move_at)
            .context("movement server clock regressed")?;
        let credit = (p.movement_credit + speed * server_elapsed as f32).min(speed * 350. + 0.3);
        ensure!(
            distance <= credit + 0.001,
            "movement exceeds server time budget: distance={distance} credit={credit} server_elapsed_ms={server_elapsed} client_elapsed_ms={elapsed} previous_credit={}",
            p.movement_credit
        );
        let samples = (distance / 0.1).ceil().max(1.) as usize;
        ensure!(
            (1..=samples).all(|i| {
                let f = i as f32 / samples as f32;
                self.walkable(Pos {
                    x: p.pos.x + (pos.x - p.pos.x) * f,
                    y: p.pos.y + (pos.y - p.pos.y) * f,
                })
            }),
            "movement across a blocked tile"
        );
        Ok(credit - distance)
    }
    pub fn move_to(&mut self, id: i32, pos: Pos, time: i32) -> Result<()> {
        let p = self.players.get(&id).context("player missing")?;
        ensure!(
            p.last_move == 0 || i64::from(time) - i64::from(p.last_move) <= 1000,
            "movement time jump"
        );
        // MOVE is sent before the client applies NEW_TICK conditions, so either
        // form can be in flight across a toggle. Neither may move a paused player.
        let sentinel = pos.x == -1. && pos.y == -1.;
        let correct = p.paused && !sentinel && p.pos.distance(pos) > 0.001;
        let stationary = p.paused || sentinel;
        let pos = if stationary {
            ensure!(
                pos.x.is_finite() && pos.y.is_finite(),
                "non-finite movement"
            );
            p.pos
        } else {
            pos
        };
        let remaining_credit = self.validate_move(id, pos, time)?;
        let p = self.players.get_mut(&id).unwrap();
        if !stationary {
            p.movement_credit = remaining_credit;
        }
        p.pos = pos;
        p.last_move = time;
        p.last_move_at = self.now;
        if correct {
            self.correct_position(id, time, "paused movement")?;
        }
        Ok(())
    }
    pub fn walkable(&self, pos: Pos) -> bool {
        let Some(c) = self.map.cell(pos.x, pos.y) else {
            return false;
        };
        if self.content.grounds[&c.ground] <= 0. {
            return false;
        }
        self.entities_at(pos)
            .all(|e| !self.content.objects[&e.kind].blocks)
    }
    fn entities_at(&self, pos: Pos) -> impl Iterator<Item = &Entity> {
        self.map.cell(pos.x, pos.y).into_iter().flat_map(move |_| {
            self.cells[pos.y as usize * self.map.width as usize + pos.x as usize]
                .iter()
                .map(|id| self.entities.get(id).expect("occupied cell missing entity"))
        })
    }
    fn near_entities(&self, a: Pos, b: Pos) -> impl Iterator<Item = &Entity> {
        let x0 = (a.x.min(b.x) as i32 - 1).max(0);
        let x1 = (a.x.max(b.x) as i32 + 1).min(self.map.width - 1);
        let y0 = (a.y.min(b.y) as i32 - 1).max(0);
        let y1 = (a.y.max(b.y) as i32 + 1).min(self.map.height - 1);
        (y0..=y1).flat_map(move |y| {
            (x0..=x1).flat_map(move |x| {
                self.cells[(y * self.map.width + x) as usize]
                    .iter()
                    .map(|id| self.entities.get(id).expect("occupied cell missing entity"))
            })
        })
    }
    pub fn native_shoot(
        &mut self,
        id: i32,
        time: i32,
        bullet: u8,
        kind: u16,
        start: Pos,
        angle: f32,
    ) -> Result<()> {
        // Skipped actor ticks must not charge real client attacks against a slower simulation clock.
        let received = self.attack_epoch.elapsed().as_millis() as u64;
        self.shoot_timed(id, (time, received), bullet, kind, start, angle)
    }
    // Fixture tests drive fixed ticks without sleeping, so script shots use simulation time.
    pub fn shoot(
        &mut self,
        id: i32,
        time: i32,
        bullet: u8,
        kind: u16,
        start: Pos,
        angle: f32,
    ) -> Result<()> {
        self.shoot_timed(id, (time, self.now), bullet, kind, start, angle)
    }
    fn shoot_timed(
        &mut self,
        id: i32,
        (time, received): (i32, u64),
        bullet: u8,
        kind: u16,
        start: Pos,
        angle: f32,
    ) -> Result<()> {
        ensure!(angle.is_finite(), "invalid shot angle");
        ensure!(
            start.x.is_finite() && start.y.is_finite(),
            "non-finite shot origin"
        );
        let p = self.players.get(&id).context("player missing")?;
        ensure!(
            p.character.alive
                && !p.death_pending
                && !p.departing
                && p.persistence_failure.is_none(),
            "dead, departing or quarantined player cannot shoot"
        );
        ensure!(
            p.character.items[0] == kind as i32,
            "weapon does not match equipped item"
        );
        // SHOOT can precede MOVE, and stationary clients need not send MOVE; prediction cannot advance movement credit.
        let center = Pos {
            x: start.x - 0.3 * angle.cos(),
            y: start.y - 0.3 * angle.sin(),
        };
        if !p.paused && p.pos.distance(center) > 0.001 {
            self.validate_move(id, center, time)
                .with_context(|| format!("invalid shot origin: player=({}, {}) origin=({}, {}) angle={} shot_time={} move_time={} server_time={} last_move_at={}", p.pos.x, p.pos.y, start.x, start.y, angle, time, p.last_move, self.now, p.last_move_at))?;
        }
        let item = self.content.get(kind)?;
        let spec = item
            .projectiles
            .get(&0)
            .cloned()
            .context("weapon has no projectile zero")?;
        // The selected client's integer attack period cannot fund a fractional charge indefinitely.
        let cooldown = (1.
            / ((0.0015 + (0.008 - 0.0015) * p.character.effective(&self.content)?[5] as f64 / 75.)
                * f64::from(item.rate)))
        .floor() as f32;
        let p = self.players.get_mut(&id).unwrap();
        ensure!(time >= 0, "negative shot time");
        if time != p.last_attack {
            ensure!(
                i64::from(time) - i64::from(p.last_attack) >= cooldown as i64 - 2,
                "attack too fast"
            );
            let elapsed = received
                .checked_sub(p.last_attack_at)
                .context("attack receive clock regressed")?;
            let credit = (p.attack_credit + elapsed as f32).min(cooldown * 2.);
            ensure!(
                credit >= cooldown - 50.,
                "attack exceeds server time budget: credit={credit} cooldown={cooldown} received_time={received} simulation_time={} last_received_time={} shot_time={time} last_shot_time={}",
                self.now,
                p.last_attack_at,
                p.last_attack
            );
            p.attack_credit = credit - cooldown;
            p.last_attack_at = received;
            p.last_attack = time;
            p.attack_count = 0;
        }
        ensure!(
            p.attack_count < item.shots,
            "too many projectiles in attack"
        );
        ensure!(bullet == p.next_bullet, "unexpected player bullet counter");
        p.next_bullet = (bullet + 1) % 128;
        p.attack_count += 1;
        let draw = if spec.min == spec.max {
            spec.min
        } else {
            p.seed = ((p.seed as u64 * 16807) % 2147483647) as u32;
            spec.min + (p.seed % (spec.max - spec.min) as u32) as i32
        };
        // In-flight native SHOOT packets must still consume their counter/RNG draw.
        if p.paused {
            return Ok(());
        }
        p.character.shots += 1;
        let damage = (draw as f32
            * (0.5 + 1.5 * p.character.effective(&self.content)?[2] as f32 / 75.))
            as i32;
        self.bullets.push(Bullet {
            owner: id,
            source_kind: p.character.class,
            id: bullet,
            friendly: true,
            start,
            pos: start,
            angle,
            born: self.now,
            damage,
            spec,
            hit: BTreeSet::new(),
        });
        let mut o = Writer::new();
        o.u8(bullet).i32(id).i16(kind as i16).f32(angle);
        for (other, p) in &self.players {
            if *other != id && p.known.contains(&id) {
                send(p, 74, o.0.clone());
            }
        }
        tracing::debug!(player = id, bullet, damage, "player shot");
        Ok(())
    }
    pub fn use_item(&mut self, id: i32, slot: Slot, pos: Pos) -> Result<Option<InventoryPlan>> {
        self.cancel_trade(id, "Trade cancelled by inventory mutation.");
        let p = self.players.get(&id).context("player missing")?;
        ensure!(!p.paused, "Cannot use items while paused.");
        ensure!(
            !p.pending
                && !p.death_pending
                && !p.departing
                && p.character.alive
                && p.character.hp > 0
                && p.persistence_failure.is_none(),
            "player is busy, departing, quarantined or dead"
        );
        ensure!(
            slot.object == id && slot.slot < 12 && p.character.items[slot.slot] == slot.kind,
            "item ownership mismatch"
        );
        ensure!(slot.kind >= 0, "empty item slot");
        let item = self.content.get(slot.kind as u16)?.clone();
        match item.activate.as_str() {
            "Heal" | "Magic" => {
                let mut items = p.character.items.clone();
                items[slot.slot] = -1;
                self.players.get_mut(&id).unwrap().pending = true;
                let mut plan = self.plan(id, items, None, 0);
                if item.activate == "Heal" {
                    plan.healing = item.heal;
                } else {
                    plan.mana = item.heal;
                }
                Ok(Some(plan))
            }
            "HealNova" => {
                self.heal_nova(id, slot, &item)?;
                Ok(None)
            }
            "BulletNova" => {
                ensure!(
                    slot.slot == 1
                        && item.slot_type == self.content.get(p.character.class)?.slots[1],
                    "ability must be equipped by a compatible class"
                );
                ensure!(p.character.mp >= item.mana_cost, "not enough mana");
                ensure!(
                    self.now >= p.last_ability + item.cooldown_ms,
                    "ability cooldown"
                );
                ensure!(p.pos.distance(pos) <= 15., "ability target too far");
                let spec = item
                    .projectiles
                    .get(&0)
                    .cloned()
                    .context("spell has no projectile zero")?;
                let p = self.players.get_mut(&id).unwrap();
                p.character.mp -= item.mana_cost;
                p.last_ability = self.now;
                p.character.shots += 20;
                let mut messages = Vec::new();
                for i in 0..20 {
                    let bullet = p.server_bullet;
                    p.server_bullet = 128 + (bullet.wrapping_add(1) % 128);
                    let angle = i as f32 * std::f32::consts::TAU / 20.;
                    let damage = spec.min;
                    self.bullets.push(Bullet {
                        owner: id,
                        source_kind: p.character.class,
                        id: bullet,
                        friendly: true,
                        start: pos,
                        pos,
                        angle,
                        born: self.now,
                        damage,
                        spec: spec.clone(),
                        hit: BTreeSet::new(),
                    });
                    let mut o = Writer::new();
                    o.u8(bullet)
                        .i32(id)
                        .i16(slot.kind as i16)
                        .f32(pos.x)
                        .f32(pos.y)
                        .f32(angle)
                        .i16(damage as i16);
                    messages.push(o.0);
                }
                for body in messages {
                    for p in self.players.values() {
                        if p.known.contains(&id) {
                            send(p, 13, body.clone());
                        }
                    }
                }
                tracing::info!(player = id, "spell cast");
                Ok(None)
            }
            _ => bail!("item activation not implemented: {}", item.activate),
        }
    }
    fn plan(
        &self,
        id: i32,
        items: Vec<i32>,
        bag: Option<(i32, Vec<i32>)>,
        healing: i32,
    ) -> InventoryPlan {
        let p = &self.players[&id];
        InventoryPlan {
            player: id,
            items,
            revision: p.character.revision,
            account: p.character.account,
            character: p.character.id,
            bag,
            healing,
            mana: 0,
            vault: None,
            dropped: None,
            collected: None,
            loot_slot: None,
        }
    }
    pub fn drop_item(&mut self, id: i32, slot: Slot) -> Result<InventoryPlan> {
        self.cancel_trade(id, "Trade cancelled by inventory mutation.");
        let p = self.players.get(&id).context("drop owner missing")?;
        ensure!(
            !p.pending && !p.death_pending && !p.departing && p.character.alive,
            "player busy, departing or dead"
        );
        let pos = p.pos;
        let account = p.character.account;
        let mut items = p.character.items.clone();
        let mut source = if slot.object == id {
            None
        } else {
            let e = self
                .entities
                .get(&slot.object)
                .context("drop container missing")?;
            ensure!(
                !e.reserved
                    && e.accessible(account)
                    && pos.distance(e.pos) < 1.5
                    && self.content.get(e.kind)?.class == "Container",
                "drop container inaccessible"
            );
            Some((slot.object, e.items.clone()))
        };
        let inventory = match &mut source {
            Some((_, items)) => items,
            None => &mut items,
        };
        ensure!(
            slot.kind >= 0 && inventory.get(slot.slot) == Some(&slot.kind),
            "empty or stale drop slot"
        );
        let soulbound = self.content.get(u16::try_from(slot.kind)?)?.soulbound;
        inventory[slot.slot] = -1;
        let dropped = self.loot_bag(pos, soulbound.then_some(account), vec![slot.kind])?;
        let e = self.entities.get_mut(&dropped).unwrap();
        e.kind = if soulbound { 0x503 } else { 0x500 };
        e.published = false;
        e.reserved = true;
        if let Some((source, _)) = &source {
            self.entities.get_mut(source).unwrap().reserved = true;
        }
        self.players.get_mut(&id).unwrap().pending = true;
        let mut plan = self.plan(id, items, source, 0);
        if let (Some((container, vault)), Some((source, items))) = (&self.vault, &plan.bag)
            && container == source
        {
            let mut vault = vault.clone();
            vault.items = items.clone();
            plan.vault = Some(vault);
        }
        if slot.object != id {
            plan.loot_slot = Some((slot.object, slot.slot));
        }
        plan.dropped = Some(dropped);
        Ok(plan)
    }
    pub fn swap(&mut self, id: i32, from: Slot, to: Slot) -> Result<InventoryPlan> {
        self.cancel_trade(id, "Trade cancelled by inventory mutation.");
        let p = self.players.get(&id).context("player missing")?;
        ensure!(
            !p.pending && !p.death_pending && !p.departing && p.character.alive,
            "player busy, departing or dead"
        );
        ensure!(
            from.object == id || to.object == id,
            "swap must involve own inventory"
        );
        let mut items = p.character.items.clone();
        let bag_id = if from.object != id {
            Some(from.object)
        } else if to.object != id {
            Some(to.object)
        } else {
            None
        };
        let mut bag = if let Some(b) = bag_id {
            let e = self.entities.get(&b).context("container missing")?;
            ensure!(
                !e.reserved
                    && e.accessible(p.character.account)
                    && p.pos.distance(e.pos) < 1.5
                    && self.content.get(e.kind)?.class == "Container",
                "container inaccessible"
            );
            e.items.clone()
        } else {
            Vec::new()
        };
        let get = |s: &Slot| -> Result<i32> {
            let a = if s.object == id { &items } else { &bag };
            Ok(*a.get(s.slot).context("invalid inventory slot")?)
        };
        let a = get(&from)?;
        let b = get(&to)?;
        ensure!(a == from.kind && b == to.kind, "stale inventory offer");
        for (s, k) in [(&from, b), (&to, a)] {
            if s.object != id && k >= 0 {
                let item = self.content.get(u16::try_from(k)?)?;
                let container = &self.entities[&s.object];
                let definition = self.content.get(container.kind)?;
                ensure!(
                    if item.soulbound {
                        definition.put_soulbound
                            && container.extra_stats.contains(&(54, p.character.account))
                    } else {
                        definition.put_normal
                    },
                    "container rejects this normal or Soulbound item"
                );
            }
            if s.object == id && s.slot < 4 && k >= 0 {
                let obj = self.content.get(k as u16)?;
                ensure!(
                    !obj.unsupported_equipment,
                    "equipment effect not implemented"
                );
                ensure!(
                    obj.slot_type == self.content.get(p.character.class)?.slots[s.slot],
                    "incompatible equipment slot"
                );
            }
        }
        if from.object == id {
            items[from.slot] = b
        } else {
            bag[from.slot] = b
        }
        if to.object == id {
            items[to.slot] = a
        } else {
            bag[to.slot] = a
        }
        if let Some(b) = bag_id {
            self.entities.get_mut(&b).unwrap().reserved = true;
        }
        self.players.get_mut(&id).unwrap().pending = true;
        let mut plan = self.plan(id, items, bag_id.map(|b| (b, bag)), 0);
        if let Some(container) = bag_id {
            let source = if from.object == container { &from } else { &to };
            plan.loot_slot = Some((container, source.slot));
            if source.kind >= 0
                && self.entities[&container].drop_slots.contains(&source.slot)
                && self.players[&id].controller == Controller::Human
                && self.content.get(source.kind as u16)?.class == "Equipment"
            {
                plan.collected = Some(source.kind as u16);
            }
        }
        if let (Some((container, vault)), Some((bag, items))) = (&self.vault, &plan.bag)
            && container == bag
        {
            let mut updated = vault.clone();
            updated.items = items.clone();
            plan.vault = Some(updated);
        }
        Ok(plan)
    }
    pub fn poison(&mut self, id: i32, error: String) -> Result<()> {
        let p = self
            .players
            .get_mut(&id)
            .context("quarantine owner missing")?;
        tracing::error!(player=id,account=p.character.account,%error,"uncertain persistence; avatar and reservations quarantined");
        p.persistence_failure = Some(error);
        p.pending = true;
        p.bot_goal = None;
        p.bot_aim = None;
        if let Some(output) = &p.tx {
            let _ = output.lost.send(true);
        }
        Ok(())
    }
    pub fn finish_inventory(&mut self, plan: InventoryPlan, result: Result<i32>) -> Result<()> {
        if let Err(error) = result {
            self.poison(plan.player, error.to_string())?;
            return Err(error);
        }
        let p = self
            .players
            .get_mut(&plan.player)
            .context("player disappeared during save")?;
        p.pending = false;
        if let Some((id, _)) = &plan.bag {
            self.entities
                .get_mut(id)
                .context("reserved bag disappeared")?
                .reserved = false;
        }
        if let Some((bag, slot)) = plan.loot_slot {
            self.entities
                .get_mut(&bag)
                .context("reserved drop source disappeared")?
                .drop_slots
                .remove(&slot);
        }
        let mut o = Writer::new();
        match result {
            Ok(revision) => {
                p.character.items = plan.items;
                p.character.revision = revision;
                let values = p.character.effective(&self.content)?;
                p.character.hp = p.character.hp.min(values[0]);
                p.character.mp = p.character.mp.min(values[1]);
                if plan.healing > 0 && !p.death_pending {
                    p.character.hp = (p.character.hp + plan.healing).min(values[0]);
                }
                if plan.mana > 0 && !p.death_pending {
                    p.character.mp = (p.character.mp + plan.mana).min(values[1]);
                }
                if let Some((id, items)) = plan.bag {
                    self.entities.get_mut(&id).unwrap().items = items;
                }
                if let Some(dropped) = plan.dropped {
                    let e = self
                        .entities
                        .get_mut(&dropped)
                        .context("reserved ground drop missing")?;
                    e.reserved = false;
                    e.published = true;
                    self.bag_expiry.insert(dropped, self.now + 60_000);
                }
                if let Some(mut vault) = plan.vault {
                    vault.revision += 1;
                    self.vault.as_mut().context("committed chest missing")?.1 = vault;
                }
                o.i32(0);
                tracing::info!(player = plan.player, revision, "inventory committed");
            }
            Err(e) => {
                tracing::error!(player=plan.player,error=%e,"inventory transaction rejected");
                return Err(e);
            }
        }
        send(p, 4, o.0);
        Ok(())
    }
    pub fn action(&mut self, id: i32, a: Action) -> Result<Option<InventoryPlan>> {
        let p = self.players.get(&id).context("player missing")?;
        ensure!(
            p.controller == Controller::Qa,
            "script actions require a QA-owned player"
        );
        self.controlled_action(id, a)
    }
    pub fn autonomous_action(&mut self, id: i32, a: Action) -> Result<Option<InventoryPlan>> {
        ensure!(
            self.players.get(&id).context("player missing")?.controller == Controller::Autonomous,
            "autonomous actions require an autonomous controller"
        );
        self.controlled_action(id, a)
    }
    fn controlled_action(&mut self, id: i32, a: Action) -> Result<Option<InventoryPlan>> {
        let p = self.players.get(&id).context("player missing")?;
        ensure!(
            p.character.alive
                && !p.death_pending
                && !p.departing
                && p.persistence_failure.is_none(),
            "dead, departing or quarantined player cannot act"
        );
        match a {
            Action::Move { x, y } => {
                ensure!(self.walkable(Pos { x, y }), "goal outside map or blocked");
                self.players
                    .get_mut(&id)
                    .context("player missing")?
                    .bot_goal = Some(Pos { x, y });
                Ok(None)
            }
            Action::Shoot { angle } => {
                ensure!(angle.is_finite(), "invalid aim");
                self.players.get_mut(&id).context("player missing")?.bot_aim = Some(angle);
                Ok(None)
            }
            Action::Stop => {
                let p = self.players.get_mut(&id).context("player missing")?;
                p.bot_goal = None;
                p.bot_aim = None;
                Ok(None)
            }
            Action::Use { slot, x, y } => {
                let kind = *self
                    .players
                    .get(&id)
                    .context("player missing")?
                    .character
                    .items
                    .get(slot)
                    .context("invalid slot")?;
                self.use_item(
                    id,
                    Slot {
                        object: id,
                        slot,
                        kind,
                    },
                    Pos { x, y },
                )
            }
            Action::Swap { from, to } => self.swap(id, from, to).map(Some),
            Action::Drop { slot } => self.drop_item(id, slot).map(Some),
        }
    }
    pub fn correct_position(&mut self, id: i32, time: i32, reason: &str) -> Result<()> {
        let p = self.players.get_mut(&id).context("player missing")?;
        let previous_time = p.last_move;
        p.last_move = time;
        let mut out = Writer::new();
        out.i32(id).f32(p.pos.x).f32(p.pos.y);
        send(p, wire::GOTO, out.0);
        tracing::warn!(
            player = id,
            previous_time,
            client_time = time,
            reason,
            "movement rejected; sent authoritative GOTO"
        );
        Ok(())
    }
    pub fn pending_progress(&mut self) -> Result<Vec<(i32, Character)>> {
        let mut plans = Vec::new();
        for (id, p) in &mut self.players {
            if (p.experience_queued == 0 && p.caves_queued == 0) || p.pending || p.death_started {
                continue;
            }
            let mut snapshot = p.character.clone();
            snapshot.gain_experience(&self.content, p.experience_queued, &mut self.rng)?;
            snapshot.pirate_caves = snapshot
                .pirate_caves
                .checked_add(p.caves_queued)
                .context("dungeon clear counter overflow")?;
            snapshot.alive = true;
            p.experience_queued = 0;
            p.caves_queued = 0;
            p.pending = true;
            p.progress_started = true;
            plans.push((*id, snapshot));
        }
        Ok(plans)
    }
    pub fn finish_progress(&mut self, id: i32, snapshot: Character) -> Result<()> {
        let p = self
            .players
            .get_mut(&id)
            .context("progress owner missing")?;
        ensure!(
            p.pending
                && p.progress_started
                && p.character.id == snapshot.id
                && p.character.revision == snapshot.revision,
            "progress reservation changed"
        );
        let cleared_cave = snapshot.pirate_caves > p.character.pirate_caves;
        let leveled = snapshot.level > p.character.level;
        p.character.exp = snapshot.exp;
        p.character.fame = snapshot.fame;
        p.character.level = snapshot.level;
        p.character.base = snapshot.base;
        p.character.pirate_caves = snapshot.pirate_caves;
        if leveled && !p.death_pending {
            let values = p.character.effective(&self.content)?;
            p.character.hp = values[0];
            p.character.mp = values[1];
            p.hp_fraction = 0.;
            p.mp_fraction = 0.;
        }
        p.pending = false;
        p.progress_started = false;
        tracing::info!(
            player = id,
            level = p.character.level,
            experience = p.character.exp,
            "progress committed"
        );
        if cleared_cave && let Some(exit) = self.clear_exit.take() {
            self.entities
                .get_mut(&exit)
                .context("reserved clear exit missing")?
                .published = true;
            tracing::info!(entity = exit, "durable Cave clear published Nexus exit");
        }
        Ok(())
    }
    pub fn pending_deaths(&mut self) -> Vec<(i32, Character, String)> {
        let dying: Vec<_> = self
            .players
            .iter()
            .filter(|(_, p)| p.death_pending)
            .map(|(id, _)| *id)
            .collect();
        for id in dying {
            self.cancel_trade(id, "Trade cancelled by permanent death.");
        }
        self.players
            .iter_mut()
            .filter(|(_, p)| {
                p.death_pending
                    && !p.pending
                    && p.experience_queued == 0
                    && p.caves_queued == 0
                    && !p.death_started
            })
            .map(|(id, p)| {
                p.death_started = true;
                let mut c = p.character.clone();
                c.alive = false;
                (*id, c, p.killer.clone())
            })
            .collect()
    }
    pub fn finish_death(&mut self, id: i32) -> Result<()> {
        let p = self.players.get_mut(&id).context("dying player missing")?;
        ensure!(p.death_started && p.death_pending, "death not pending");
        p.character.alive = false;
        p.character.items.fill(-1);
        p.death_pending = false;
        let mut out = Writer::new();
        out.i32(p.character.account)
            .i32(p.character.id)
            .utf(&p.killer);
        send(p, 41, out.0);
        tracing::info!(player=id,character=p.character.id,killed_by=%p.killer,"permanent death committed");
        Ok(())
    }
    pub fn begin_departure(&mut self, id: i32) -> Result<Option<Character>> {
        self.cancel_trade(id, "Trade cancelled by departure.");
        let p = self
            .players
            .get_mut(&id)
            .context("departing player missing")?;
        ensure!(
            p.persistence_failure.is_none(),
            "quarantined persistence failure: {:?}",
            p.persistence_failure
        );
        p.departing = true;
        p.tx = None;
        p.bot_goal = None;
        p.bot_aim = None;
        if p.pending || p.death_pending || p.experience_queued > 0 || p.caves_queued > 0 {
            return Ok(None);
        };
        p.pending = true;
        Ok(Some(p.character.clone()))
    }
    pub fn finish_departure(&mut self, id: i32) -> Result<()> {
        self.players
            .get_mut(&id)
            .context("departing player missing")?
            .pending = false;
        self.leave(id)?;
        Ok(())
    }
    pub fn inspect_map(&self, player: Option<i32>) -> Result<serde_json::Value> {
        let account = match player {
            Some(id) => {
                self.players
                    .get(&id)
                    .context("map observer missing")?
                    .character
                    .account
            }
            None => -1,
        };
        let blocked: Vec<_> = (0..self.map.height)
            .flat_map(|y| (0..self.map.width).map(move |x| (x, y)))
            .filter(|(x, y)| {
                !self.walkable(Pos {
                    x: *x as f32 + 0.5,
                    y: *y as f32 + 0.5,
                })
            })
            .collect();
        let entities: Vec<_> = self
            .entities
            .values()
            .filter(|e| e.accessible(account))
            .collect();
        Ok(
            serde_json::json!({"width":self.map.width,"height":self.map.height,"blocked":blocked,"entities":entities}),
        )
    }
    pub fn observe(&self, id: i32) -> Result<serde_json::Value> {
        let p = self.players.get(&id).context("player missing")?;
        let entities: Vec<_> = self
            .entities
            .values()
            .filter(|e| p.pos.distance(e.pos) < 20. && e.accessible(p.character.account))
            .collect();
        let others: Vec<_> = self
            .players
            .iter()
            .filter(|(i, q)| **i != id && p.pos.distance(q.pos) < 20.)
            .map(|(i, q)| serde_json::json!({"id":i,"pos":q.pos,"name":q.character.name}))
            .collect();
        Ok(
            serde_json::json!({"tick":self.tick,"time_ms":self.now,"player":id,"pos":p.pos,"character":p.character,"entities":entities,"players":others,"bullets":self.bullets.len(),"own_projectiles":self.bullets.iter().filter(|b| b.friendly && b.owner == id).map(|b| serde_json::json!({"id":b.id,"start":b.start,"pos":b.pos,"angle":b.angle,"born":b.born})).collect::<Vec<_>>(),"pending":p.pending}),
        )
    }
    fn loot_bag(&mut self, pos: Pos, owner: Option<i32>, mut items: Vec<i32>) -> Result<i32> {
        ensure!(
            !items.is_empty() && items.len() <= 8,
            "loot bag requires one to eight items"
        );
        ensure!(
            self.bag_expiry.len() < 128,
            "transient loot bag limit reached"
        );
        let mut appearance = 0;
        for item in &items {
            let object = self.content.get(*item as u16)?;
            appearance = appearance.max(object.bag_type);
            ensure!(
                owner.is_some() || !object.soulbound,
                "public loot cannot contain soulbound equipment"
            );
        }
        let kind = match appearance {
            0 => 0x500,
            1 => 0x503,
            2 => 0x507,
            3 => 0x508,
            4 => 0x509,
            _ => bail!("unsupported loot bag appearance"),
        };
        items.resize(8, -1);
        let id = self.spawn_entity(kind, pos, items)?;
        let bag = self.entities.get_mut(&id).unwrap();
        bag.size = 80;
        if let Some(owner) = owner {
            bag.extra_stats.push((54, owner));
        }
        self.bag_expiry.insert(id, self.now + 60_000);
        tracing::info!(entity = id, owner, kind, "loot bag allocated");
        Ok(id)
    }
    fn drop_loot(&mut self, defeated: &Entity) -> Result<()> {
        if defeated.kind == 0x927 && self.name == "Pirate Cave" {
            ensure!(!self.cleared, "dungeon clear already recorded");
            self.cleared = true;
            for p in self.players.values_mut().filter(|p| {
                p.character.alive
                    && !p.death_pending
                    && !p.departing
                    && p.pos.distance(defeated.pos) < 25.
            }) {
                p.caves_queued += 1;
            }
            let exit = self.spawn_entity(0x71d, defeated.pos, vec![])?;
            self.entities.get_mut(&exit).unwrap().published = false;
            self.clear_exit = Some(exit);
            tracing::info!(
                boss = defeated.id,
                "Pirate Cave clear and Nexus exit staged for durable commit"
            );
        }
        let private = crate::loot::private(defeated.kind);
        // A flat 100-damage officer threshold exceeds the selected officers' entire HP.
        let threshold = if defeated.kind == 0x927 {
            100
        } else {
            (self.content.objects[&defeated.kind].hp / 10).max(1)
        };
        let mut eligible: Vec<_> = self
            .players
            .values()
            .filter(|p| p.character.alive && !p.death_pending && !p.departing)
            .filter_map(|p| {
                defeated
                    .damage
                    .get(&p.character.id)
                    .filter(|damage| **damage > if private { threshold } else { 0 })
                    .map(|damage| (p.character.account, *damage))
            })
            .collect();
        eligible.sort_by_key(|(_, damage)| std::cmp::Reverse(*damage));
        if eligible.is_empty() {
            return Ok(());
        }
        let mut drops = BTreeMap::<i32, Vec<i32>>::new();
        let attempts = (eligible.len() * if private { 1 } else { 2 }).min(8);
        for i in 0..attempts {
            if let Some(item) = crate::loot::roll(&self.content, defeated.kind, &mut self.rng) {
                let owner = if private {
                    eligible[i % eligible.len()].0
                } else {
                    -1
                };
                let items = drops.entry(owner).or_default();
                if !items.contains(&(item as i32)) || matches!(item, 0xa22 | 0xa23) {
                    items.push(item as i32);
                }
            }
        }
        for (owner, items) in drops {
            let slots = (0..items.len()).collect();
            let bag = self.loot_bag(defeated.pos, (owner >= 0).then_some(owner), items)?;
            self.entities.get_mut(&bag).unwrap().drop_slots = slots;
        }
        Ok(())
    }
    fn populate_shore(&mut self) -> Result<()> {
        let &(x, y) = self.map.spawns.first().context("shore has no spawn")?;
        let spawn = Pos { x, y };
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
                (matches!(c.terrain, 11 | 12) && self.walkable(pos) && pos.distance(spawn) >= 12.)
                    .then_some(pos)
            })
            .collect();
        candidates.shuffle(&mut self.rng);
        // Keep the bounded starter population playable on a Realm-sized coastline.
        candidates.sort_by(|a, b| a.distance(spawn).total_cmp(&b.distance(spawn)));
        for pos in candidates {
            if self.population.iter().any(|s| s.pos.distance(pos) < 6.) {
                continue;
            }
            let kind = [0x600, 0x60e, 0x60f, 0x610][self.population.len() % 4];
            let entity = self.spawn_entity(kind, pos, vec![])?;
            self.population.push(Spawn {
                kind,
                pos,
                entity: Some(entity),
                ready: 0,
            });
            if self.population.len() == 24 {
                break;
            }
        }
        ensure!(
            self.population.len() == 24,
            "sourced shore cannot support its bounded population"
        );
        Ok(())
    }
    fn repopulate(&mut self) -> Result<()> {
        for i in 0..self.population.len() {
            let s = &self.population[i];
            if s.entity.is_some_and(|id| !self.entities.contains_key(&id)) {
                self.population[i].entity = None;
                self.population[i].ready = self.now + 60_000;
            }
            let s = &self.population[i];
            if s.entity.is_none()
                && self.now >= s.ready
                && self.players.values().all(|p| p.pos.distance(s.pos) >= 12.)
            {
                let id = self.spawn_entity(s.kind, s.pos, vec![])?;
                self.population[i].entity = Some(id);
                tracing::debug!(
                    entity = id,
                    kind = self.population[i].kind,
                    "shore enemy respawned"
                );
            }
        }
        Ok(())
    }
    fn move_entity(&mut self, id: i32, goal: Pos, distance: f32) -> Result<()> {
        let old = self.entities[&id].pos;
        let length = old.distance(goal);
        if length <= 0.001 {
            return Ok(());
        }
        let distance = distance.min(length);
        let pos = Pos {
            x: old.x + (goal.x - old.x) / length * distance,
            y: old.y + (goal.y - old.y) / length * distance,
        };
        let samples = (distance / 0.1).ceil().max(1.) as usize;
        ensure!(
            (1..=samples).all(|i| self.walkable(Pos {
                x: old.x + (pos.x - old.x) * i as f32 / samples as f32,
                y: old.y + (pos.y - old.y) * i as f32 / samples as f32
            })),
            "NPC movement crosses a blocked tile"
        );
        let cell = |p: Pos| p.y as usize * self.map.width as usize + p.x as usize;
        assert!(
            self.cells[cell(old)].remove(&id),
            "moving NPC missing from spatial index"
        );
        self.cells[cell(pos)].insert(id);
        self.entities.get_mut(&id).unwrap().pos = pos;
        Ok(())
    }
    fn npc_step(&mut self) -> Vec<(i32, Vec<u8>)> {
        use crate::encounter::{Motion, attacks, profile};
        let mut packets = Vec::new();
        if !self.encounter || self.safe {
            return packets;
        }
        let ids: Vec<_> = self
            .entities
            .values()
            .filter(|e| self.content.objects[&e.kind].enemy)
            .map(|e| e.id)
            .collect();
        for id in ids {
            let entity = &self.entities[&id];
            let kind = entity.kind;
            let pos = entity.pos;
            let profile = profile(kind).expect("validated encounter profile");
            let nearest = self
                .players
                .values()
                .filter(|p| p.character.alive && !p.death_pending && !p.departing && !p.paused)
                .min_by(|a, b| pos.distance(a.pos).total_cmp(&pos.distance(b.pos)))
                .map(|p| p.pos);
            let Some(target) = nearest.filter(|p| pos.distance(*p) < 20.) else {
                continue;
            };
            let mut speed = profile.speed;
            let goal = match profile.motion {
                Motion::Chase { sight, stop } if pos.distance(target) < sight => {
                    if pos.distance(target) <= stop {
                        speed = 0.;
                    }
                    Some(target)
                }
                Motion::Circle => {
                    let boss = self
                        .entities
                        .values()
                        .find(|e| e.kind == 0x927 && pos.distance(e.pos) < 10.)
                        .map(|e| e.pos);
                    boss.map(|boss| {
                        let e = self.entities.get_mut(&id).unwrap();
                        if e.goal.is_none() {
                            e.phase = self.rng.gen_range(0. ..std::f32::consts::TAU);
                        }
                        e.phase = (e.phase + profile.speed / 3. * 0.05) % std::f32::consts::TAU;
                        Pos {
                            x: boss.x + e.phase.cos() * 3.,
                            y: boss.y + e.phase.sin() * 3.,
                        }
                    })
                }
                _ => None,
            };
            let goal = if let Some(goal) = goal {
                goal
            } else {
                let e = self.entities.get_mut(&id).unwrap();
                speed = match profile.motion {
                    Motion::Wander => profile.speed,
                    Motion::Chase { .. } if kind == 0x600 => 4.,
                    _ => 2.,
                };
                if e.goal
                    .is_none_or(|goal| goal.distance(e.pos) < 0.1 || goal.distance(e.home) > 2.01)
                {
                    let angle = self.rng.gen_range(0. ..std::f32::consts::TAU);
                    let radius = self.rng.gen_range(0. ..2.);
                    e.goal = Some(Pos {
                        x: e.home.x + angle.cos() * radius,
                        y: e.home.y + angle.sin() * radius,
                    });
                }
                e.goal.expect("wander destination assigned")
            };
            self.entities.get_mut(&id).unwrap().goal = Some(goal);
            if let Err(error) = self.move_entity(id, goal, speed / 1.5 * 0.05) {
                self.entities.get_mut(&id).unwrap().goal = None;
                tracing::debug!(entity=id,%error,"NPC movement blocked; retargeting");
            }
            let e = &self.entities[&id];
            if profile.interval == 0
                || self.now < e.next_shot
                || e.pos.distance(target) > profile.range
            {
                continue;
            }
            let heading = (target.y - e.pos.y).atan2(target.x - e.pos.x);
            let shots = attacks(kind, e.hp, heading);
            self.entities.get_mut(&id).unwrap().next_shot = self.now + profile.interval;
            for (index, angle) in shots {
                let spec = self.content.objects[&kind]
                    .projectiles
                    .get(&index)
                    .expect("selected encounter projectile missing")
                    .clone();
                let e = self.entities.get_mut(&id).unwrap();
                e.bullet = e.bullet.wrapping_add(1);
                let bullet = Bullet {
                    owner: id,
                    source_kind: kind,
                    id: e.bullet,
                    friendly: false,
                    start: e.pos,
                    pos: e.pos,
                    angle,
                    born: self.now,
                    damage: spec.min,
                    spec,
                    hit: BTreeSet::new(),
                };
                let mut out = Writer::new();
                out.u8(bullet.id)
                    .i32(id)
                    .u8(index)
                    .f32(bullet.start.x)
                    .f32(bullet.start.y)
                    .f32(angle)
                    .i16(bullet.damage as i16);
                packets.push((id, out.0));
                if kind == 0x927 {
                    tracing::info!(
                        boss = id,
                        projectile = index,
                        time_ms = self.now,
                        "Dreadstump projectile emitted"
                    );
                }
                self.bullets.push(bullet);
            }
        }
        packets
    }
    pub fn step(&mut self) {
        self.now += 50;
        self.tick += 1;
        self.trade_step();
        let ids: Vec<_> = self.players.keys().copied().collect();
        for id in &ids {
            let p = &self.players[id];
            if !p.character.alive || p.death_pending || p.departing || p.paused {
                continue;
            }
            let p = self.players.get_mut(id).unwrap();
            let values = p
                .character
                .effective(&self.content)
                .expect("validated player equipment and base stats");
            if p.character.hp < values[0] {
                p.hp_fraction += (1. + values[6] as f32 / 8.) * 0.05;
                let n = p.hp_fraction as i32;
                p.character.hp = (p.character.hp + n).min(values[0]);
                p.hp_fraction -= n as f32;
            } else {
                p.hp_fraction = 0.;
            }
            if p.character.mp < values[1] {
                p.mp_fraction += (0.6 + values[7] as f32 / 125.) * 0.05;
                let n = p.mp_fraction as i32;
                p.character.mp = (p.character.mp + n).min(values[1]);
                p.mp_fraction -= n as f32;
            } else {
                p.mp_fraction = 0.;
            }
            let goal = p.bot_goal;
            let aim = p.bot_aim;
            if let Some(goal) = goal {
                let d = p.pos.distance(goal);
                if d > 0.02 {
                    let step = (0.004 + (0.0096 - 0.004) * values[4] as f32 / 75.) * 50.;
                    let n = d.min(step);
                    let pos = Pos {
                        x: p.pos.x + (goal.x - p.pos.x) / d * n,
                        y: p.pos.y + (goal.y - p.pos.y) / d * n,
                    };
                    let time = self.now as i32;
                    if let Err(e) = self.move_to(*id, pos, time) {
                        tracing::debug!(player=id,error=%e,"bot movement blocked");
                    }
                }
            }
            if self.players[id].controller != Controller::Human {
                let pos = self.players[id].pos;
                let time = self.now as i32;
                if let Err(e) = self.move_to(*id, pos, time) {
                    tracing::error!(player=id,error=%e,"script idle MOVE rejected");
                }
            }
            if let Some(angle) = aim {
                let p = &self.players[id];
                let time = self.now as i32;
                let kind = p.character.items[0] as u16;
                let shots = self.content.objects[&kind].shots;
                let first = p.next_bullet;
                let pos = p.pos;
                let start = Pos {
                    x: pos.x + 0.3 * angle.cos(),
                    y: pos.y + 0.3 * angle.sin(),
                };
                for i in 0..shots {
                    if self
                        .shoot(*id, time, (first + i as u8) % 128, kind, start, angle)
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
        if let Err(error) = self.repopulate() {
            tracing::error!(%error,"bounded shore respawn failed");
        }
        let expired: Vec<_> = self
            .bag_expiry
            .iter()
            .filter(|(id, deadline)| {
                let bag = &self.entities[id];
                !bag.reserved
                    && (**deadline <= self.now || bag.items.iter().all(|item| *item == -1))
            })
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            self.remove_entity(id);
            tracing::debug!(entity = id, "empty or expired transient loot bag removed");
        }
        let shots = self.npc_step();
        for id in &ids {
            if let Err(e) = self.updates(*id, false) {
                tracing::error!(player=id,error=%e,"world update failed");
            }
        }
        for (owner, shot) in shots {
            for p in self.players.values().filter(|p| p.known.contains(&owner)) {
                send(p, 19, shot.clone());
            }
        }
        let guns: Vec<_> = self
            .entities
            .values()
            .filter(|e| (0x0802..=0x0805).contains(&e.kind) && self.now >= e.next_shot)
            .map(|e| e.id)
            .collect();
        for id in guns {
            let e = self.entities.get_mut(&id).unwrap();
            e.next_shot = self.now + self.rng.gen_range(2000..3500);
            e.bullet = e.bullet.wrapping_add(1);
            let angle = match e.kind {
                0x0802 => std::f32::consts::FRAC_PI_2,
                0x0804 => 0.,
                0x0805 => std::f32::consts::PI,
                _ => 0.,
            };
            let spec = self.content.objects[&e.kind]
                .projectiles
                .get(&0)
                .cloned()
                .expect("validated tutorial gun projectile zero");
            let b = Bullet {
                owner: id,
                source_kind: e.kind,
                id: e.bullet,
                friendly: false,
                start: e.pos,
                pos: e.pos,
                angle,
                born: self.now,
                damage: spec.min,
                spec,
                hit: BTreeSet::new(),
            };
            let mut o = Writer::new();
            o.u8(b.id)
                .i32(id)
                .u8(0)
                .f32(b.start.x)
                .f32(b.start.y)
                .f32(angle)
                .i16(b.damage as i16);
            for p in self.players.values() {
                if p.known.contains(&id) {
                    send(p, 19, o.0.clone());
                }
            }
            self.bullets.push(b);
        }
        let mut bullets = std::mem::take(&mut self.bullets);
        for b in &mut bullets {
            for sample in 1..=5 {
                if !b.hit.is_empty() {
                    break;
                }
                let old = b.pos;
                b.pos = b.position(
                    (self.now - 50 + sample * 10)
                        .max(b.born)
                        .min(b.born + b.spec.lifetime as u64),
                );
                if b.friendly
                    && self
                        .map
                        .cell(b.pos.x, b.pos.y)
                        .is_none_or(|c| self.content.grounds[&c.ground] <= 0.)
                    || self.entities_at(b.pos).any(|e| {
                        let o = &self.content.objects[&e.kind];
                        o.static_object
                            && (!o.enemy || !b.friendly)
                            && (o.enemy_occupy_square || o.occupy_square && !b.spec.passes_cover)
                    })
                {
                    b.hit.insert(-1);
                    break;
                }
                if b.friendly {
                    let targets: Vec<_> = self
                        .near_entities(old, b.pos)
                        .filter(|e| {
                            e.hp > 0
                                && self.content.objects[&e.kind].destructible
                                && !b.hit.contains(&e.id)
                                && inside_box(e.pos, b.pos)
                        })
                        .map(|e| e.id)
                        .collect();
                    if let Some(target) = targets.iter().min_by(|a, b_id| {
                        self.entities[a]
                            .pos
                            .distance(b.pos)
                            .total_cmp(&self.entities[b_id].pos.distance(b.pos))
                    }) {
                        if self.players.get(&b.owner).is_some_and(|p| p.paused) {
                            b.hit.insert(*target);
                            break;
                        }
                        let e = self.entities.get_mut(target).unwrap();
                        let damage = (b.damage - self.content.objects[&e.kind].defense)
                            .max(b.damage * 3 / 20);
                        if let Some(p) = self.players.get(&b.owner) {
                            *e.damage.entry(p.character.id).or_default() += damage.min(e.hp).max(0);
                        }
                        e.hp -= damage;
                        let kill = e.hp <= 0;
                        let quest = e.kind == 0x6b1 || self.content.objects[&e.kind].quest;
                        if let Some(p) = self.players.get_mut(&b.owner) {
                            if damage > 0 {
                                p.character.hits += 1;
                            }
                            if kill {
                                p.character.kills += 1;
                                if quest {
                                    p.character.quests += 1;
                                }
                            }
                        }
                        let mut o = Writer::new();
                        o.i32(*target)
                            .u8(0)
                            .i16(damage as i16)
                            .u8(u8::from(kill))
                            .u8(b.id)
                            .i32(b.owner);
                        for p in self.players.values() {
                            if p.known.contains(target) {
                                send(p, 47, o.0.clone());
                            }
                        }
                        b.hit.insert(*target);
                        if kill {
                            let e = self.remove_entity(*target);
                            let definition = &self.content.objects[&e.kind];
                            let reward = (definition.hp as f32 * definition.xp_multiplier / 10.)
                                .ceil() as i32;
                            for (id, p) in &mut self.players {
                                if p.character.alive
                                    && !p.death_pending
                                    && !p.departing
                                    && p.pos.distance(e.pos) < 25.
                                {
                                    let cap = (crate::store::experience_goal(p.character.level)
                                        .expect("validated player level")
                                        as f32
                                        * if definition.quest { 0.5 } else { 0.1 })
                                        as i32;
                                    let amount = reward.min(cap);
                                    p.experience_queued = p
                                        .experience_queued
                                        .checked_add(amount)
                                        .expect("experience queue overflow");
                                    if amount > 0 {
                                        tracing::debug!(
                                            player = id,
                                            enemy = e.kind,
                                            amount,
                                            "experience queued"
                                        );
                                    }
                                }
                            }
                            if self.name == "Realm"
                                && matches!(e.kind, 0x60e..=0x610)
                                && self.rng.gen_bool(0.01)
                            {
                                self.portal_requests.push(e.pos);
                                tracing::info!(
                                    enemy = e.kind,
                                    entity = target,
                                    "Cube defeat earned a dungeon portal"
                                );
                            }
                            if self.encounter
                                && let Err(error) = self.drop_loot(&e)
                            {
                                tracing::error!(enemy=e.kind,%error,"earned encounter loot creation failed");
                            }
                            tracing::info!(entity = target, owner = b.owner, "enemy defeated");
                        }
                    }
                } else if !self.safe {
                    for id in &ids {
                        let p = &self.players[id];
                        if p.character.alive
                            && !p.paused
                            && !p.departing
                            && !p.death_pending
                            && !b.hit.contains(id)
                            && inside_box(p.pos, b.pos)
                        {
                            let damage = (b.damage
                                - self.defense(*id).expect("valid player equipment"))
                            .max(b.damage * 3 / 20);
                            let p = self.players.get_mut(id).unwrap();
                            p.character.hp = (p.character.hp - damage).max(0);
                            if p.character.hp == 0 {
                                p.death_pending = true;
                                p.killer = self.content.objects[&b.source_kind].name.clone();
                                p.bot_goal = None;
                                p.bot_aim = None;
                            }
                            let mut o = Writer::new();
                            o.i32(*id)
                                .u8(0)
                                .i16(damage as i16)
                                .u8(0)
                                .u8(b.id)
                                .i32(b.owner);
                            send(p, 47, o.0);
                            b.hit.insert(*id);
                            tracing::debug!(
                                player = id,
                                damage,
                                hp = p.character.hp,
                                "player damaged"
                            );
                        }
                    }
                }
            }
        }
        bullets.retain(|b| self.now - b.born < b.spec.lifetime as u64 && b.hit.is_empty());
        self.bullets = bullets;
        if self.tick.is_multiple_of(2) {
            for id in ids {
                if let Err(e) = self.updates(id, false) {
                    tracing::error!(player=id,error=%e,"visibility update failed");
                    continue;
                }
                let p = &self.players[&id];
                let dynamic: Vec<_> = p
                    .known
                    .iter()
                    .filter(|entity| {
                        self.players.contains_key(entity)
                            || self
                                .entities
                                .get(entity)
                                .is_some_and(|e| e.hp > 0 || !e.items.is_empty())
                    })
                    .copied()
                    .collect();
                let mut o = Writer::new();
                o.i32(p.tick).i32(100).i16(dynamic.len() as i16);
                for entity in dynamic {
                    o.0.extend(self.stats(entity).expect("visible entity must exist"));
                }
                send(p, wire::NEW_TICK, o.0);
                self.players.get_mut(&id).unwrap().tick += 1;
            }
        }
    }
}
fn inside_box(a: Pos, b: Pos) -> bool {
    (a.x - b.x).abs() <= 0.5 && (a.y - b.y).abs() <= 0.5
}
#[cfg(test)]
#[path = "../examples/testing/cover.rs"]
mod cover_example;
#[cfg(test)]
#[path = "../examples/testing/timing.rs"]
mod timing_example;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::WIZARD;
    use std::path::Path;
    pub(super) fn world() -> World {
        let c = Arc::new(Content::load(Path::new("data")).unwrap());
        let m = Arc::new(Map::load(Path::new("data/tutorial.wmap"), &c).unwrap());
        World::new(c, m).unwrap()
    }
    pub(super) fn ch(w: &World) -> Character {
        Character {
            schema: 1,
            class: WIZARD,
            base: w.content.classes[&WIZARD].base,
            id: 1,
            account: 1,
            name: "Test".into(),
            level: 1,
            exp: 0,
            fame: 0,
            hp: 100,
            mp: 100,
            items: w.content.objects[&WIZARD].equipment.clone(),
            revision: 0,
            alive: true,
            created_at: 1,
            shots: 0,
            hits: 0,
            kills: 0,
            quests: 0,
            tiles: 0,
            pirate_caves: 0,
        }
    }
    pub(super) fn receive(
        d: &mut Directory,
        world: i32,
        account: i32,
        class: u16,
    ) -> (i32, mpsc::Receiver<Packet>) {
        let w = d.worlds.get_mut(&world).unwrap();
        let mut c = super::tests::ch(w);
        c.id = account + 100;
        c.account = account;
        c.class = class;
        c.base = w.content.classes[&class].base;
        c.items = w.content.objects[&class].equipment.clone();
        let (packets, mut rx) = mpsc::channel(128);
        let (lost, _) = watch::channel(false);
        let id = w.enter(c, Some(Output { packets, lost })).unwrap();
        while rx.try_recv().is_ok() {}
        (id, rx)
    }
    pub(super) fn directory() -> Directory {
        Directory::load(
            Path::new("data"),
            Arc::new(Content::load(Path::new("data")).unwrap()),
            16,
        )
        .unwrap()
    }
    #[test]
    fn directory_routes_and_global_identities_do_not_alias() {
        let mut d = directory();
        assert_eq!(d.initial(0).unwrap(), -2);
        for game in [-1, -2, -3, -4] {
            assert_eq!(d.initial(game).unwrap(), game);
        }
        assert!(d.initial(-5).is_err());
        assert!(d.initial(-6).is_err());
        let mut ids = BTreeSet::new();
        for world in d.worlds.values() {
            for id in world.entities.keys() {
                assert!(ids.insert(*id));
            }
        }
        let a = d.worlds.get_mut(&-1).unwrap();
        let first = a.enter(ch(a), None).unwrap();
        let b = d.worlds.get_mut(&-2).unwrap();
        let second = b.enter(ch(b), None).unwrap();
        assert_ne!(first, second);
        assert_eq!(d.location(first).unwrap(), -1);
        assert_eq!(d.location(second).unwrap(), -2);
        assert!(d.worlds[&-2].safe && !d.worlds[&-3].safe);
        assert!(d.worlds[&-2].entities.values().any(|e| e.kind == 0x704));
    }
    #[test]
    fn transfer_grants_are_bound_one_use_expiring_and_private() {
        let mut d = directory();
        let vault = d.instance("vault", Some(1)).unwrap();
        assert_eq!(d.instance("vault", Some(1)).unwrap(), vault);
        assert_ne!(d.instance("vault", Some(2)).unwrap(), vault);
        assert!(d.issue(2, 1, vault).is_err());
        let (time, key, _) = d.issue(1, 42, vault).unwrap();
        assert!(d.consume(2, vault, time, key.clone()).is_err());
        assert!(d.consume(1, -2, time, key.clone()).is_err());
        assert!(d.consume(1, vault, time + 1, key.clone()).is_err());
        assert_eq!(d.consume(1, vault, time, key.clone()).unwrap(), 42);
        assert!(d.consume(1, vault, time, key).is_err());
        let (time, key, _) = d.issue(1, 42, -2).unwrap();
        d.tick += 2400;
        assert!(d.consume(1, -2, time, key).is_err());
        let ids = AtomicI32::new(i32::MAX);
        assert!(allocate_id(&ids).is_err());
        assert_eq!(ids.load(Ordering::Relaxed), i32::MAX);
    }
    #[test]
    fn directory_instances_are_bounded_and_idle_instances_are_collected() {
        let mut d = directory();
        for _ in 0..8 {
            d.instance("pirate-cave", None).unwrap();
        }
        assert!(d.instance("pirate-cave", None).is_err());
        d.tick = 6001;
        d.step();
        assert_eq!(d.worlds.len(), 4);
    }
    #[test]
    fn sourced_worlds_preserve_connected_wall_and_merchant_config_stats() {
        let content = Arc::new(Content::load(Path::new("data")).unwrap());
        for name in [
            "nexus",
            "nexus-limbo",
            "vault",
            "realm-shore",
            "pirate-cave",
        ] {
            let map = Arc::new(
                Map::load(&Path::new("data").join(format!("{name}.wmap")), &content).unwrap(),
            );
            let world = World::new(content.clone(), map).unwrap();
            for e in world.entities.values() {
                world.stats(e.id).unwrap();
            }
            if name == "pirate-cave" {
                assert!(
                    world
                        .entities
                        .values()
                        .any(|e| e.extra_stats.iter().any(|(stat, _)| *stat == 41))
                );
            }
            if name == "nexus" {
                assert!(
                    world
                        .entities
                        .values()
                        .any(|e| e.extra_stats.iter().any(|(stat, _)| *stat == 34))
                );
            }
        }
    }
    #[test]
    fn progress_is_invisible_until_commit_and_cannot_heal_pending_death() {
        let mut w = world();
        let id = w.enter(ch(&w), None).unwrap();
        w.players.get_mut(&id).unwrap().experience_queued = 50;
        let plans = w.pending_progress().unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].1.level, 2);
        assert_eq!(w.players[&id].character.level, 1);
        assert!(w.pending_progress().unwrap().is_empty());
        let p = w.players.get_mut(&id).unwrap();
        p.character.hp = 0;
        p.death_pending = true;
        assert!(w.pending_deaths().is_empty());
        w.finish_progress(id, plans[0].1.clone()).unwrap();
        assert_eq!(w.players[&id].character.level, 2);
        assert_eq!(w.players[&id].character.hp, 0);
        let death = w.pending_deaths();
        assert_eq!(death.len(), 1);
        assert_eq!(death[0].1.level, 2);
        assert_eq!(death[0].1.hp, 0);
    }
    #[test]
    fn committed_fame_survives_departure_and_permanent_death() {
        let mut w = world();
        let id = w.enter(ch(&w), None).unwrap();
        w.players.get_mut(&id).unwrap().experience_queued = 1000;
        let plans = w.pending_progress().unwrap();
        assert_eq!(plans[0].1.fame, 1);
        assert_eq!(w.players[&id].character.fame, 0);
        w.finish_progress(id, plans[0].1.clone()).unwrap();
        let saved = w.begin_departure(id).unwrap().unwrap();
        assert_eq!(saved.fame, 1);
        w.finish_departure(id).unwrap();
        let id = w.enter(saved, None).unwrap();
        let p = w.players.get_mut(&id).unwrap();
        p.character.hp = 0;
        p.death_pending = true;
        assert_eq!(w.pending_deaths()[0].1.fame, 1);
    }
    #[test]
    fn departure_waits_for_earned_progress() {
        let mut w = world();
        let id = w.enter(ch(&w), None).unwrap();
        w.players.get_mut(&id).unwrap().experience_queued = 50;
        assert!(w.begin_departure(id).unwrap().is_none());
        let plans = w.pending_progress().unwrap();
        w.finish_progress(id, plans[0].1.clone()).unwrap();
        let leaving = w.begin_departure(id).unwrap().unwrap();
        assert_eq!(leaving.exp, 50);
        assert_eq!(leaving.level, 2);
        w.finish_departure(id).unwrap();
        assert!(!w.players.contains_key(&id));
    }
    #[test]
    fn health_excludes_qa_actors_from_population() {
        let mut d = directory();
        let w = d.worlds.get_mut(&-1).unwrap();
        w.enter(ch(w), None).unwrap();
        let mut native = ch(w);
        native.id = 2;
        native.account = 2;
        let (packets, _rx) = mpsc::channel(64);
        let (lost, _watch) = watch::channel(false);
        w.enter(native, Some(Output { packets, lost })).unwrap();
        let health = d.health(Instant::now());
        assert_eq!(health["players"], 1);
        assert_eq!(health["bots"], 0);
        assert_eq!(health["qa_actors"], 1);
        assert_eq!(health["worlds"], d.worlds.len());
        assert_eq!(
            health["entities"],
            d.worlds.values().map(|w| w.entities.len()).sum::<usize>()
        );
        assert_eq!(
            health["projectiles"],
            d.worlds.values().map(|w| w.bullets.len()).sum::<usize>()
        );
        assert_eq!(health["tick"], d.tick);
        assert_eq!(health["max_tick_us"], d.max_tick_us);
    }
    #[test]
    fn health_uptime_uses_elapsed_process_time_not_world_ticks() {
        let w = directory();
        let started = Instant::now();
        let before = w.health(started)["uptime_seconds"].as_u64().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let after = w.health(started)["uptime_seconds"].as_u64().unwrap();
        assert!(after > before);
        assert_eq!(w.tick, 0);
    }
    #[test]
    fn one_owner_and_no_teleport() {
        let mut w = world();
        let c = ch(&w);
        let id = w.enter(c.clone(), None).unwrap();
        assert!(w.enter(c, None).is_err());
        assert!(w.move_to(id, Pos { x: 12., y: 3. }, 100).is_err());
    }
    #[test]
    fn queue_loss_is_observable() {
        let mut w = world();
        let id = w.enter(ch(&w), None).unwrap();
        let (packets, mut rx) = mpsc::channel(1);
        let (lost, watch) = watch::channel(false);
        let p = w.players.get_mut(&id).unwrap();
        p.tx = Some(Output { packets, lost });
        send(p, 6, vec![1]);
        assert!(!*watch.borrow());
        send(p, 6, vec![2]);
        assert!(*watch.borrow());
        assert_eq!(rx.try_recv().unwrap().body, vec![1]);
    }
    #[test]
    fn control_cannot_steal_browser_input() {
        let mut w = world();
        let (packets, _rx) = mpsc::channel(64);
        let (lost, _watch) = watch::channel(false);
        let id = w.enter(ch(&w), Some(Output { packets, lost })).unwrap();
        assert!(
            w.action(id, Action::Stop)
                .err()
                .unwrap()
                .to_string()
                .contains("QA-owned")
        );
    }
    #[test]
    fn selected_collision_box_is_inclusive() {
        let a = Pos { x: 0., y: 0. };
        assert!(inside_box(a, Pos { x: 0.5, y: 0.5 }));
        assert!(inside_box(a, Pos { x: -0.5, y: -0.5 }));
        assert!(!inside_box(a, Pos { x: 0.500001, y: 0. }));
    }
    #[test]
    fn initial_map_is_fogged() {
        let mut w = world();
        let id = w.enter(ch(&w), None).unwrap();
        assert!(w.players[&id].tiles.len() < w.map.cells.len());
        assert!(w.players[&id].known.contains(&id));
    }
    #[test]
    fn initial_update_places_self_before_private_container_owner_stats() {
        let mut w = world();
        let pos = Pos {
            x: w.map.spawns[0].0,
            y: w.map.spawns[0].1,
        };
        let bag = w.loot_bag(pos, Some(1), vec![0xa4f]).unwrap();
        let (tx, mut rx) = mpsc::channel(64);
        let (lost, _lost) = watch::channel(false);
        let id = w.enter(ch(&w), Some(Output { packets: tx, lost })).unwrap();
        assert!(bag < id);
        assert_eq!(rx.try_recv().unwrap().id, wire::CREATE_SUCCESS);
        let update = rx.try_recv().unwrap();
        assert_eq!(update.id, wire::UPDATE);
        let mut r = crate::wire::Reader::new(&update.body);
        let tiles = r.i16().unwrap();
        r.take(tiles as usize * 5).unwrap();
        assert!(r.i16().unwrap() >= 2);
        assert_eq!(r.i16().unwrap() as u16, WIZARD);
        assert_eq!(r.i32().unwrap(), id);
        assert!(w.visible(id).contains(&bag));
    }
    #[test]
    fn immutable_scenery_is_not_rebroadcast_in_dynamic_ticks() {
        let mut w = world();
        let (tx, mut rx) = mpsc::channel(64);
        let (lost, _lost) = watch::channel(false);
        let id = w.enter(ch(&w), Some(Output { packets: tx, lost })).unwrap();
        while rx.try_recv().is_ok() {}
        assert!(w.players[&id].known.iter().any(|entity| {
            w.entities
                .get(entity)
                .is_some_and(|e| e.hp == 0 && e.items.is_empty())
        }));
        w.step();
        w.step();
        let dynamic: Vec<_> = w.players[&id]
            .known
            .iter()
            .filter(|entity| {
                w.players.contains_key(entity)
                    || w.entities
                        .get(entity)
                        .is_some_and(|e| e.hp > 0 || !e.items.is_empty())
            })
            .copied()
            .collect();
        let mut expected = Writer::new();
        expected.i32(0).i32(100).i16(dynamic.len() as i16);
        for entity in dynamic {
            expected.0.extend(w.stats(entity).unwrap());
        }
        let mut tick = None;
        while let Ok(packet) = rx.try_recv() {
            if packet.id == wire::NEW_TICK {
                assert!(tick.replace(packet).is_none());
            }
        }
        assert_eq!(tick.unwrap().body, expected.0);
    }
    #[test]
    fn movement_cannot_borrow_future_clock() {
        let mut w = world();
        let id = w.enter(ch(&w), None).unwrap();
        let p = w.players[&id].pos;
        w.move_to(
            id,
            Pos {
                x: p.x + 0.4,
                y: p.y,
            },
            100,
        )
        .unwrap();
        assert!(
            w.move_to(
                id,
                Pos {
                    x: p.x + 0.8,
                    y: p.y
                },
                200
            )
            .is_err()
        );
    }
    #[test]
    fn shots_cannot_borrow_future_clock() {
        let mut w = world();
        let id = w.enter(ch(&w), None).unwrap();
        let p = w.players[&id].pos;
        let start = Pos {
            x: p.x + 0.3,
            y: p.y,
        };
        for (time, bullet) in [(100, 1), (100, 2), (500, 3), (500, 4)] {
            w.shoot(id, time, bullet, 0xa97, start, 0.).unwrap();
        }
        assert!(w.shoot(id, 900, 5, 0xa97, start, 0.).is_err());
        assert_eq!(w.bullets.len(), 4);
    }
    #[test]
    fn magic_potion_publishes_after_commit_and_cannot_restore_pending_death() {
        let mut w = world();
        let mut c = ch(&w);
        c.items[4] = 0xa23;
        c.mp = 25;
        let id = w.enter(c, None).unwrap();
        let slot = Slot {
            object: id,
            slot: 4,
            kind: 0xa23,
        };
        let plan = w.use_item(id, slot, w.players[&id].pos).unwrap().unwrap();
        assert_eq!(w.players[&id].character.mp, 25);
        assert_eq!(w.players[&id].character.items[4], 0xa23);
        w.finish_inventory(plan, Ok(1)).unwrap();
        assert_eq!(w.players[&id].character.mp, 100);
        assert_eq!(w.players[&id].character.items[4], -1);
        let p = w.players.get_mut(&id).unwrap();
        p.character.items[4] = 0xa23;
        p.character.mp = 25;
        let plan = w.use_item(id, slot, w.players[&id].pos).unwrap().unwrap();
        let p = w.players.get_mut(&id).unwrap();
        p.character.hp = 0;
        p.death_pending = true;
        w.finish_inventory(plan, Ok(2)).unwrap();
        assert_eq!(w.players[&id].character.mp, 25);
    }
    #[test]
    fn selected_container_put_flags_cannot_expose_soulbound_items() {
        let mut w = world();
        let soulbound = w
            .content
            .objects
            .values()
            .find(|o| o.class == "Equipment" && o.soulbound && o.bag_type <= 4)
            .unwrap()
            .kind;
        let mut c = ch(&w);
        c.items[5] = i32::from(soulbound);
        let id = w.enter(c, None).unwrap();
        let pos = w.players[&id].pos;
        let public = w.loot_bag(pos, None, vec![0xa22]).unwrap();
        let from = Slot {
            object: id,
            slot: 5,
            kind: i32::from(soulbound),
        };
        assert!(
            w.swap(
                id,
                from,
                Slot {
                    object: public,
                    slot: 1,
                    kind: -1
                }
            )
            .is_err()
        );
        let private = w
            .loot_bag(pos, Some(1), vec![i32::from(soulbound)])
            .unwrap();
        w.entities.get_mut(&private).unwrap().kind = 0x503;
        assert!(
            w.swap(
                id,
                Slot {
                    object: id,
                    slot: 4,
                    kind: 0xa22
                },
                Slot {
                    object: private,
                    slot: 1,
                    kind: -1
                }
            )
            .is_err()
        );
        w.entities.get_mut(&private).unwrap().kind = 0x504;
        let plan = w
            .swap(
                id,
                from,
                Slot {
                    object: private,
                    slot: 1,
                    kind: -1,
                },
            )
            .unwrap();
        assert_eq!(w.players[&id].character.items[5], i32::from(soulbound));
        w.finish_inventory(plan, Ok(1)).unwrap();
        assert_eq!(w.players[&id].character.items[5], -1);
        assert_eq!(w.entities[&private].items[1], i32::from(soulbound));
    }
    #[test]
    fn late_heal_cannot_revive() {
        let mut w = world();
        let id = w.enter(ch(&w), None).unwrap();
        let plan = w
            .use_item(
                id,
                Slot {
                    object: id,
                    slot: 4,
                    kind: 0xa22,
                },
                w.players[&id].pos,
            )
            .unwrap()
            .unwrap();
        let p = w.players.get_mut(&id).unwrap();
        p.character.hp = 0;
        p.death_pending = true;
        w.finish_inventory(plan, Ok(1)).unwrap();
        assert_eq!(w.players[&id].character.hp, 0);
        assert_eq!(w.players[&id].character.items[4], -1);
        let deaths = w.pending_deaths();
        assert_eq!(deaths.len(), 1);
        assert!(!deaths[0].1.alive);
        assert!(w.pending_deaths().is_empty());
        w.finish_death(id).unwrap();
        assert!(!w.players[&id].character.alive);
    }
    #[test]
    fn failed_pickup_quarantines_both_owners() {
        let mut w = world();
        let id = w.enter(ch(&w), None).unwrap();
        let bag = w
            .entities
            .values()
            .find(|e| e.kind == 0x806)
            .unwrap()
            .clone();
        w.players.get_mut(&id).unwrap().pos = bag.pos;
        let plan = w
            .swap(
                id,
                Slot {
                    object: bag.id,
                    slot: 0,
                    kind: 0xa4f,
                },
                Slot {
                    object: id,
                    slot: 3,
                    kind: -1,
                },
            )
            .unwrap();
        assert!(w.entities[&bag.id].reserved);
        assert!(
            w.finish_inventory(plan, Err(anyhow::anyhow!("synthetic storage failure")))
                .is_err()
        );
        assert!(w.players[&id].pending && w.entities[&bag.id].reserved);
        assert!(w.begin_departure(id).is_err());
        assert_eq!(w.entities[&bag.id].items[0], 0xa4f);
        assert_eq!(w.players[&id].character.items[3], -1);
    }
    #[tokio::test]
    async fn sourced_private_chest_publishes_only_atomic_character_vault_commit() {
        use crate::store::{InventoryWrite, Store};
        let content = Arc::new(Content::load(std::path::Path::new("data")).unwrap());
        let store = Store::open(std::path::Path::new(":memory:"), &content).unwrap();
        let account = store.list("vault-actor".into()).await.unwrap();
        let ch = store
            .create(account.id, content.get(crate::content::WIZARD).unwrap())
            .await
            .unwrap();
        let map = Arc::new(Map::load(std::path::Path::new("data/vault.wmap"), &content).unwrap());
        let mut w = World::with_ids(content, map, Arc::new(AtomicI32::new(1))).unwrap();
        w.name = "Vault".into();
        w.owner = Some(account.id);
        w.safe = true;
        let original = store.vault(account.id).await.unwrap();
        let mut foreign = original.clone();
        foreign.account += 1;
        assert!(w.load_vault(foreign).is_err());
        w.load_vault(original.clone()).unwrap();
        let player = w.enter(ch, None).unwrap();
        let chest = w.vault.as_ref().unwrap().0;
        w.players.get_mut(&player).unwrap().pos = w.entities[&chest].pos;
        let plan = w
            .swap(
                player,
                Slot {
                    object: player,
                    slot: 4,
                    kind: 0xa22,
                },
                Slot {
                    object: chest,
                    slot: 0,
                    kind: -1,
                },
            )
            .unwrap();
        assert_eq!(w.entities[&chest].items[0], -1);
        assert_eq!(store.vault(account.id).await.unwrap().items[0], -1);
        let revisions = store
            .exchange(
                vec![InventoryWrite {
                    id: plan.character,
                    account: plan.account,
                    revision: plan.revision,
                    items: plan.items.clone(),
                }],
                plan.vault.clone(),
            )
            .await
            .unwrap();
        w.finish_inventory(plan, Ok(revisions[0])).unwrap();
        assert_eq!(w.players[&player].character.items[4], -1);
        assert_eq!(w.entities[&chest].items[0], 0xa22);
        let saved = store.vault(account.id).await.unwrap();
        assert_eq!(saved.items[0], 0xa22);
        assert_eq!(saved.revision, 1);
        assert!(w.load_vault(original).is_err());
        w.load_vault(saved).unwrap();
    }
    #[test]
    fn ground_drop_is_hidden_until_commit_and_quarantines_uncertain_failure() {
        let mut w = world();
        let owner = w.enter(ch(&w), None).unwrap();
        let mut second = ch(&w);
        second.id = 2;
        second.account = 2;
        let other = w.enter(second, None).unwrap();
        assert!(
            w.drop_item(
                owner,
                Slot {
                    object: other,
                    slot: 4,
                    kind: 0xa22
                }
            )
            .is_err()
        );
        let plan = w
            .drop_item(
                owner,
                Slot {
                    object: owner,
                    slot: 4,
                    kind: 0xa22,
                },
            )
            .unwrap();
        let ground = plan.dropped.unwrap();
        assert!(!w.visible(owner).contains(&ground));
        assert!(!w.visible(other).contains(&ground));
        assert_eq!(w.players[&owner].character.items[4], 0xa22);
        assert!(
            w.drop_item(
                owner,
                Slot {
                    object: owner,
                    slot: 4,
                    kind: 0xa22
                }
            )
            .is_err()
        );
        w.finish_inventory(plan, Ok(1)).unwrap();
        assert_eq!(w.players[&owner].character.items[4], -1);
        assert!(w.visible(other).contains(&ground));
        let pick = w
            .swap(
                other,
                Slot {
                    object: ground,
                    slot: 0,
                    kind: 0xa22,
                },
                Slot {
                    object: other,
                    slot: 5,
                    kind: -1,
                },
            )
            .unwrap();
        w.finish_inventory(pick, Ok(1)).unwrap();
        assert_eq!(w.entities[&ground].items[0], -1);
        assert_eq!(w.players[&other].character.items[5], 0xa22);
        let plan = w
            .drop_item(
                other,
                Slot {
                    object: other,
                    slot: 5,
                    kind: 0xa22,
                },
            )
            .unwrap();
        let uncertain = plan.dropped.unwrap();
        assert!(
            w.finish_inventory(plan, Err(anyhow::anyhow!("lost database result")))
                .is_err()
        );
        assert!(w.players[&other].pending);
        assert!(w.begin_departure(other).is_err());
        w.now += 60_000;
        w.step();
        assert!(w.entities[&uncertain].reserved);
        assert!(!w.visible(owner).contains(&uncertain));
        assert_eq!(w.players[&other].character.items[5], 0xa22);
    }
    #[test]
    fn empty_ground_bag_despawns_only_after_commit_before_timeout() {
        let mut w = world();
        let owner = w.enter(ch(&w), None).unwrap();
        let bag = w
            .loot_bag(w.players[&owner].pos, None, vec![0xa22])
            .unwrap();
        let plan = w
            .swap(
                owner,
                Slot {
                    object: bag,
                    slot: 0,
                    kind: 0xa22,
                },
                Slot {
                    object: owner,
                    slot: 5,
                    kind: -1,
                },
            )
            .unwrap();
        w.step();
        assert!(w.entities.contains_key(&bag));
        assert_eq!(w.players[&owner].character.items[5], -1);
        w.finish_inventory(plan, Ok(1)).unwrap();
        assert!(w.now < 60_000);
        w.step();
        assert!(!w.entities.contains_key(&bag));
        assert!(!w.bag_expiry.contains_key(&bag));
        assert!(w.cells.iter().all(|cell| !cell.contains(&bag)));
        assert_eq!(w.players[&owner].character.items[5], 0xa22);
    }
    #[test]
    fn private_loot_is_server_scoped_and_reservations_protect_expiry() {
        let mut w = world();
        let owner = w.enter(ch(&w), None).unwrap();
        let mut ch2 = ch(&w);
        ch2.id = 2;
        ch2.account = 2;
        let other = w.enter(ch2, None).unwrap();
        let pos = w.players[&owner].pos;
        let bag = w.loot_bag(pos, Some(1), vec![0xa4f]).unwrap();
        assert!(w.visible(owner).contains(&bag));
        assert!(!w.visible(other).contains(&bag));
        for observer in [Some(other), None] {
            assert!(
                !w.inspect_map(observer).unwrap()["entities"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|e| e["id"] == bag)
            );
        }
        assert!(
            w.inspect_map(Some(owner)).unwrap()["entities"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["id"] == bag)
        );
        assert!(w.inspect_map(Some(-999)).is_err());
        assert!(
            !w.observe(other).unwrap()["entities"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["id"] == bag)
        );
        let from = Slot {
            object: bag,
            slot: 0,
            kind: 0xa4f,
        };
        assert!(
            w.swap(
                other,
                from,
                Slot {
                    object: other,
                    slot: 5,
                    kind: -1
                }
            )
            .is_err()
        );
        let plan = w
            .swap(
                owner,
                from,
                Slot {
                    object: owner,
                    slot: 5,
                    kind: -1,
                },
            )
            .unwrap();
        w.now = 60_000;
        w.step();
        assert!(w.entities.contains_key(&bag));
        assert_eq!(w.players[&owner].character.items[5], -1);
        w.finish_inventory(plan, Ok(1)).unwrap();
        w.step();
        assert!(!w.entities.contains_key(&bag));
        assert_eq!(w.players[&owner].character.items[5], 0xa4f);
        assert_eq!(w.players[&other].character.items[5], -1);
        assert_eq!(w.cells.iter().filter(|c| c.contains(&bag)).count(), 0);
    }
    #[test]
    fn native_projectile_observation_exposes_only_the_players_friendly_origins() {
        let mut w = world();
        let a = w.enter(ch(&w), None).unwrap();
        let mut other = ch(&w);
        other.id = 2;
        other.account = 2;
        let b = w.enter(other, None).unwrap();
        w.now = 1000;
        for id in [a, b] {
            let p = w.players[&id].pos;
            w.shoot(
                id,
                1000,
                1,
                2711,
                Pos {
                    x: p.x + 0.3,
                    y: p.y,
                },
                0.,
            )
            .unwrap();
        }
        let mut hostile = w.bullets[0].clone();
        hostile.friendly = false;
        w.bullets.push(hostile);
        let observed = w.observe(a).unwrap();
        let own = observed["own_projectiles"].as_array().unwrap();
        assert_eq!(own.len(), 1);
        assert_eq!(
            own[0]["start"],
            serde_json::to_value(w.bullets[0].start).unwrap()
        );
        assert_eq!(own[0]["angle"], 0.);
    }
    #[test]
    fn native_shoot_before_move_uses_the_same_speed_time_and_wall_budget() {
        let mut w = world();
        let id = w.enter(ch(&w), None).unwrap();
        let pos = (1..w.map.height - 1)
            .flat_map(|y| {
                (1..w.map.width - 1).map(move |x| Pos {
                    x: x as f32 + 0.5,
                    y: y as f32 + 0.5,
                })
            })
            .find(|p| {
                (-1..=1).all(|dy| {
                    (-1..=1).all(|dx| {
                        w.walkable(Pos {
                            x: p.x + dx as f32,
                            y: p.y + dy as f32,
                        })
                    })
                })
            })
            .unwrap();
        let p = w.players.get_mut(&id).unwrap();
        p.pos = pos;
        p.last_move = 1000;
        p.last_move_at = 0;
        p.movement_credit = 0.;
        w.now = 100;
        let origin = Pos {
            x: pos.x + 0.673777,
            y: pos.y - 0.139555,
        };
        let angle = (-0.139555_f32).atan2(0.673777);
        assert!(
            w.shoot(
                id,
                1091,
                1,
                0xa97,
                Pos {
                    x: pos.x + 20.,
                    y: pos.y
                },
                0.
            )
            .is_err()
        );
        w.shoot(id, 1091, 1, 0xa97, origin, angle).unwrap();
        assert_eq!(w.players[&id].pos.x, pos.x);
        assert_eq!(w.players[&id].last_move, 1000);
        assert_eq!(w.players[&id].movement_credit, 0.);
        assert_eq!(w.players[&id].character.shots, 1);
        assert!(
            w.validate_move(
                id,
                Pos {
                    x: pos.x + 0.6,
                    y: pos.y
                },
                1091
            )
            .is_err()
        );
        assert!(w.validate_move(id, pos, 999).is_err());
        assert!(w.move_to(id, pos, 2001).is_err());
        w.now = 5100;
        let stationary = Pos {
            x: pos.x + 0.3 * angle.cos(),
            y: pos.y + 0.3 * angle.sin(),
        };
        w.shoot(id, 6000, 2, 0xa97, stationary, angle).unwrap();
        w.now = 5600;
        w.shoot(
            id,
            6500,
            3,
            0xa97,
            Pos {
                x: pos.x + 0.35,
                y: pos.y,
            },
            0.,
        )
        .unwrap();
        assert_eq!(w.players[&id].last_move, 1000);
        assert_eq!(w.players[&id].movement_credit, 0.);
        assert!(
            w.shoot(
                id,
                7000,
                3,
                0xa97,
                Pos {
                    x: f32::NAN,
                    y: pos.y
                },
                angle
            )
            .is_err()
        );
        let blocker = *w.content.objects.iter().find(|(_, o)| o.blocks).unwrap().0;
        w.spawn_entity(blocker, pos, vec![]).unwrap();
        assert!(
            w.validate_move(
                id,
                Pos {
                    x: pos.x + 0.1,
                    y: pos.y
                },
                1091
            )
            .is_err()
        );
    }
    #[test]
    fn hiscore_snapshots_exclude_qa_and_keep_the_latest_fame_until_death_commits() {
        let mut d = Directory::load(Path::new("data"), world().content, 16).unwrap();
        let world_id = d.initial(-2).unwrap();
        let (id, _receiver) = receive(&mut d, world_id, 1, WIZARD);
        let w = d.worlds.get_mut(&world_id).unwrap();
        w.players.get_mut(&id).unwrap().character.fame = 100;
        w.players.get_mut(&id).unwrap().death_pending = true;
        assert_eq!(d.competitive_characters()[0].fame, 100);
        d.worlds
            .get_mut(&world_id)
            .unwrap()
            .players
            .get_mut(&id)
            .unwrap()
            .controller = Controller::Qa;
        assert!(d.competitive_characters().is_empty());
        let p = d
            .worlds
            .get_mut(&world_id)
            .unwrap()
            .players
            .get_mut(&id)
            .unwrap();
        p.controller = Controller::Human;
        p.persistence_failure = Some("test storage failure".into());
        assert!(d.competitive_characters().is_empty());
    }
    #[test]
    fn only_human_enemy_drop_pickups_stage_collection_and_player_drops_never_do() {
        for controller in [Controller::Human, Controller::Qa, Controller::Autonomous] {
            let (mut w, id, enemy) = encounter(0x683);
            w.players.get_mut(&id).unwrap().controller = controller;
            w.rng = rand::rngs::StdRng::seed_from_u64(5);
            let mut defeated = w.remove_entity(enemy);
            defeated.damage.insert(w.players[&id].character.id, 100);
            for _ in 0..32 {
                w.drop_loot(&defeated).unwrap();
                if w.entities.values().any(|e| !e.drop_slots.is_empty()) {
                    break;
                }
            }
            let bag = w
                .entities
                .values()
                .find(|e| !e.drop_slots.is_empty())
                .unwrap()
                .clone();
            let slot = *bag.drop_slots.first().unwrap();
            let kind = bag.items[slot];
            w.players.get_mut(&id).unwrap().pos = bag.pos;
            let plan = w
                .swap(
                    id,
                    Slot {
                        object: bag.id,
                        slot,
                        kind,
                    },
                    Slot {
                        object: id,
                        slot: 11,
                        kind: -1,
                    },
                )
                .unwrap();
            assert_eq!(
                plan.collected,
                if controller == Controller::Human {
                    Some(kind as u16)
                } else {
                    None
                }
            );
            assert!(w.entities[&bag.id].drop_slots.contains(&slot));
            w.finish_inventory(plan, Ok(1)).unwrap();
            assert!(!w.entities[&bag.id].drop_slots.contains(&slot));
            let plan = w
                .drop_item(
                    id,
                    Slot {
                        object: id,
                        slot: 11,
                        kind,
                    },
                )
                .unwrap();
            let dropped = plan.dropped.unwrap();
            assert_eq!(plan.collected, None);
            w.finish_inventory(plan, Ok(2)).unwrap();
            let plan = w
                .swap(
                    id,
                    Slot {
                        object: dropped,
                        slot: 0,
                        kind,
                    },
                    Slot {
                        object: id,
                        slot: 11,
                        kind: -1,
                    },
                )
                .unwrap();
            assert_eq!(plan.collected, None);
        }
    }
    #[test]
    fn dungeon_clear_counter_is_staged_until_progress_commit() {
        let (mut w, player, boss) = encounter(0x927);
        w.name = "Pirate Cave".into();
        let mut defeated = w.remove_entity(boss);
        defeated.damage.insert(w.players[&player].character.id, 101);
        w.drop_loot(&defeated).unwrap();
        assert!(w.cleared);
        assert_eq!(w.players[&player].character.pirate_caves, 0);
        assert!(w.entities.values().any(|e| e.kind == 0x71d && !e.published));
        assert!(!w.entities.values().any(|e| e.kind == 0x71d && e.published));
        assert!(w.drop_loot(&defeated).is_err());
        let plans = w.pending_progress().unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].1.pirate_caves, 1);
        assert!(w.pending_progress().unwrap().is_empty());
        assert!(w.begin_departure(player).unwrap().is_none());
        w.finish_progress(player, plans[0].1.clone()).unwrap();
        assert_eq!(w.players[&player].character.pirate_caves, 1);
        assert!(w.entities.values().any(|e| e.kind == 0x71d && e.published));
        assert!(w.clear_exit.is_none());
    }
    fn encounter(kind: u16) -> (World, i32, i32) {
        let mut w = world();
        let obsolete: Vec<_> = w
            .entities
            .values()
            .filter(|e| w.content.objects[&e.kind].enemy)
            .map(|e| e.id)
            .collect();
        for id in obsolete {
            w.remove_entity(id);
        }
        let player = w.enter(ch(&w), None).unwrap();
        let pos = w.players[&player].pos;
        let npc = w
            .spawn_entity(
                kind,
                Pos {
                    x: pos.x + 4.,
                    y: pos.y,
                },
                vec![],
            )
            .unwrap();
        w.encounter = true;
        (w, player, npc)
    }
    #[test]
    fn dropped_portal_binds_both_players_to_one_cave_and_expires() {
        let mut d = Directory::load(Path::new("data"), world().content, 16).unwrap();
        let w = d.worlds.get_mut(&-3).unwrap();
        let pos = w.population[0].pos;
        let a = w.enter(ch(w), None).unwrap();
        let mut other = ch(w);
        other.id = 2;
        other.account = 2;
        let b = w.enter(other, None).unwrap();
        w.players.get_mut(&a).unwrap().pos = pos;
        w.players.get_mut(&b).unwrap().pos = pos;
        let portal = d.drop_portal(-3, pos).unwrap();
        d.worlds
            .get_mut(&-3)
            .unwrap()
            .entities
            .get_mut(&portal)
            .unwrap()
            .published = false;
        assert!(
            d.destination(a, Some(portal))
                .unwrap_err()
                .to_string()
                .contains("portal not published")
        );
        d.worlds
            .get_mut(&-3)
            .unwrap()
            .entities
            .get_mut(&portal)
            .unwrap()
            .published = true;
        let (_, _, destination) = d.destination(a, Some(portal)).unwrap();
        assert_eq!(d.destination(b, Some(portal)).unwrap().2, destination);
        assert_eq!(d.worlds[&destination].name, "Pirate Cave");
        d.worlds
            .get_mut(&-3)
            .unwrap()
            .players
            .get_mut(&b)
            .unwrap()
            .pos = Pos {
            x: pos.x + 3.,
            y: pos.y,
        };
        assert!(d.destination(b, Some(portal)).is_err());
        let fake = d
            .worlds
            .get_mut(&-3)
            .unwrap()
            .spawn_entity(0x717, pos, vec![])
            .unwrap();
        assert!(d.destination(a, Some(fake)).is_err());
        d.tick = 599;
        d.step();
        assert!(!d.portals.contains_key(&portal));
        assert!(!d.worlds[&-3].entities.contains_key(&portal));
        assert!(d.destination(a, Some(portal)).is_err());
    }
    #[test]
    fn shore_population_is_bounded_sourced_and_cannot_respawn_on_players() {
        let mut d = Directory::load(Path::new("data"), world().content, 16).unwrap();
        let w = d.worlds.get_mut(&-3).unwrap();
        assert_eq!(w.population.len(), 24);
        let (x, y) = w.map.spawns[0];
        let arrival = Pos { x, y };
        for spawn in &w.population {
            assert!((12.0..60.0).contains(&spawn.pos.distance(arrival)));
            let cell =
                &w.map.cells[spawn.pos.y as usize * w.map.width as usize + spawn.pos.x as usize];
            assert!(matches!(cell.terrain, 11 | 12));
            assert!(w.walkable(spawn.pos));
            assert!(matches!(spawn.kind, 0x600 | 0x60e | 0x60f | 0x610));
        }
        let pos = w.population[0].pos;
        let old = w.population[0].entity.unwrap();
        w.remove_entity(old);
        w.repopulate().unwrap();
        assert!(w.population[0].entity.is_none());
        let player = w.enter(ch(w), None).unwrap();
        w.players.get_mut(&player).unwrap().pos = pos;
        w.now = 60_000;
        w.repopulate().unwrap();
        assert!(w.population[0].entity.is_none());
        w.players.get_mut(&player).unwrap().pos = Pos { x: -30., y: -30. };
        w.repopulate().unwrap();
        let new = w.population[0].entity.unwrap();
        assert_ne!(old, new);
        assert_eq!(
            w.entities_at(pos)
                .filter(|e| e.kind == w.population[0].kind)
                .count(),
            1
        );
        assert_eq!(
            w.entities
                .values()
                .filter(|e| w.content.objects[&e.kind].enemy)
                .count(),
            24
        );
    }
    #[test]
    fn moving_chaser_stays_indexed_and_fires_real_selected_projectiles() {
        let (mut w, player, npc) = encounter(0x687);
        let initial = w.entities[&npc].pos;
        for _ in 0..20 {
            w.step();
        }
        let current = w.entities[&npc].pos;
        assert!(initial.distance(current) > 2.);
        assert!(current.distance(w.players[&player].pos) <= 1.3);
        assert_eq!(w.cells.iter().filter(|cell| cell.contains(&npc)).count(), 1);
        assert!(w.entities_at(current).any(|e| e.id == npc));
        assert!(w.players[&player].character.hp < 100);
        let before = current;
        assert!(w.move_entity(npc, Pos { x: -10., y: -10. }, 100.).is_err());
        assert_eq!(w.entities[&npc].pos.distance(before), 0.);
        assert!(w.entities_at(before).any(|e| e.id == npc));
    }
    #[test]
    fn boss_emits_both_attacks_only_after_threshold_and_safe_worlds_do_not_run_ai() {
        let (mut w, _, boss) = encounter(0x927);
        w.step();
        assert!(!w.bullets.iter().any(|b| b.owner == boss));
        w.entities.get_mut(&boss).unwrap().hp = 639;
        for _ in 0..10 {
            w.step();
        }
        let attacks: Vec<_> = w
            .bullets
            .iter()
            .filter(|b| b.owner == boss)
            .map(|b| b.spec.speed)
            .collect();
        assert!(attacks.contains(&75.));
        assert!(attacks.contains(&130.));
        let pos = w.entities[&boss].pos;
        w.safe = true;
        for _ in 0..40 {
            w.step();
        }
        assert_eq!(w.entities[&boss].pos.distance(pos), 0.);
        assert!(!w.bullets.iter().any(|b| b.owner == boss));
    }
    #[test]
    fn overlapping_entities_preserve_collision_visibility_and_removal() {
        let mut w = world();
        let (x, y) = w.map.spawns[0];
        let pos = Pos { x, y };
        let first = w.spawn_entity(0x806, pos, vec![-1; 8]).unwrap();
        let second = w.spawn_entity(0x806, pos, vec![-1; 8]).unwrap();
        assert_eq!(w.entities_at(pos).count(), 2);
        assert_eq!(
            w.near_entities(pos, pos)
                .filter(|e| e.id == first || e.id == second)
                .count(),
            2
        );
        let wall = w.spawn_entity(0x163, pos, vec![]).unwrap();
        assert!(!w.walkable(pos));
        assert!(w.spawn_entity(0x806, pos, vec![]).is_err());
        w.remove_entity(wall);
        assert!(w.walkable(pos));
        w.remove_entity(first);
        assert_eq!(
            w.entities_at(pos).map(|e| e.id).collect::<Vec<_>>(),
            vec![second]
        );
    }
    #[test]
    fn projectile_keeps_killer_identity_after_emitter_dies() {
        let mut w = world();
        let mut character = ch(&w);
        character.hp = 1;
        let player = w.enter(character, None).unwrap();
        let emitter = w.entities.values().find(|e| e.kind == 0x802).unwrap().id;
        let kind = w.entities[&emitter].kind;
        let pos = w.players[&player].pos;
        w.bullets.push(Bullet {
            owner: emitter,
            source_kind: kind,
            id: 1,
            friendly: false,
            start: pos,
            pos,
            angle: 0.,
            born: 0,
            damage: 100,
            spec: w.content.objects[&kind].projectiles[&0].clone(),
            hit: BTreeSet::new(),
        });
        w.remove_entity(emitter);
        w.step();
        assert!(w.players[&player].death_pending);
        assert_eq!(w.players[&player].killer, w.content.objects[&kind].name);
    }
    #[test]
    fn selected_cover_stops_hostile_projectiles_before_player_damage() {
        for (kind, passes_cover, hp) in [
            (0x120, false, 100),
            (0x120, true, 100),
            (0x100, false, 100),
            (0x100, true, 92),
        ] {
            let mut w = world();
            for id in w.entities.keys().copied().collect::<Vec<_>>() {
                w.remove_entity(id);
            }
            for cell in &mut Arc::make_mut(&mut w.map).cells {
                cell.ground = 0x61;
            }
            let player = w.enter(ch(&w), None).unwrap();
            w.players.get_mut(&player).unwrap().pos = Pos { x: 12.5, y: 100.5 };
            w.spawn_entity(kind, Pos { x: 11.5, y: 100.5 }, vec![])
                .unwrap();
            let start = Pos { x: 10.5, y: 100.5 };
            let mut spec = w.content.objects[&0x683].projectiles[&0].clone();
            spec.passes_cover = passes_cover;
            w.bullets.push(Bullet {
                owner: 500,
                source_kind: 0x683,
                id: 1,
                friendly: false,
                start,
                pos: start,
                angle: 0.,
                born: 0,
                damage: 8,
                spec,
                hit: BTreeSet::new(),
            });
            for _ in 0..5 {
                w.step();
            }
            assert_eq!(
                w.players[&player].character.hp, hp,
                "{kind:#x}/{passes_cover}"
            );
            assert!(w.bullets.is_empty());
        }
    }
    #[test]
    fn native_attack_credit_uses_receive_time_not_skipped_simulation_ticks() {
        let mut w = world();
        let mut character = ch(&w);
        character.base[5] = 26;
        let player = w.enter(character, None).unwrap();
        let pos = w.players[&player].pos;
        let start = Pos {
            x: pos.x + 0.3,
            y: pos.y,
        };
        let p = w.players.get_mut(&player).unwrap();
        p.last_attack = 1000;
        p.last_attack_at = 1000;
        p.attack_credit = 0.;
        w.now = 1200;
        assert!(
            w.shoot_timed(player, (1298, 1200), 1, 0xa97, start, 0.)
                .is_err()
        );
        w.attack_epoch = Instant::now() - std::time::Duration::from_millis(1298);
        w.native_shoot(player, 1298, 1, 0xa97, start, 0.).unwrap();
        w.native_shoot(player, 1298, 2, 0xa97, start, 0.).unwrap();
        assert_eq!(w.players[&player].character.shots, 2);
        w.now = 9000;
        let p = w.players.get_mut(&player).unwrap();
        p.last_attack_at = 1298;
        p.attack_credit = 0.;
        assert!(
            w.shoot_timed(player, (1596, 1400), 3, 0xa97, start, 0.)
                .unwrap_err()
                .to_string()
                .contains("attack exceeds server time budget")
        );
        assert!(
            w.shoot_timed(player, (1596, 1200), 3, 0xa97, start, 0.)
                .unwrap_err()
                .to_string()
                .contains("attack receive clock regressed")
        );
    }
    #[test]
    fn native_integer_attack_period_keeps_the_bounded_server_credit() {
        for dexterity in [15, 17, 18, 25, 75] {
            let mut w = world();
            let mut character = ch(&w);
            character.base[5] = dexterity;
            let player = w.enter(character, None).unwrap();
            let period = (1. / (0.0015_f64 + (0.008 - 0.0015) * dexterity as f64 / 75.)) as i32;
            let pos = w.players[&player].pos;
            let start = Pos {
                x: pos.x + 0.3,
                y: pos.y,
            };
            for attack in 0..2000 {
                let time = attack * period;
                w.now = time as u64;
                for _ in 0..2 {
                    let bullet = w.players[&player].next_bullet;
                    w.shoot(player, time, bullet, 0xa97, start, 0.).unwrap();
                }
                w.bullets.clear();
            }
            assert_eq!(w.players[&player].character.shots, 4000);
            let bullet = w.players[&player].next_bullet;
            assert!(
                w.shoot(player, 2000 * period - 3, bullet, 0xa97, start, 0.)
                    .is_err()
            );
        }
    }
    #[test]
    fn candidate_tutorial_gun_still_reaches_its_open_lane() {
        let mut w = world();
        let player = w.enter(ch(&w), None).unwrap();
        w.players.get_mut(&player).unwrap().pos = Pos { x: 12.2, y: 137.8 };
        assert!((0..200).any(|_| {
            w.step();
            w.players[&player].character.hp < 100
        }));
        w.players.get_mut(&player).unwrap().pos = Pos { x: 12.2, y: 138.0 };
        assert!((0..3000).any(|_| {
            w.step();
            w.players[&player].death_pending
        }));
    }
    #[test]
    fn paired_staff_wave() {
        let w = world();
        let spec = w.content.objects[&0xa97].projectiles[&0].clone();
        let mut b = Bullet {
            owner: 1,
            source_kind: WIZARD,
            id: 1,
            friendly: true,
            start: Pos { x: 0., y: 0. },
            pos: Pos { x: 0., y: 0. },
            angle: 0.,
            born: 0,
            damage: 7,
            spec,
            hit: BTreeSet::new(),
        };
        let a = b.position(60);
        b.id = 2;
        let c = b.position(60);
        assert!((a.x - 1.08).abs() < 0.001);
        assert!((a.y + c.y).abs() < 0.001);
        assert!(a.y.abs() > 0.4);
    }
}
