//! Rooms attached one after another through straight corridors, shared by the
//! reconstructed Tomb and Mad Lab generators.
use crate::content::{Cell, Map};
use rand::{Rng, rngs::StdRng, seq::SliceRandom};

#[derive(Clone, Copy)]
pub struct Room {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}
impl Room {
    pub fn center(self) -> (i32, i32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }
    fn clear_of(self, b: Self) -> bool {
        self.x + self.w + 2 <= b.x
            || b.x + b.w + 2 <= self.x
            || self.y + self.h + 2 <= b.y
            || b.y + b.h + 2 <= self.y
    }
}

/// Chains `count` rooms after a first room of `first` size; `size` names each later room's
/// dimensions and `width` the corridor width. Returns rooms, corridors and a void map
/// (ground 0xfe) with a two-cell margin, or None when a random direction is boxed in.
pub fn chain(
    rng: &mut StdRng,
    first: (i32, i32),
    count: usize,
    mut size: impl FnMut(&mut StdRng, usize) -> (i32, i32),
    width: i32,
) -> Option<(Vec<Room>, Vec<Room>, Map)> {
    let mut rooms = vec![Room {
        x: 0,
        y: 0,
        w: first.0,
        h: first.1,
    }];
    let mut corridors = vec![];
    for number in 1..=count {
        let (w, h) = size(rng, number);
        let p = rooms[number - 1];
        let (cx, cy) = p.center();
        let mut sides = [0, 1, 2, 3];
        sides.shuffle(rng);
        let gap = rng.gen_range(4..=7);
        let (room, corridor) = sides.into_iter().find_map(|side| {
            let (r, c) = match side {
                0 => (
                    Room {
                        x: p.x + p.w + gap,
                        y: cy - h / 2,
                        w,
                        h,
                    },
                    Room {
                        x: p.x + p.w,
                        y: cy - width / 2,
                        w: gap,
                        h: width,
                    },
                ),
                1 => (
                    Room {
                        x: p.x - gap - w,
                        y: cy - h / 2,
                        w,
                        h,
                    },
                    Room {
                        x: p.x - gap,
                        y: cy - width / 2,
                        w: gap,
                        h: width,
                    },
                ),
                2 => (
                    Room {
                        x: cx - w / 2,
                        y: p.y + p.h + gap,
                        w,
                        h,
                    },
                    Room {
                        x: cx - width / 2,
                        y: p.y + p.h,
                        w: width,
                        h: gap,
                    },
                ),
                _ => (
                    Room {
                        x: cx - w / 2,
                        y: p.y - gap - h,
                        w,
                        h,
                    },
                    Room {
                        x: cx - width / 2,
                        y: p.y - gap,
                        w: width,
                        h: gap,
                    },
                ),
            };
            rooms.iter().all(|o| r.clear_of(*o)).then_some((r, c))
        })?;
        rooms.push(room);
        corridors.push(corridor);
    }
    let min_x = rooms.iter().map(|r| r.x).min()? - 2;
    let min_y = rooms.iter().map(|r| r.y).min()? - 2;
    for room in rooms.iter_mut().chain(&mut corridors) {
        room.x -= min_x;
        room.y -= min_y;
    }
    let width = rooms.iter().map(|r| r.x + r.w).max()? + 2;
    let height = rooms.iter().map(|r| r.y + r.h).max()? + 2;
    let map = Map {
        width,
        height,
        cells: vec![
            Cell {
                ground: 0xfe,
                object: None,
                config: String::new(),
                terrain: 0,
                region: 0,
            };
            (width * height) as usize
        ],
        spawns: vec![],
    };
    Some((rooms, corridors, map))
}

/// Floors every corridor cell and rings all floor with `wall` objects on void cells.
pub fn finish(map: &mut Map, corridors: &[Room], floor: u8, wall: u16) {
    let width = map.width;
    for c in corridors {
        for y in c.y..c.y + c.h {
            for x in c.x..c.x + c.w {
                let cell = &mut map.cells[(y * width + x) as usize];
                cell.ground = floor;
                cell.object = None;
            }
        }
    }
    for y in 1..map.height - 1 {
        for x in 1..width - 1 {
            let i = (y * width + x) as usize;
            if map.cells[i].ground == 0xfe
                && (-1..=1).any(|dy| {
                    (-1..=1).any(|dx| map.cells[((y + dy) * width + x + dx) as usize].ground != 0xfe)
                })
            {
                map.cells[i].object = Some(wall);
            }
        }
    }
}
