use super::chain;
use crate::{
    content::{Content, Map},
    encounter::{Anchor, Motion, Profile, nr_speed},
};
use anyhow::{Result, ensure};
use rand::{Rng, SeedableRng, seq::SliceRandom};

pub const NAME: &str = "Tomb of the Ancients";
pub const TEMPLATE: &str = "tomb-of-the-ancients";
pub const PORTAL: u16 = 0x72c;
/// Nut, Geb and Bes in the selected client's display names.
pub const SUPPORT: u16 = 0xd26;
pub const ATTACKER: u16 = 0xd27;
pub const DEFENDER: u16 = 0xd28;
pub const BOSSES: [u16; 3] = [SUPPORT, ATTACKER, DEFENDER];
pub const STATUES: [u16; 3] = [0xd3d, 0xd3e, 0xd3f];
pub const ANCHOR: u16 = 0xd25;
pub const MUMMY: u16 = 0xd1a;
pub const SCARAB: u16 = 0xd1b;
pub const EAGLE: u16 = 0xd3a;
pub const PRIEST: u16 = 0xd2a;
pub const PRIESTESS: u16 = 0xd2b;
pub const INACTIVE: u16 = 0xd2c;
pub const ACTIVE: u16 = 0xd29;
pub const COWARDICE: u16 = 0xd40;
pub const WALL: u16 = 0x1dd;
/// Fire, thunder and frost obelisks: invincible, they spin a volley every so often.
pub const TURRETS: [u16; 3] = [0xd31, 0xd32, 0xd33];
pub const JACKAL: u16 = 0xd3b;
pub const LION: u16 = 0xd3c;
/// Sphinx 1-3, Nile 1-3, Pyramid 1-3; artifact N guards Bes, Geb and Nut respectively.
pub const ARTIFACTS: [u16; 9] = [
    0xd1c, 0xd1d, 0xd1e, 0xd1f, 0xd20, 0xd21, 0xd22, 0xd23, 0xd24,
];
pub const ROSTER: [u16; 28] = [
    SUPPORT, ATTACKER, DEFENDER, 0xd3d, 0xd3e, 0xd3f, ANCHOR, MUMMY, SCARAB, EAGLE, PRIEST,
    PRIESTESS, INACTIVE, ACTIVE, 0xd1c, 0xd1d, 0xd1e, 0xd1f, 0xd20, 0xd21, 0xd22, 0xd23, 0xd24,
    0xd31, 0xd32, 0xd33, JACKAL, LION,
];
/// Turret clock: 2.5 s asleep, then 2 s spinning and 5 s paused, forever.
pub fn turret_spin(elapsed: u64) -> Option<u64> {
    let t = elapsed.checked_sub(2500)? % 7000;
    (t < 2000).then_some(t)
}
const SANDSTONE: u8 = 0xe0;
const COVERED: u8 = 0xe2;
const MISSING: u8 = 0xe3;
const QUICKSAND: u8 = 0xbf;

pub fn artifact_guard(kind: u16) -> Option<u16> {
    ARTIFACTS
        .iter()
        .position(|k| *k == kind)
        .map(|i| [DEFENDER, ATTACKER, SUPPORT][i % 3])
}
pub fn boss_name(kind: u16) -> &'static str {
    match kind {
        SUPPORT => "Nut",
        ATTACKER => "Geb",
        _ => "Bes",
    }
}
/// HP fractions (per ten thousand) below which each later phase begins.
fn thresholds(kind: u16) -> [i64; 7] {
    match kind {
        SUPPORT => [9875, 9788, 9000, 7000, 5000, 3000, 600],
        ATTACKER => [9880, 9788, 7200, 6750, 4000, 2000, 600],
        _ => [9890, 9790, 7000, 5500, 4000, 2500, 600],
    }
}
pub fn stage(kind: u16, hp: i32, max: i32) -> u64 {
    thresholds(kind)
        .iter()
        .filter(|t| i64::from(hp) * 10000 < **t * i64::from(max))
        .count() as u64
}
pub fn shielded(kind: u16, stage: u64) -> bool {
    let second = if kind == SUPPORT { 2 } else { 3 };
    matches!(stage, 1 | 5 | 6 | 7) || stage == second
}
pub fn armored(kind: u16, stage: u64) -> bool {
    if kind == DEFENDER {
        stage != 4
    } else {
        stage == 0
    }
}
pub fn taunt(kind: u16, stage: u64) -> Option<&'static str> {
    let lines: [Option<&str>; 8] = match kind {
        SUPPORT => [
            Some("ENOUGH OF YOUR VANDALISM!"),
            Some("Impudence! I am an immortal, I needn't take you seriously."),
            None,
            Some("Bes, protect me at once!"),
            Some("Geb, eradicate these cretins from our tomb!"),
            Some("My artifacts shall make your lethargic lives end much more swiftly!"),
            None,
            Some("This cannot be! You shall not succeed!"),
        ],
        ATTACKER => [
            Some("ENOUGH OF YOUR VANDALISM!"),
            None,
            None,
            Some("Bes, protect me at once!"),
            Some("Nut, disable our foes!"),
            Some("My artifacts shall destroy you from your soul to your flesh!"),
            Some("My artifacts shall destroy you from your soul to your flesh!"),
            Some("This cannot be! You shall not succeed!"),
        ],
        _ => [
            Some("THIS WILL NOW BE YOUR TOMB!"),
            Some("Impudence! I am an Immortal, I needn't waste time on you!"),
            None,
            Some("Nut, disable our foes!"),
            Some("Geb, eradicate these cretins from our tomb!"),
            Some("Nut, let them wish they were dead!"),
            Some("My artifacts shall prove my wall of defense is impenetrable!"),
            Some("The end of your path is here!"),
        ],
    };
    lines[stage as usize]
}
/// Artifacts each boss keeps alive per phase (kind, maximum).
pub fn artifacts(kind: u16, stage: u64) -> &'static [(u16, usize)] {
    match (kind, stage) {
        (DEFENDER, 5) => &[(0xd22, 1), (0xd23, 1)],
        (DEFENDER, 6) => &[(0xd22, 2)],
        (DEFENDER, 7) => &[(0xd22, 1), (0xd23, 1), (0xd24, 1)],
        (SUPPORT, 5 | 7) => &[(0xd1c, 1)],
        (SUPPORT, 6) => &[(0xd1c, 2)],
        (ATTACKER, 5 | 7) => &[(0xd1f, 1)],
        (ATTACKER, 6) => &[(0xd1f, 2)],
        _ => &[],
    }
}
pub fn boss_motion(kind: u16, stage: u64) -> (Motion, f32) {
    let orbit = |radius: f32, rate| {
        (
            Motion::Orbit {
                anchor: Anchor::Kind(ANCHOR),
                radius,
            },
            nr_speed(rate),
        )
    };
    let chase = (
        Motion::Chase {
            sight: 20.,
            stop: 1.,
        },
        nr_speed(0.6),
    );
    match (kind, stage) {
        (ATTACKER, 7) => (
            Motion::Maintain {
                sight: 20.,
                distance: 6.,
            },
            nr_speed(0.5),
        ),
        (_, 7) => chase,
        (DEFENDER, 2 | 4) => orbit(5., 0.7),
        (DEFENDER, 3) => orbit(3., 0.6),
        (DEFENDER, 5 | 6) => orbit(7., 0.6),
        (SUPPORT, 2) => orbit(4., 0.7),
        (SUPPORT, 3) => orbit(6., 0.6),
        (SUPPORT, 4) => orbit(7., 0.6),
        (SUPPORT, 5) => orbit(4., 0.6),
        (ATTACKER, 5 | 6) => orbit(4., 0.6),
        _ => orbit(5., 0.6),
    }
}
pub fn profile(kind: u16) -> Option<Profile> {
    let chase = |sight, rate| {
        (
            Motion::Chase {
                sight,
                stop: 1.,
            },
            nr_speed(rate),
        )
    };
    let (motion, speed) = match kind {
        MUMMY => chase(12., 0.6),
        SCARAB => chase(7., 1.5),
        EAGLE => chase(12., 0.7),
        JACKAL => chase(10., 0.5),
        LION => chase(12., 0.4),
        PRIEST | PRIESTESS => (
            Motion::Orbit {
                anchor: Anchor::Kind(INACTIVE),
                radius: if kind == PRIEST { 6. } else { 9. },
            },
            nr_speed(if kind == PRIEST { 0.4 } else { 0.6 }),
        ),
        k if BOSSES.contains(&k) => boss_motion(k, 0),
        k if ARTIFACTS.contains(&k) => (
            Motion::Orbit {
                anchor: Anchor::Kind(artifact_guard(k)?),
                radius: 2.,
            },
            nr_speed(1.),
        ),
        ANCHOR | INACTIVE | ACTIVE | 0xd3d..=0xd3f | 0xd31..=0xd33 => (Motion::Stationary, 0.),
        _ => return None,
    };
    Some(Profile {
        motion,
        speed,
        interval: 1000,
        range: 12.,
    })
}
pub fn initial_condition(kind: u16) -> i32 {
    use crate::condition::{INVINCIBLE, INVULNERABLE, bit};
    match kind {
        ANCHOR | 0xd31..=0xd33 => bit(INVINCIBLE),
        INACTIVE | 0xd3d..=0xd3f => bit(INVULNERABLE),
        _ => 0,
    }
}

// The lost generator is reconstructed as a single chain of sandstone chambers: three
// treasure chambers guard a sarcophagus each, and the last chamber holds the sleeping statues.
pub fn generate(content: &Content, seed: u64) -> Result<Map> {
    for attempt in 0..64 {
        if let Some(map) = layout(seed ^ (attempt << 48)) {
            for cell in &map.cells {
                ensure!(
                    content.grounds.contains_key(&cell.ground),
                    "unknown Tomb ground"
                );
                if let Some(kind) = cell.object {
                    content.get(kind)?;
                }
            }
            tracing::info!(seed, attempt, map.width, map.height, "generated reconstructed Tomb");
            return Ok(map);
        }
    }
    anyhow::bail!("Tomb seed {seed}: no chain placement found")
}
fn layout(seed: u64) -> Option<Map> {
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let count = rng.gen_range(11..=13);
    let (rooms, corridors, mut map) = chain::chain(
        &mut rng,
        (17, 13),
        count,
        |rng, n| {
            if n == count {
                (33, 33)
            } else {
                (rng.gen_range(15..=21), rng.gen_range(15..=21))
            }
        },
        3,
    )?;
    let width = map.width;
    let index = |x: i32, y: i32| (y * width + x) as usize;
    let mut treasure: Vec<usize> = (2..count).collect();
    treasure.shuffle(&mut rng);
    treasure.truncate(3);
    for (number, room) in rooms.iter().enumerate() {
        let (cx, cy) = room.center();
        let boss = number == count;
        let mut sites = vec![];
        for y in room.y..room.y + room.h {
            for x in room.x..room.x + room.w {
                let c = &mut map.cells[index(x, y)];
                c.region = number as u8 + 2;
                c.ground = if rng.gen_ratio(1, 12) {
                    COVERED
                } else if rng.gen_ratio(1, 40) {
                    MISSING
                } else {
                    SANDSTONE
                };
                if (x - cx).abs() > 3 && (y - cy).abs() > 3 {
                    sites.push((x, y));
                }
            }
        }
        if number > 0 && !boss {
            for _ in 0..rng.gen_range(1..=3) {
                let (qx, qy) = (
                    rng.gen_range(room.x + 2..room.x + room.w - 2),
                    rng.gen_range(room.y + 2..room.y + room.h - 2),
                );
                let radius = rng.gen_range(2..=4);
                for y in room.y..room.y + room.h {
                    for x in room.x..room.x + room.w {
                        if (x - qx).pow(2) + (y - qy).pow(2) <= radius * radius
                            && (x - cx).abs().max((y - cy).abs()) > 1
                        {
                            map.cells[index(x, y)].ground = QUICKSAND;
                        }
                    }
                }
            }
        }
        sites.retain(|(x, y)| map.cells[index(*x, *y)].ground != QUICKSAND);
        sites.shuffle(&mut rng);
        let mut place = |kind: u16, x: i32, y: i32| {
            map.cells[index(x, y)].object = Some(kind);
        };
        if number == 0 {
            place(COWARDICE, cx, cy);
            map.cells[index(cx, cy)].region = 1;
            map.spawns.push((cx as f32 + 0.5, cy as f32 + 0.5));
        } else if boss {
            place(ANCHOR, cx, cy);
            place(STATUES[0], cx, cy - 8);
            place(STATUES[1], cx - 7, cy + 5);
            place(STATUES[2], cx + 7, cy + 5);
            place(TURRETS[2], cx - 12, cy - 12);
            place(TURRETS[0], cx + 12, cy - 12);
        } else if treasure.contains(&number) {
            place(INACTIVE, cx, cy);
            place(PRIEST, cx - 3, cy);
            place(PRIESTESS, cx + 3, cy);
            for _ in 0..2 {
                let (x, y) = sites.pop()?;
                place(SCARAB, x, y);
            }
        } else {
            let census = [
                (MUMMY, rng.gen_range(2..=4)),
                (SCARAB, rng.gen_range(2..=4)),
                (EAGLE, rng.gen_range(0..=2)),
                (JACKAL, rng.gen_range(0..=2)),
                (LION, rng.gen_range(0..=2)),
                (*TURRETS.choose(&mut rng)?, rng.gen_range(0..=2)),
            ];
            for (kind, n) in census {
                for _ in 0..n {
                    let (x, y) = sites.pop()?;
                    place(kind, x, y);
                }
            }
        }
    }
    chain::finish(&mut map, &corridors, SANDSTONE, WALL);
    Some(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn chambers_connect_spawn_treasure_and_statues() {
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
            assert_eq!(cell(spawn.0, spawn.1).object, Some(COWARDICE));
            assert_eq!(cell(spawn.0, spawn.1).region, 1);
            let count = |kind| map.cells.iter().filter(|c| c.object == Some(kind)).count();
            assert_eq!(count(ANCHOR), 1, "seed {seed}");
            assert_eq!(count(INACTIVE), 3, "seed {seed}");
            assert_eq!((count(PRIEST), count(PRIESTESS)), (3, 3), "seed {seed}");
            for statue in STATUES {
                assert_eq!(count(statue), 1, "seed {seed}");
            }
            assert!(count(MUMMY) >= 10 && count(WALL) > 100, "seed {seed}");
            let walkable = |x, y| {
                (0..map.width).contains(&x)
                    && (0..map.height).contains(&y)
                    && content.grounds[&cell(x, y).ground] > 0.
                    && cell(x, y)
                        .object
                        .is_none_or(|k| !content.objects[&k].blocks)
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
