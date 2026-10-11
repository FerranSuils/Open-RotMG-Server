use crate::{
    content::{Content, Map, Projectile, WIZARD},
    store::Character,
    wire::{self, Packet, Writer},
};
use anyhow::{Context, Result, bail, ensure};
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Instant,
};
use tokio::sync::{mpsc, oneshot, watch};
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
    pub reserved: bool,
    next_shot: u64,
    bullet: u8,
}
pub struct Player {
    pub character: Character,
    pub pos: Pos,
    pub tx: Option<Output>,
    pub departing: bool,
    script_owned: bool,
    pub known: BTreeSet<i32>,
    tiles: BTreeSet<(i16, i16)>,
    pub death_started: bool,
    killer: String,
    movement_credit: f32,
    hp_fraction: f32,
    mp_fraction: f32,
    pub pending: bool,
    pub death_pending: bool,
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
#[derive(Clone, Debug, Serialize, Deserialize)]
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
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Action {
    Move { x: f32, y: f32 },
    Shoot { angle: f32 },
    Stop,
    Use { slot: usize, x: f32, y: f32 },
    Swap { from: Slot, to: Slot },
}

pub struct World {
    pub content: Arc<Content>,
    pub map: Arc<Map>,
    pub entities: BTreeMap<i32, Entity>,
    pub players: BTreeMap<i32, Player>,
    pub bullets: Vec<Bullet>,
    cells: Vec<Option<i32>>,
    rng: rand::rngs::StdRng,
    pub now: u64,
    pub tick: u64,
    next_id: i32,
    pub max_tick_us: u64,
}
type Job = Box<dyn FnOnce(&mut World) + Send>;
#[derive(Clone)]
pub struct Handle {
    tx: mpsc::Sender<Job>,
}
impl Handle {
    pub fn spawn(world: World) -> Self {
        let (tx, mut rx) = mpsc::channel::<Job>(256);
        tokio::spawn(async move {
            let mut world = world;
            let mut timer = tokio::time::interval(std::time::Duration::from_millis(50));
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {_ =timer.tick()=>{let start=Instant::now();world.step();let us=start.elapsed().as_micros()as u64;world.max_tick_us=world.max_tick_us.max(us);if us>25_000{tracing::warn!(tick=world.tick,elapsed_us=us,"slow simulation tick");}
                if world.tick.is_multiple_of(100){tracing::info!(tick=world.tick,players=world.players.len(),entities=world.entities.len(),bullets=world.bullets.len(),max_tick_us=world.max_tick_us,"world health");world.max_tick_us=0;}},job=rx.recv()=>match job{Some(f)=>f(&mut world),None=>break}}
            }
        });
        Self { tx }
    }
    pub async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut World) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Box::new(move |w| {
                let _ = tx.send(f(w));
            }))
            .await
            .map_err(|_| anyhow::anyhow!("world stopped"))?;
        rx.await.context("world worker failed")?
    }
}
fn send(p: &Player, id: u8, body: Vec<u8>) {
    if let Some(tx) = &p.tx
        && tx.packets.try_send(Packet { id, body }).is_err()
    {
        let _ = tx.lost.send(true);
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
    pub fn new(content: Arc<Content>, map: Arc<Map>) -> Result<Self> {
        let mut entities = BTreeMap::new();
        let mut cells = vec![None; map.cells.len()];
        let mut id = 1000;
        for (y, row) in map.cells.chunks(map.width as usize).enumerate() {
            for (x, c) in row.iter().enumerate() {
                if let Some(kind) = c.object {
                    let o = content.get(kind)?;
                    let mut e = Entity {
                        id,
                        kind,
                        name: String::new(),
                        pos: Pos {
                            x: x as f32 + 0.5,
                            y: y as f32 + 0.5,
                        },
                        hp: o.hp,
                        size: 100,
                        condition: 0,
                        items: o.equipment.clone(),
                        reserved: false,
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
                            _ => bail!("unsupported map setting {k}"),
                        }
                    }
                    cells[y * map.width as usize + x] = Some(id);
                    entities.insert(id, e);
                    id += 1;
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
            rng: rand::rngs::StdRng::seed_from_u64(1),
            now: 0,
            tick: 0,
            next_id: 100_000,
            max_tick_us: 0,
        })
    }
    pub fn map_info(&self) -> Vec<u8> {
        let mut o = Writer::new();
        o.i32(self.map.width)
            .i32(self.map.height)
            .utf("Development tutorial")
            .u32(1)
            .i32(0)
            .u8(0)
            .u8(1)
            .i16(0)
            .i16(0);
        o.0
    }
    pub fn enter(&mut self, ch: Character, tx: Option<Output>) -> Result<i32> {
        ensure!(ch.alive && ch.hp > 0, "dead character");
        ensure!(self.players.len() < 85, "development world full");
        ensure!(
            !self
                .players
                .values()
                .any(|p| p.character.account == ch.account),
            "account already in this world"
        );
        let id = self.next_id;
        self.next_id += 1;
        let (x, y) = self.map.spawns[self.players.len() % self.map.spawns.len()];
        let char_id = ch.id;
        self.players.insert(
            id,
            Player {
                character: ch,
                pos: Pos { x, y },
                departing: false,
                script_owned: tx.is_none(),
                tx,
                known: BTreeSet::new(),
                tiles: BTreeSet::new(),
                death_started: false,
                killer: String::new(),
                movement_credit: 0.78,
                hp_fraction: 0.,
                mp_fraction: 0.,
                pending: false,
                death_pending: false,
                bot_goal: None,
                bot_aim: None,
                next_bullet: 1,
                server_bullet: 128,
                seed: 1,
                last_attack: -1000,
                last_attack_at: self.now,
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
            "player entered development world"
        );
        Ok(id)
    }
    pub fn leave(&mut self, id: i32) -> Result<Character> {
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
            let w = self.content.get(WIZARD)?;
            o.i32(id).f32(p.pos.x).f32(p.pos.y).i16(33);
            for (k, v) in [
                (0, w.hp),
                (1, p.character.hp),
                (2, 100),
                (3, w.mp),
                (4, p.character.mp),
                (5, 50),
                (6, p.character.exp),
                (7, p.character.level),
            ] {
                stat(&mut o, k, v);
            }
            for (i, item) in p.character.items.iter().enumerate() {
                stat(&mut o, 8 + i as u8, *item);
            }
            for (k, v) in [
                (20, w.attack),
                (21, self.defense(id)?),
                (22, w.speed),
                (26, w.vitality),
                (27, w.wisdom),
                (28, w.dexterity),
                (29, 0),
                (30, 0),
            ] {
                stat(&mut o, k, v);
            }
            text_stat(&mut o, 31, &p.character.name);
            stat(&mut o, 32, 0);
            stat(&mut o, 33, 0);
            text_stat(&mut o, 62, "");
            stat(&mut o, 63, 0);
        } else {
            let e = self.entities.get(&id).context("entity missing")?;
            o.i32(id)
                .f32(e.pos.x)
                .f32(e.pos.y)
                .i16((4 + usize::from(!e.name.is_empty()) + e.items.len()) as i16);
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
        }
        Ok(o.0)
    }
    fn defense(&self, id: i32) -> Result<i32> {
        let p = &self.players[&id];
        let mut d = self.content.get(WIZARD)?.defense;
        for v in p.character.items.iter().take(4).filter(|v| **v >= 0) {
            for (k, n) in &self.content.get(*v as u16)?.bonuses {
                if *k == 21 {
                    d += n;
                }
            }
        }
        Ok(d)
    }
    fn visible(&self, id: i32) -> BTreeSet<i32> {
        let pos = self.players[&id].pos;
        self.entities
            .values()
            .filter(|e| e.pos.distance(pos) < 20.)
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
        let added: Vec<i32> = visible.difference(previous).copied().collect();
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
                    WIZARD
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
        self.players.get_mut(&id).unwrap().known = visible;
        Ok(())
    }
    pub fn move_to(&mut self, id: i32, pos: Pos, time: i32) -> Result<()> {
        ensure!(
            pos.x.is_finite() && pos.y.is_finite(),
            "non-finite movement"
        );
        let p = self.players.get(&id).context("player missing")?;
        ensure!(
            p.character.alive && !p.death_pending,
            "dead player cannot move"
        );
        ensure!(time >= 0, "negative movement time");
        let elapsed = if p.last_move == 0 {
            100.
        } else {
            (i64::from(time) - i64::from(p.last_move)) as f32
        };
        ensure!((0. ..=1000.).contains(&elapsed), "movement time jump");
        let cell = self
            .map
            .cell(p.pos.x, p.pos.y)
            .context("player outside map")?;
        let speed = (0.004 + (0.0096 - 0.004) * self.content.get(WIZARD)?.speed as f32 / 75.)
            * self.content.grounds[&cell.ground];
        ensure!(
            p.pos.distance(pos) <= speed * elapsed + 0.3,
            "movement exceeds speed"
        );
        let credit = (p.movement_credit + speed * (self.now - p.last_move_at) as f32)
            .min(speed * 350. + 0.3);
        let distance = p.pos.distance(pos);
        ensure!(
            distance <= credit + 0.001,
            "movement exceeds server time budget"
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
        let p = self.players.get_mut(&id).unwrap();
        p.movement_credit = credit - distance;
        p.pos = pos;
        p.last_move = time;
        p.last_move_at = self.now;
        Ok(())
    }
    pub fn walkable(&self, pos: Pos) -> bool {
        let Some(c) = self.map.cell(pos.x, pos.y) else {
            return false;
        };
        if self.content.grounds[&c.ground] <= 0. {
            return false;
        }
        self.entity_at(pos)
            .is_none_or(|e| !self.content.objects[&e.kind].blocks)
    }
    fn entity_at(&self, pos: Pos) -> Option<&Entity> {
        self.map.cell(pos.x, pos.y)?;
        self.cells[pos.y as usize * self.map.width as usize + pos.x as usize].map(|id| {
            self.entities
                .get(&id)
                .expect("occupied cell missing entity")
        })
    }
    fn near_entities(&self, a: Pos, b: Pos) -> impl Iterator<Item = &Entity> {
        let x0 = (a.x.min(b.x) as i32 - 1).max(0);
        let x1 = (a.x.max(b.x) as i32 + 1).min(self.map.width - 1);
        let y0 = (a.y.min(b.y) as i32 - 1).max(0);
        let y1 = (a.y.max(b.y) as i32 + 1).min(self.map.height - 1);
        (y0..=y1).flat_map(move |y| {
            (x0..=x1).filter_map(move |x| {
                self.cells[(y * self.map.width + x) as usize].map(|id| {
                    self.entities
                        .get(&id)
                        .expect("occupied cell missing entity")
                })
            })
        })
    }
    pub fn shoot(
        &mut self,
        id: i32,
        time: i32,
        bullet: u8,
        kind: u16,
        start: Pos,
        angle: f32,
    ) -> Result<()> {
        ensure!(angle.is_finite(), "invalid shot angle");
        let p = self.players.get(&id).context("player missing")?;
        ensure!(
            p.character.alive && !p.death_pending,
            "dead player cannot shoot"
        );
        ensure!(
            p.character.items[0] == kind as i32,
            "weapon does not match equipped item"
        );
        ensure!(
            p.pos.distance(start) <= 0.6,
            "shot origin too far from player"
        );
        let item = self.content.get(kind)?;
        let spec = item
            .projectile
            .clone()
            .context("weapon has no projectile")?;
        let cooldown = 1.
            / ((0.0015 + (0.008 - 0.0015) * self.content.get(WIZARD)?.dexterity as f32 / 75.)
                * item.rate);
        let p = self.players.get_mut(&id).unwrap();
        ensure!(time >= 0, "negative shot time");
        if time != p.last_attack {
            ensure!(
                i64::from(time) - i64::from(p.last_attack) >= cooldown as i64 - 2,
                "attack too fast"
            );
            let credit =
                (p.attack_credit + (self.now - p.last_attack_at) as f32).min(cooldown * 2.);
            ensure!(
                credit >= cooldown - 50.,
                "attack exceeds server time budget"
            );
            p.attack_credit = credit - cooldown;
            p.last_attack_at = self.now;
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
        p.character.shots += 1;
        let draw = if spec.min == spec.max {
            spec.min
        } else {
            p.seed = ((p.seed as u64 * 16807) % 2147483647) as u32;
            spec.min + (p.seed % (spec.max - spec.min) as u32) as i32
        };
        let damage =
            (draw as f32 * (0.5 + 1.5 * self.content.get(WIZARD)?.attack as f32 / 75.)) as i32;
        self.bullets.push(Bullet {
            owner: id,
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
        let p = self.players.get(&id).context("player missing")?;
        ensure!(
            !p.pending && !p.death_pending && p.character.alive,
            "player is busy or dead"
        );
        ensure!(
            slot.object == id && slot.slot < 12 && p.character.items[slot.slot] == slot.kind,
            "item ownership mismatch"
        );
        ensure!(slot.kind >= 0, "empty item slot");
        let item = self.content.get(slot.kind as u16)?.clone();
        match item.activate.as_str() {
            "Heal" => {
                let mut items = p.character.items.clone();
                items[slot.slot] = -1;
                self.players.get_mut(&id).unwrap().pending = true;
                Ok(Some(self.plan(id, items, None, item.heal)))
            }
            "BulletNova" => {
                ensure!(p.character.mp >= item.mana_cost, "not enough mana");
                ensure!(self.now >= p.last_ability + 500, "ability cooldown");
                ensure!(p.pos.distance(pos) <= 15., "ability target too far");
                let spec = item.projectile.context("spell has no projectile")?;
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
        }
    }
    pub fn swap(&mut self, id: i32, from: Slot, to: Slot) -> Result<InventoryPlan> {
        let p = self.players.get(&id).context("player missing")?;
        ensure!(
            !p.pending && !p.death_pending && p.character.alive,
            "player busy or dead"
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
            if s.object == id && s.slot < 4 && k >= 0 {
                let obj = self.content.get(k as u16)?;
                ensure!(
                    !obj.unsupported_equipment,
                    "equipment effect not implemented"
                );
                ensure!(
                    obj.slot_type == self.content.get(WIZARD)?.slots[s.slot],
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
        Ok(self.plan(id, items, bag_id.map(|b| (b, bag)), 0))
    }
    pub fn finish_inventory(&mut self, plan: InventoryPlan, result: Result<i32>) -> Result<()> {
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
        let mut o = Writer::new();
        match result {
            Ok(revision) => {
                p.character.items = plan.items;
                p.character.revision = revision;
                if plan.healing > 0 && !p.death_pending {
                    p.character.hp =
                        (p.character.hp + plan.healing).min(self.content.get(WIZARD)?.hp);
                }
                if let Some((id, items)) = plan.bag {
                    self.entities.get_mut(&id).unwrap().items = items;
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
            p.script_owned,
            "script actions require a script-owned player; use /dev/join"
        );
        ensure!(
            p.character.alive && !p.death_pending,
            "dead player cannot act"
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
        }
    }
    pub fn correct_position(&mut self, id: i32, time: i32, reason: &str) -> Result<()> {
        let p = self.players.get_mut(&id).context("player missing")?;
        p.last_move = time;
        let mut out = Writer::new();
        out.i32(id).f32(p.pos.x).f32(p.pos.y);
        send(p, 52, out.0);
        tracing::warn!(
            player = id,
            reason,
            "movement rejected; sent authoritative GOTO"
        );
        Ok(())
    }
    pub fn pending_deaths(&mut self) -> Vec<(i32, Character, String)> {
        self.players
            .iter_mut()
            .filter(|(_, p)| p.death_pending && !p.pending && !p.death_started)
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
        let p = self
            .players
            .get_mut(&id)
            .context("departing player missing")?;
        p.departing = true;
        p.tx = None;
        p.bot_goal = None;
        p.bot_aim = None;
        if p.pending || p.death_pending {
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
    pub fn observe(&self, id: i32) -> Result<serde_json::Value> {
        let p = self.players.get(&id).context("player missing")?;
        let entities: Vec<_> = self
            .entities
            .values()
            .filter(|e| p.pos.distance(e.pos) < 20.)
            .collect();
        let others: Vec<_> = self
            .players
            .iter()
            .filter(|(i, q)| **i != id && p.pos.distance(q.pos) < 20.)
            .map(|(i, q)| serde_json::json!({"id":i,"pos":q.pos,"name":q.character.name}))
            .collect();
        Ok(
            serde_json::json!({"tick":self.tick,"time_ms":self.now,"player":id,"pos":p.pos,"character":p.character,"entities":entities,"players":others,"bullets":self.bullets.len(),"pending":p.pending}),
        )
    }
    pub fn step(&mut self) {
        self.now += 50;
        self.tick += 1;
        let ids: Vec<_> = self.players.keys().copied().collect();
        for id in &ids {
            let p = &self.players[id];
            if !p.character.alive || p.death_pending || p.departing {
                continue;
            }
            let p = self.players.get_mut(id).unwrap();
            let w = &self.content.objects[&WIZARD];
            if p.character.hp < w.hp {
                p.hp_fraction += (1. + w.vitality as f32 / 8.) * 0.05;
                let n = p.hp_fraction as i32;
                p.character.hp = (p.character.hp + n).min(w.hp);
                p.hp_fraction -= n as f32;
            } else {
                p.hp_fraction = 0.;
            }
            if p.character.mp < w.mp {
                p.mp_fraction += (0.6 + w.wisdom as f32 / 125.) * 0.05;
                let n = p.mp_fraction as i32;
                p.character.mp = (p.character.mp + n).min(w.mp);
                p.mp_fraction -= n as f32;
            } else {
                p.mp_fraction = 0.;
            }
            let goal = p.bot_goal;
            let aim = p.bot_aim;
            if let Some(goal) = goal {
                let d = p.pos.distance(goal);
                if d > 0.02 {
                    let step = (0.004
                        + (0.0096 - 0.004) * self.content.objects[&WIZARD].speed as f32 / 75.)
                        * 50.;
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
            if self.players[id].script_owned {
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
        for id in &ids {
            if let Err(e) = self.updates(*id, false) {
                tracing::error!(player=id,error=%e,"world update failed");
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
                .projectile
                .clone()
                .expect("validated tutorial gun projectile");
            let b = Bullet {
                owner: id,
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
                    || b.friendly
                        && self.entity_at(b.pos).is_some_and(|e| {
                            let o = &self.content.objects[&e.kind];
                            o.blocks && !o.destructible
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
                        let e = self.entities.get_mut(target).unwrap();
                        let damage = (b.damage - self.content.objects[&e.kind].defense)
                            .max(b.damage * 3 / 20);
                        e.hp -= damage;
                        let kill = e.hp <= 0;
                        let quest = e.kind == 0x6b1;
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
                            let e = self.entities.remove(target).unwrap();
                            self.cells
                                [e.pos.y as usize * self.map.width as usize + e.pos.x as usize] =
                                None;
                            tracing::info!(entity = target, owner = b.owner, "enemy defeated");
                        }
                    }
                } else {
                    for id in &ids {
                        let p = &self.players[id];
                        if p.character.alive
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
                                p.killer = self.content.objects[&self.entities[&b.owner].kind]
                                    .name
                                    .clone();
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
                let mut o = Writer::new();
                o.i32(p.tick).i32(100).i16(p.known.len() as i16);
                for entity in &p.known {
                    o.0.extend(self.stats(*entity).expect("visible entity must exist"));
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
mod tests {
    use super::*;
    use std::path::Path;
    fn world() -> World {
        let c = Arc::new(Content::load(Path::new("data")).unwrap());
        let m = Arc::new(Map::load(Path::new("data/tutorial.wmap"), &c).unwrap());
        World::new(c, m).unwrap()
    }
    fn ch(w: &World) -> Character {
        Character {
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
        }
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
                .contains("script-owned")
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
    fn failed_pickup_releases_both_owners() {
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
        assert!(!w.players[&id].pending && !w.entities[&bag.id].reserved);
        assert_eq!(w.entities[&bag.id].items[0], 0xa4f);
        assert_eq!(w.players[&id].character.items[3], -1);
    }
    #[test]
    fn paired_staff_wave() {
        let w = world();
        let spec = w.content.objects[&0xa97].projectile.clone().unwrap();
        let mut b = Bullet {
            owner: 1,
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
