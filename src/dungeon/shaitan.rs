use super::chain;
use crate::{
    content::{Content, Map},
    encounter::{Anchor, Motion, Profile, nr_speed},
};
use anyhow::{Result, ensure};
use rand::{Rng, SeedableRng, seq::SliceRandom};

pub const NAME: &str = "Lair of Shaitan";
pub const TEMPLATE: &str = "lair-of-shaitan";
// Custom content: art and XML come from pixel-art/inject.py (the selected 123.5.1 client has no Shaitan).
pub const HEAD: u16 = 0x6300;
pub const HAND_R: u16 = 0x6301;
pub const HAND_L: u16 = 0x6302;
pub const SMASH_R: u16 = 0x6303;
pub const SMASH_L: u16 = 0x6304;
pub const CREEPY_HANDS: u16 = 0x6305;
pub const CREEPY_HEAD: u16 = 0x6306;
/// Floats up when the head dies; untouchable for five seconds, then it pays out.
pub const BALLOON: u16 = 0x6307;
pub const PORTAL: u16 = 0x6310;
pub const WALL: u16 = 0x6311;
pub const FLOOR: u8 = 0x11;
pub const LAVA: u8 = 0x70;
pub const ROSTER: [u16; 8] = [
    HEAD,
    HAND_R,
    HAND_L,
    SMASH_R,
    SMASH_L,
    CREEPY_HANDS,
    CREEPY_HEAD,
    BALLOON,
];
const ENTRANCE: u16 = 0x703;
/// Head sweep stops, as offsets from its home (the reference MoveTo targets are map coordinates).
pub const SWEEP: [f32; 3] = [5., 0., -5.];

pub fn is_hand(kind: u16) -> bool {
    matches!(kind, HAND_R | HAND_L)
}
pub fn profile(kind: u16) -> Option<Profile> {
    let follow = |sight, stop, rate| {
        (
            Motion::Follow {
                sight,
                stop,
                idle: nr_speed(0.1),
                duration: 0,
                cooldown: 0,
            },
            nr_speed(rate),
            10.,
        )
    };
    let (motion, speed, range) = match kind {
        HEAD | BALLOON => (Motion::Stationary, 0., 30.),
        HAND_R | HAND_L => follow(40., 1., 0.8),
        SMASH_R | SMASH_L => follow(15., 0.5, 2.),
        CREEPY_HANDS => follow(10., 2., 0.5),
        CREEPY_HEAD => follow(15., 4., 0.2),
        _ => return None,
    };
    Some(Profile {
        motion,
        speed,
        interval: 1000,
        range,
    })
}
/// Hands guard the head from the start, unreachable until the fight begins.
pub fn initial_condition(kind: u16) -> i32 {
    use crate::condition::{INVINCIBLE, INVULNERABLE, bit};
    match kind {
        HEAD => bit(INVINCIBLE),
        HAND_R | HAND_L => bit(INVINCIBLE) | bit(INVULNERABLE),
        BALLOON => bit(INVULNERABLE),
        _ => 0,
    }
}
pub fn hand_motion(stage: Option<u64>) -> Motion {
    match stage {
        Some(3) => profile(HAND_R).expect("hand profile").motion,
        Some(4) => Motion::Protect {
            anchor: Anchor::Home,
            distance: 0.1,
        },
        _ => Motion::Stationary,
    }
}

// The lost generator is reconstructed as a few guarded halls with lava pools that lead to
// the arena where the head, the hands and four lava corners wait.
pub fn generate(content: &Content, seed: u64) -> Result<Map> {
    for attempt in 0..64 {
        if let Some(map) = layout(seed ^ (attempt << 48)) {
            for cell in &map.cells {
                ensure!(content.grounds.contains_key(&cell.ground), "unknown Lair ground");
                if let Some(kind) = cell.object {
                    content.get(kind)?;
                }
            }
            tracing::info!(seed, attempt, map.width, map.height, "generated reconstructed Lair of Shaitan");
            return Ok(map);
        }
    }
    anyhow::bail!("Lair of Shaitan seed {seed}: no chain placement found")
}
fn layout(seed: u64) -> Option<Map> {
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let halls = rng.gen_range(3..=4);
    let (rooms, corridors, mut map) = chain::chain(
        &mut rng,
        (16, 12),
        halls + 1,
        |rng, n| {
            if n == halls + 1 {
                (41, 41)
            } else {
                (rng.gen_range(14..=20), rng.gen_range(14..=20))
            }
        },
        4,
    )?;
    let width = map.width;
    let index = |x: i32, y: i32| (y * width + x) as usize;
    for (number, room) in rooms.iter().enumerate() {
        let (cx, cy) = room.center();
        let mut sites = vec![];
        for y in room.y..room.y + room.h {
            for x in room.x..room.x + room.w {
                let c = &mut map.cells[index(x, y)];
                c.region = number as u8 + 2;
                c.ground = FLOOR;
                if (x - cx).abs() > 2 && (y - cy).abs() > 2 {
                    sites.push((x, y));
                }
            }
        }
        let arena = number == halls + 1;
        for _ in 0..if arena { 0 } else { rng.gen_range(1..=2) } {
            let (qx, qy) = *sites.choose(&mut rng)?;
            for (x, y) in &sites {
                if (x - qx).pow(2) + (y - qy).pow(2) <= 4 {
                    map.cells[index(*x, *y)].ground = LAVA;
                }
            }
        }
        sites.retain(|(x, y)| map.cells[index(*x, *y)].ground == FLOOR);
        sites.shuffle(&mut rng);
        if number == 0 {
            map.cells[index(cx, cy)].object = Some(ENTRANCE);
            map.cells[index(cx, cy)].region = 1;
            map.spawns.push((cx as f32 + 0.5, cy as f32 + 0.5));
        } else if arena {
            // The head sits opposite the entrance corridor, the hands beside it.
            let (px, py) = rooms[number - 1].center();
            let (dx, dy) = (px - cx, py - cy);
            let (ux, uy) = if dx.abs() > dy.abs() {
                (dx.signum(), 0)
            } else {
                (0, dy.signum())
            };
            let at = |along: i32, across: i32| (cx - ux * along - uy * across, cy - uy * along + ux * across);
            for (kind, (x, y)) in [
                (HEAD, at(13, 0)),
                (HAND_R, at(9, 11)),
                (HAND_L, at(9, -11)),
            ] {
                map.cells[index(x, y)].object = Some(kind);
            }
            for (sx, sy) in [(-1, -1), (1, -1), (-1, 1), (1, 1)] {
                for y in -2..=2i32 {
                    for x in -2..=2i32 {
                        if x * x + y * y <= 5 {
                            map.cells[index(cx + sx * 16 + x, cy + sy * 16 + y)].ground = LAVA;
                        }
                    }
                }
            }
        } else {
            for (kind, n) in [
                (CREEPY_HANDS, rng.gen_range(3..=5)),
                (CREEPY_HEAD, rng.gen_range(0..=2)),
            ] {
                for _ in 0..n {
                    let (x, y) = sites.pop()?;
                    map.cells[index(x, y)].object = Some(kind);
                }
            }
        }
    }
    chain::finish(&mut map, &corridors, FLOOR, WALL);
    Some(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn halls_lead_to_an_arena_with_the_head_and_both_hands() {
        let content = Content::load(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/data"
        )))
        .unwrap();
        for seed in 0..24 {
            let map = generate(&content, seed).unwrap();
            let cell = |x: i32, y: i32| &map.cells[(y * map.width + x) as usize];
            let (sx, sy) = map.spawns[0];
            let spawn = (sx as i32, sy as i32);
            let count = |kind| map.cells.iter().filter(|c| c.object == Some(kind)).count();
            assert_eq!(
                (count(HEAD), count(HAND_R), count(HAND_L), count(ENTRANCE)),
                (1, 1, 1, 1),
                "seed {seed}"
            );
            assert!(count(CREEPY_HANDS) >= 9, "seed {seed}");
            let walkable = |x, y| {
                (0..map.width).contains(&x)
                    && (0..map.height).contains(&y)
                    && content.grounds[&cell(x, y).ground] > 0.
                    && cell(x, y)
                        .object
                        .is_none_or(|k| ROSTER.contains(&k) || !content.objects[&k].blocks)
            };
            let mut reached = BTreeSet::from([spawn]);
            let mut todo = vec![spawn];
            while let Some((x, y)) = todo.pop() {
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let p = (x + dx, y + dy);
                    if walkable(p.0, p.1) && reached.insert(p) {
                        todo.push(p);
                    }
                }
            }
            for (i, c) in map.cells.iter().enumerate() {
                let p = (i as i32 % map.width, i as i32 / map.width);
                if c.object.is_some_and(|k| ROSTER.contains(&k)) {
                    assert!(reached.contains(&p), "seed {seed}: unreachable {:#x}", c.object.unwrap());
                }
            }
        }
    }
}
