use super::chain;
use crate::{
    content::{Content, Map},
    encounter::{Anchor, Motion, Profile, nr_speed},
};
use anyhow::{Result, ensure};
use rand::{
    Rng, SeedableRng,
    seq::{IteratorRandom, SliceRandom},
};

pub const NAME: &str = "Mad Lab";
pub const TEMPLATE: &str = "mad-lab";
pub const PORTAL: u16 = 0x890;
pub const BOSS: u16 = 0x976;
pub const POTION: u16 = 0x978;
pub const TURRET: u16 = 0x12aa;
pub const MINI_BOT: u16 = 0x979;
pub const CYBORG: u16 = 0x980;
pub const EXPERIMENT: u16 = 0x981;
pub const ENFORCER: u16 = 0x982;
pub const CRUSHER: u16 = 0x983;
pub const CAGE: u16 = 0x892;
/// Upper-left, upper-right, lower-left and lower-right gas emitters.
pub const GAS: [u16; 4] = [0x993, 0x994, 0x995, 0x996];
/// Caged variants the doctor releases while hiding: experiment, cyborg, mini bot.
pub const CAGED: [u16; 3] = [0x1e23, 0x1e24, 0x1e25];
pub const TABLE: u16 = 0x188b;
pub const VAT: u16 = 0x189a;
pub const SEAL: u16 = 0x18c3;
pub const WALL: u16 = 0x188c;
pub const BONES: u16 = 0x18a3;
pub const TESLA: u16 = 0x188a;
pub const BUBBLE: u16 = 0x1e22;
/// Map region of the gate walls that fall once every Tesla Coil is destroyed.
pub const GATE: u8 = 250;
pub const FLOOR: u8 = 0xd3;
pub const ROSTER: [u16; 21] = [
    BOSS, POTION, TURRET, MINI_BOT, CYBORG, EXPERIMENT, ENFORCER, CRUSHER, CAGE, 0x993, 0x994,
    0x995, 0x996, 0x1e23, 0x1e24, 0x1e25, TABLE, VAT, SEAL, TESLA, BUBBLE,
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Act {
    Potions,
    Turrets,
    Hide,
}
/// The doctor's script: four rounds of potions (12 s), turrets (10 s) and hiding (15 s);
/// each round leaves a different gas emitter off. Below 20% HP he only alternates throws.
pub fn act(elapsed: u64, rage: bool) -> (u64, Act, [bool; 4]) {
    if rage {
        let t = elapsed % 22000;
        let act = if t < 12000 { Act::Potions } else { Act::Turrets };
        return (1000 + elapsed / 22000 * 2 + u64::from(act == Act::Turrets), act, [false; 4]);
    }
    let (round, t) = (elapsed / 37000, elapsed % 37000);
    let (index, act) = match t {
        0..12000 => (0, Act::Potions),
        12000..22000 => (1, Act::Turrets),
        _ => (2, Act::Hide),
    };
    let mut gas = [act != Act::Hide; 4];
    gas[(round % 4) as usize] = false;
    (round * 3 + index, act, gas)
}
pub fn crusher_stage(hp: i32, max: i32) -> u64 {
    [75, 50, 25]
        .iter()
        .filter(|pct| i64::from(hp) * 100 < i64::from(max) * **pct)
        .count() as u64
}
pub fn profile(kind: u16) -> Option<Profile> {
    let wander = (Motion::WanderHome { radius: 4. }, nr_speed(0.5));
    let (motion, speed) = match kind {
        BOSS | MINI_BOT | EXPERIMENT | ENFORCER | CRUSHER | 0x1e23 | 0x1e25 => wander,
        CYBORG | 0x1e24 => (
            Motion::Chase {
                sight: 10.,
                stop: 1.,
            },
            nr_speed(0.6),
        ),
        POTION | TURRET | CAGE | TABLE | VAT | SEAL | TESLA | BUBBLE | 0x993..=0x996 => {
            (Motion::Stationary, 0.)
        }
        _ => return None,
    };
    Some(Profile {
        motion,
        speed,
        interval: 1000,
        range: 10.,
    })
}
pub fn boss_motion(act: Act) -> (Motion, f32) {
    if act == Act::Hide {
        (
            Motion::Protect {
                anchor: Anchor::Home,
                distance: 0.1,
            },
            nr_speed(1.),
        )
    } else {
        (Motion::WanderHome { radius: 4. }, nr_speed(0.5))
    }
}
pub fn initial_condition(kind: u16) -> i32 {
    use crate::condition::{INVINCIBLE, bit};
    if matches!(kind, 0x993..=0x996 | CAGE | BUBBLE) {
        bit(INVINCIBLE)
    } else {
        0
    }
}

// The lost generator is reconstructed as a chain of laboratory halls, some sealed by a
// breakable wall, ending in the doctor's arena with its four gas emitters and monster cage.
pub fn generate(content: &Content, seed: u64) -> Result<Map> {
    for attempt in 0..64 {
        if let Some(map) = layout(seed ^ (attempt << 48)) {
            for cell in &map.cells {
                ensure!(content.grounds.contains_key(&cell.ground), "unknown Lab ground");
                if let Some(kind) = cell.object {
                    content.get(kind)?;
                }
            }
            tracing::info!(seed, attempt, map.width, map.height, "generated reconstructed Mad Lab");
            return Ok(map);
        }
    }
    anyhow::bail!("Mad Lab seed {seed}: no chain placement found")
}
fn layout(seed: u64) -> Option<Map> {
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let count = rng.gen_range(8..=10);
    let (rooms, corridors, mut map) = chain::chain(
        &mut rng,
        (16, 12),
        count,
        |rng, n| {
            if n == count {
                (27, 39)
            } else {
                (rng.gen_range(14..=22), rng.gen_range(14..=22))
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
        sites.shuffle(&mut rng);
        let mut place = |kind: u16, x: i32, y: i32| {
            map.cells[index(x, y)].object = Some(kind);
        };
        if number == 0 {
            place(0x703, cx, cy);
            map.cells[index(cx, cy)].region = 1;
            map.spawns.push((cx as f32 + 0.5, cy as f32 + 0.5));
        } else if number == count {
            place(BOSS, cx, cy - 8);
            place(CAGE, cx, cy - 14);
            for (gas, (dx, dy)) in GAS.iter().zip([(-11, -17), (11, -17), (-11, 17), (11, 17)]) {
                place(*gas, cx + dx, cy + dy);
            }
            for dx in [-9, 9] {
                place(VAT, cx + dx, cy + 4);
                place(TABLE, cx + dx, cy - 4);
            }
        } else {
            if number == count - 1 {
                for _ in 0..3 {
                    let (x, y) = sites.pop()?;
                    place(TESLA, x, y);
                }
            }
            let census = [
                (EXPERIMENT, rng.gen_range(0..=1)),
                (ENFORCER, rng.gen_range(0..=1)),
                (CRUSHER, rng.gen_range(0..=1)),
                (MINI_BOT, rng.gen_range(2..=4)),
                (CYBORG, rng.gen_range(1..=3)),
                (TABLE, rng.gen_range(1..=3)),
                (BONES, rng.gen_range(0..=2)),
            ];
            for (kind, n) in census {
                for _ in 0..n {
                    let (x, y) = sites.pop()?;
                    place(kind, x, y);
                }
            }
        }
    }
    chain::finish(&mut map, &corridors, FLOOR, WALL);
    // The last corridor is gated by walls that only fall with the Tesla Coils.
    let gate = corridors[count - 1];
    for y in gate.y..gate.y + gate.h {
        for x in gate.x..gate.x + gate.w {
            if (gate.w >= gate.h && x == gate.x + gate.w / 2) || (gate.h > gate.w && y == gate.y + gate.h / 2) {
                let c = &mut map.cells[index(x, y)];
                c.object = Some(WALL);
                c.region = GATE;
            }
        }
    }
    // Two earlier corridors are sealed; the breakable wall is the Lab's own destructible wall.
    for c in corridors[..count - 1].iter().skip(1).choose_multiple(&mut rng, 2) {
        for y in c.y..c.y + c.h {
            for x in c.x..c.x + c.w {
                if (c.w >= c.h && x == c.x + c.w / 2) || (c.h > c.w && y == c.y + c.h / 2) {
                    map.cells[index(x, y)].object = Some(SEAL);
                }
            }
        }
    }
    Some(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn halls_connect_and_seals_can_be_broken_through() {
        let content = Content::load(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/data"
        )))
        .unwrap();
        assert!(content.objects[&SEAL].blocks && content.objects[&SEAL].enemy);
        for seed in 0..24 {
            let map = generate(&content, seed).unwrap();
            let cell = |x: i32, y: i32| &map.cells[(y * map.width + x) as usize];
            let (sx, sy) = map.spawns[0];
            let spawn = (sx as i32, sy as i32);
            let count = |kind| map.cells.iter().filter(|c| c.object == Some(kind)).count();
            assert_eq!((count(BOSS), count(CAGE)), (1, 1), "seed {seed}");
            for gas in GAS {
                assert_eq!(count(gas), 1, "seed {seed}");
            }
            assert!(count(SEAL) >= 8 && count(MINI_BOT) >= 14, "seed {seed}");
            assert_eq!(count(TESLA), 3, "seed {seed}");
            assert!(map.cells.iter().filter(|c| c.region == GATE && c.object == Some(WALL)).count() >= 4);
            let walkable = |x, y| {
                (0..map.width).contains(&x)
                    && (0..map.height).contains(&y)
                    && content.grounds[&cell(x, y).ground] > 0.
                    && (cell(x, y).region == GATE
                        || cell(x, y)
                            .object
                            .is_none_or(|k| ROSTER.contains(&k) || !content.objects[&k].blocks))
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
