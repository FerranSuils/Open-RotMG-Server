use super::*;

#[derive(Debug, Serialize)]
pub struct Session {
    pub world: i32,
    pub player: i32,
    pub account: i32,
    pub character: i32,
}

#[derive(Serialize)]
pub struct PlayerView {
    pub session: Session,
    pub tick: u64,
    pub time_ms: u64,
    pub position: Pos,
    pub map: MapInfo,
    pub character: OwnCharacter,
    pub lifecycle: Lifecycle,
    pub tiles: Vec<Tile>,
    pub objects: Vec<VisibleObject>,
    pub projectiles: Vec<VisibleProjectile>,
}
#[derive(Serialize)]
pub struct MapInfo {
    pub name: String,
    pub width: i32,
    pub height: i32,
}
#[derive(Serialize)]
pub struct OwnCharacter {
    pub class: u16,
    pub level: i32,
    pub experience: i32,
    pub hp: i32,
    pub mp: i32,
    pub stats: [i32; 8],
    pub items: Vec<i32>,
    pub inventory_revision: i32,
    pub condition_mask: i32,
}
#[derive(Serialize)]
pub struct Lifecycle {
    pub alive: bool,
    pub death_pending: bool,
    pub departing: bool,
    pub busy: bool,
    pub quarantined: bool,
    pub output: OutputState,
}
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputState {
    Detached,
    Live,
    Lost,
}
#[derive(Serialize)]
pub struct Tile {
    pub x: i16,
    pub y: i16,
    pub ground: u8,
    pub ground_speed: f32,
}
#[derive(Serialize)]
pub struct VisibleObject {
    pub id: i32,
    pub kind: u16,
    pub name: String,
    pub position: Pos,
    pub hp: i32,
    pub equipped: Vec<i32>,
    pub contents: Vec<i32>,
    pub blocks: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition_mask: Option<i32>,
}
#[derive(Serialize)]
pub struct VisibleProjectile {
    pub owner: i32,
    pub id: u8,
    pub start: Pos,
    pub position: Pos,
    pub angle: f32,
    pub speed: f32,
    pub lifetime_ms: f32,
    pub amplitude: f32,
    pub frequency: f32,
    pub passes_cover: bool,
}

impl Directory {
    pub fn player_view(&self, id: i32) -> Result<PlayerView> {
        let scope = self.location(id)?;
        let w = &self.worlds[&scope];
        let p = &w.players[&id];
        let visible = w.visible(id);
        let objects = visible
            .intersection(&p.known)
            .filter(|entity| **entity != id)
            .map(|entity| {
                if let Some(q) = w.players.get(entity) {
                    VisibleObject {
                        id: *entity,
                        kind: q.character.class,
                        name: q.character.name.clone(),
                        position: q.pos,
                        hp: q.character.hp,
                        equipped: q.character.items[..4].to_vec(),
                        contents: vec![],
                        blocks: false,
                        condition_mask: None,
                    }
                } else {
                    let e = &w.entities[entity];
                    VisibleObject {
                        id: *entity,
                        kind: e.kind,
                        name: e.name.clone(),
                        position: e.pos,
                        hp: e.hp,
                        equipped: vec![],
                        contents: e.items.clone(),
                        blocks: w.content.objects[&e.kind].blocks,
                        condition_mask: Some(e.conditions(w.now)),
                    }
                }
            })
            .collect();
        let tiles = p
            .tiles
            .iter()
            .map(|(x, y)| {
                let cell = &w.map.cells[*y as usize * w.map.width as usize + *x as usize];
                Tile {
                    x: *x,
                    y: *y,
                    ground: cell.ground,
                    ground_speed: w.content.grounds[&cell.ground],
                }
            })
            .collect();
        // A newly discovered shooter does not reveal its earlier, unreceived shots.
        let projectiles = w
            .bullets
            .iter()
            .filter(|b| {
                b.pos.distance(p.pos) < 20.
                    && p.known_since
                        .get(&b.owner)
                        .is_some_and(|since| b.born > *since)
            })
            .map(|b| VisibleProjectile {
                owner: b.owner,
                id: b.id,
                start: b.start,
                position: b.pos,
                angle: b.angle,
                speed: b.spec.speed,
                lifetime_ms: b.spec.lifetime,
                amplitude: b.spec.amplitude,
                frequency: b.spec.frequency,
                passes_cover: b.spec.passes_cover,
            })
            .collect();
        Ok(PlayerView {
            session: Session {
                world: scope,
                player: id,
                account: p.character.account,
                character: p.character.id,
            },
            tick: w.tick,
            time_ms: w.now,
            position: p.pos,
            map: MapInfo {
                name: w.name.clone(),
                width: w.map.width,
                height: w.map.height,
            },
            character: OwnCharacter {
                class: p.character.class,
                level: p.character.level,
                experience: p.character.exp,
                hp: p.character.hp,
                mp: p.character.mp,
                stats: p.character.effective(&w.content)?,
                items: p.character.items.clone(),
                inventory_revision: p.character.revision,
                condition_mask: p.conditions(w.now) | if p.paused { 1 << 20 } else { 0 },
            },
            lifecycle: Lifecycle {
                alive: p.character.alive,
                death_pending: p.death_pending,
                departing: p.departing,
                busy: p.pending,
                quarantined: p.persistence_failure.is_some(),
                output: match &p.tx {
                    None => OutputState::Detached,
                    Some(tx) if *tx.lost.borrow() || tx.packets.is_closed() => OutputState::Lost,
                    Some(_) => OutputState::Live,
                },
            },
            tiles,
            objects,
            projectiles,
        })
    }
}
