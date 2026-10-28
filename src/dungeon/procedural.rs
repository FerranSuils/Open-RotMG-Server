//! Themed procedural dungeons (custom content from pixel-art/inject.py): rooms joined by
//! two-wide corridors, some sealed by breakable walls of at most MAX_BREAKABLE_RUN in a row.
use crate::{
    content::{Cell, Content, Map},
    encounter::{Motion, Profile, ring, spread},
    loot::{ABILITIES, ARMOR, Pick, RINGS, WEAPONS},
};
use anyhow::{Context, Result, ensure};
use rand::{Rng, SeedableRng, seq::SliceRandom};
use std::collections::VecDeque;

mod roster;
pub use roster::{PORTAL_DROPS, ROSTER, THEMES};

pub const MAX_BREAKABLE_RUN: usize = 4;
const VOID: u8 = 0xfe;
const ENTRANCE: u16 = 0x703;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Arch {
    Melee,
    Shot,
    Spread,
    Ring,
    Boss,
}
pub struct Theme {
    pub template: &'static str,
    pub name: &'static str,
    pub portal: u16,
    pub boss: u16,
    pub mobs: [u16; 4],
    pub wall: u16,
    pub breakable: u16,
    pub deco: u16,
    pub floor: u8,
    pub floor2: u8,
    /// A perfect labyrinth of four-wide passages instead of rooms and corridors.
    pub maze: bool,
    /// The selected client already defines this portal / floor: inject.py emits no copy.
    pub vanilla_portal: bool,
    pub vanilla_floor: bool,
}

fn entry(kind: u16) -> Option<(Arch, usize)> {
    ROSTER
        .iter()
        .find(|(k, ..)| *k == kind)
        .map(|&(_, arch, loot)| (arch, loot))
}
pub fn theme(template: &str) -> Option<&'static Theme> {
    THEMES.iter().find(|t| t.template == template)
}
pub fn is_boss(kind: u16) -> bool {
    entry(kind).is_some_and(|(arch, _)| arch == Arch::Boss)
}
pub fn portal_drop(enemy: u16) -> Option<(u16, f64)> {
    PORTAL_DROPS
        .iter()
        .find(|(kind, ..)| *kind == enemy)
        .map(|&(_, theme, chance)| (THEMES[theme].portal, chance))
}
// Attack ranges match the projectile speed x lifetime written by inject.py.
pub fn profile(kind: u16) -> Option<Profile> {
    let (motion, speed, interval, range) = match entry(kind)?.0 {
        Arch::Melee => (
            Motion::Chase {
                sight: 9.,
                stop: 0.5,
            },
            10.,
            700,
            2.,
        ),
        Arch::Shot => (
            Motion::Chase {
                sight: 10.,
                stop: 4.,
            },
            8.,
            1000,
            8.,
        ),
        Arch::Spread => (
            Motion::Chase {
                sight: 10.,
                stop: 5.,
            },
            7.,
            1400,
            7.,
        ),
        Arch::Ring => (Motion::Wander, 5., 1600, 6.),
        Arch::Boss => (
            Motion::Chase {
                sight: 14.,
                stop: 5.,
            },
            5.,
            500,
            10.,
        ),
    };
    Some(Profile {
        motion,
        speed,
        interval,
        range,
    })
}
pub fn attacks(kind: u16, heading: f32) -> Option<Vec<(u8, f32)>> {
    Some(match entry(kind)?.0 {
        Arch::Melee | Arch::Shot => vec![(0, heading)],
        Arch::Spread => spread(0, 3, 16., heading),
        Arch::Ring => ring(0, 8, heading),
        // Bosses run bullet-hell phases through encounter::patterns (attack schedules).
        Arch::Boss => return None,
    })
}
const LOOT: [&[(f64, Pick)]; 10] = {
    use Pick::Tier;
    [
        &[(0.03, Tier(1, WEAPONS)), (0.03, Tier(1, ARMOR))],
        &[
            (0.03, Tier(2, WEAPONS)),
            (0.03, Tier(2, ARMOR)),
            (0.02, Tier(1, RINGS)),
        ],
        &[
            (0.03, Tier(4, WEAPONS)),
            (0.03, Tier(4, ARMOR)),
            (0.02, Tier(1, ABILITIES)),
        ],
        &[
            (0.03, Tier(6, WEAPONS)),
            (0.03, Tier(6, ARMOR)),
            (0.02, Tier(2, ABILITIES)),
            (0.02, Tier(1, RINGS)),
        ],
        &[
            (0.03, Tier(7, WEAPONS)),
            (0.01, Tier(8, WEAPONS)),
            (0.03, Tier(8, ARMOR)),
            (0.02, Tier(3, ABILITIES)),
        ],
        &[
            (0.04, Tier(5, WEAPONS)),
            (0.03, Tier(6, ARMOR)),
            (0.03, Tier(2, ABILITIES)),
        ],
        &[
            (0.04, Tier(7, WEAPONS)),
            (0.04, Tier(7, ARMOR)),
            (0.03, Tier(3, ABILITIES)),
            (0.02, Tier(2, RINGS)),
        ],
        &[
            (0.04, Tier(8, WEAPONS)),
            (0.04, Tier(8, ARMOR)),
            (0.03, Tier(3, RINGS)),
        ],
        &[
            (0.3, Tier(8, WEAPONS)),
            (0.15, Tier(9, WEAPONS)),
            (0.3, Tier(8, ARMOR)),
            (0.1, Tier(4, ABILITIES)),
            (0.15, Tier(3, RINGS)),
        ],
        &[
            (0.3, Tier(9, WEAPONS)),
            (0.15, Tier(10, WEAPONS)),
            (0.05, Tier(11, WEAPONS)),
            (0.3, Tier(9, ARMOR)),
            (0.15, Tier(10, ARMOR)),
            (0.15, Tier(4, ABILITIES)),
            (0.15, Tier(4, RINGS)),
        ],
    ]
};
pub fn loot(kind: u16) -> Option<&'static [(f64, Pick)]> {
    if THEMES.iter().any(|t| t.breakable == kind) {
        return Some(&[]);
    }
    Some(LOOT[entry(kind)?.1])
}

#[derive(Clone, Copy)]
struct Room {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}
impl Room {
    fn center(self) -> (i32, i32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }
}

pub fn generate(content: &Content, theme: &Theme, seed: u64) -> Result<Map> {
    if theme.maze {
        return maze(content, theme, seed);
    }
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let (w, h) = (72, 72);
    let idx = |x: i32, y: i32| (y * w + x) as usize;
    let mut rooms: Vec<Room> = vec![];
    for _ in 0..600 {
        if rooms.len() == 14 {
            break;
        }
        let (rw, rh) = (rng.gen_range(7..=12), rng.gen_range(7..=12));
        let r = Room {
            x: rng.gen_range(3..w - rw - 3),
            y: rng.gen_range(3..h - rh - 3),
            w: rw,
            h: rh,
        };
        if !rooms.iter().any(|o| {
            r.x < o.x + o.w + 4 && o.x < r.x + r.w + 4 && r.y < o.y + o.h + 4 && o.y < r.y + r.h + 4
        }) {
            rooms.push(r);
        }
    }
    ensure!(
        rooms.len() >= 6,
        "{} seed {seed}: only {} rooms",
        theme.name,
        rooms.len()
    );
    let mut region = vec![0u8; (w * h) as usize];
    let mut carved = vec![0u8; (w * h) as usize]; // corridor ownership count outside rooms
    for (n, r) in rooms.iter().enumerate() {
        for y in r.y..r.y + r.h {
            for x in r.x..r.x + r.w {
                region[idx(x, y)] = n as u8 + 2; // region 1 marks the spawn
            }
        }
    }
    // Minimum spanning tree keeps every room connected; extra short links become shortcuts.
    let d2 = |a: Room, b: Room| {
        let ((ax, ay), (bx, by)) = (a.center(), b.center());
        (ax - bx).pow(2) + (ay - by).pow(2)
    };
    let mut links = vec![];
    let mut joined = vec![0];
    while joined.len() < rooms.len() {
        let (a, b) = joined
            .iter()
            .flat_map(|&a| {
                (0..rooms.len())
                    .filter(|b| !joined.contains(b))
                    .map(move |b| (a, b))
            })
            .min_by_key(|&(a, b)| d2(rooms[a], rooms[b]))
            .unwrap();
        links.push((a, b, false));
        joined.push(b);
    }
    for a in 0..rooms.len() {
        for b in a + 1..rooms.len() {
            if d2(rooms[a], rooms[b]) < 500
                && !links
                    .iter()
                    .any(|&(x, y, _)| (x, y) == (a, b) || (x, y) == (b, a))
                && rng.gen_bool(0.25)
            {
                links.push((a, b, true));
            }
        }
    }
    // Each corridor is two L-shaped legs; a leg is its centre line plus a perpendicular partner cell.
    let mut corridors: Vec<(bool, Vec<(bool, Vec<(i32, i32)>)>)> = vec![];
    for &(a, b, shortcut) in &links {
        let ((ax, ay), (bx, by)) = (rooms[a].center(), rooms[b].center());
        let corner = if rng.gen_bool(0.5) {
            (bx, ay)
        } else {
            (ax, by)
        };
        let mut legs = vec![];
        for ((x0, y0), (x1, y1)) in [((ax, ay), corner), (corner, (bx, by))] {
            let horizontal = y0 == y1;
            let steps = (x1 - x0).abs().max((y1 - y0).abs());
            let cells: Vec<_> = (0..=steps)
                .map(|s| (x0 + (x1 - x0).signum() * s, y0 + (y1 - y0).signum() * s))
                .collect();
            for &(x, y) in &cells {
                for (cx, cy) in [(x, y), if horizontal { (x, y + 1) } else { (x + 1, y) }] {
                    if region[idx(cx, cy)] == 0 {
                        carved[idx(cx, cy)] += 1;
                    }
                }
            }
            legs.push((horizontal, cells));
        }
        corridors.push((shortcut, legs));
    }
    let mut map = Map {
        width: w,
        height: h,
        cells: vec![
            Cell {
                ground: VOID,
                object: None,
                config: String::new(),
                terrain: 0,
                region: 0
            };
            (w * h) as usize
        ],
        spawns: vec![],
    };
    for i in 0..(w * h) as usize {
        if region[i] > 0 || carved[i] > 0 {
            map.cells[i].ground = if rng.gen_bool(0.18) {
                theme.floor2
            } else {
                theme.floor
            };
            map.cells[i].region = region[i];
        }
    }
    let floor = |m: &Map, x: i32, y: i32| m.cells[idx(x, y)].ground != VOID;
    // Seal shortcuts (always) and some main corridors with 1..=4 breakable walls, only on
    // stretches owned by a single corridor so a seal can never touch another passage.
    // A second pass forces one seal when chance left the whole dungeon without any.
    let mut sealed = vec![false; corridors.len()];
    for (pass, n) in [0, 1]
        .into_iter()
        .flat_map(|pass| (0..corridors.len()).map(move |n| (pass, n)))
    {
        let (shortcut, legs) = &corridors[n];
        if sealed[n]
            || pass == 1 && sealed.contains(&true)
            || pass == 0 && !shortcut && !rng.gen_bool(0.35)
        {
            continue;
        }
        let run = rng.gen_range(1..=MAX_BREAKABLE_RUN as i32);
        let mut options = vec![];
        for (horizontal, cells) in legs {
            let (px, py) = if *horizontal { (0, 1) } else { (1, 0) };
            for s in 1..cells.len().saturating_sub(run as usize + 1) {
                let span = &cells[s - 1..s + run as usize + 1]; // one free cell either side
                let exclusive = span.iter().all(|&(x, y)| {
                    [(x, y), (x + px, y + py)]
                        .iter()
                        .all(|&(cx, cy)| region[idx(cx, cy)] == 0 && carved[idx(cx, cy)] == 1)
                        && !floor(&map, x - px, y - py)
                        && !floor(&map, x + 2 * px, y + 2 * py)
                });
                if exclusive {
                    options.push((s, *horizontal, cells));
                }
            }
        }
        if let Some(&(s, horizontal, cells)) = options.choose(&mut rng) {
            for &(x, y) in &cells[s..s + run as usize] {
                map.cells[idx(x, y)].object = Some(theme.breakable);
                let (px, py) = if horizontal { (x, y + 1) } else { (x + 1, y) };
                map.cells[idx(px, py)].object = Some(theme.breakable);
            }
            sealed[n] = true;
        }
    }
    let seals = sealed.iter().filter(|s| **s).count();
    ensure!(
        seals > 0,
        "{} seed {seed}: no corridor can hold a breakable seal",
        theme.name
    );
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            if !floor(&map, x, y)
                && (-1..=1).any(|dy| (-1..=1).any(|dx| floor(&map, x + dx, y + dy)))
            {
                map.cells[idx(x, y)].object = Some(theme.wall);
            }
        }
    }
    // Difficulty grows with walking distance (breaking seals allowed) from the entrance room.
    let (sx, sy) = rooms[0].center();
    let mut dist = vec![-1i32; (w * h) as usize];
    let mut queue = VecDeque::from([(sx, sy)]);
    dist[idx(sx, sy)] = 0;
    while let Some((x, y)) = queue.pop_front() {
        for (nx, ny) in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
            if floor(&map, nx, ny) && dist[idx(nx, ny)] < 0 {
                dist[idx(nx, ny)] = dist[idx(x, y)] + 1;
                queue.push_back((nx, ny));
            }
        }
    }
    let far = |r: Room| dist[idx(r.center().0, r.center().1)];
    let boss_room = (1..rooms.len()).max_by_key(|&i| far(rooms[i])).unwrap();
    let max = far(rooms[boss_room]).max(1) as f64;
    for (n, &r) in rooms.iter().enumerate().skip(1) {
        let depth = far(r) as f64 / max;
        let mut sites: Vec<_> = (r.y + 1..r.y + r.h - 1)
            .flat_map(|y| (r.x + 1..r.x + r.w - 1).map(move |x| (x, y)))
            .filter(|&p| p != r.center())
            .collect();
        sites.shuffle(&mut rng);
        let count = ((r.w * r.h) as f64 / 16. * (0.5 + depth)).round() as usize
            + if n == boss_room { 2 } else { 0 };
        for _ in 0..count.clamp(2, 12) {
            let (x, y) = sites.pop().context("room has no enemy site")?;
            let tier = ((rng.gen_range(0.0..2.0) + depth * 2.5) as usize).min(3);
            map.cells[idx(x, y)].object = Some(theme.mobs[tier]);
        }
        for _ in 0..(r.w * r.h) / 25 {
            let (x, y) = sites.pop().context("room has no decoration site")?;
            map.cells[idx(x, y)].object = Some(theme.deco);
        }
    }
    let (bx, by) = rooms[boss_room].center();
    map.cells[idx(bx, by)].object = Some(theme.boss);
    let start = &mut map.cells[idx(sx, sy)];
    start.object = Some(ENTRANCE);
    start.region = 1;
    map.spawns.push((sx as f32 + 0.5, sy as f32 + 0.5));
    for cell in &map.cells {
        ensure!(
            content.grounds.contains_key(&cell.ground),
            "unknown {} ground {:#x}",
            theme.name,
            cell.ground
        );
        if let Some(kind) = cell.object {
            content.get(kind)?;
        }
    }
    tracing::info!(
        seed,
        dungeon = theme.name,
        rooms = rooms.len(),
        seals,
        "generated procedural dungeon"
    );
    Ok(map)
}

/// Recursive-backtracker labyrinth on a 13x13 grid of 4x4 chambers; closed walls become
/// breakable shortcuts, the farthest chamber grows into the boss hall.
fn maze(content: &Content, theme: &Theme, seed: u64) -> Result<Map> {
    const N: i32 = 13;
    const S: i32 = 5;
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let (w, h) = (N * S + 3, N * S + 3);
    let idx = |x: i32, y: i32| (y * w + x) as usize;
    let cell = |c: i32| (c % N, c / N);
    let mut open = vec![[false; 4]; (N * N) as usize];
    let mut visited = vec![false; (N * N) as usize];
    let mut stack = vec![0i32];
    visited[0] = true;
    while let Some(&c) = stack.last() {
        let (cx, cy) = cell(c);
        let next: Vec<_> = [(1, 0, 0usize), (0, 1, 1), (-1, 0, 2), (0, -1, 3)]
            .into_iter()
            .filter_map(|(dx, dy, d)| {
                let (nx, ny) = (cx + dx, cy + dy);
                ((0..N).contains(&nx) && (0..N).contains(&ny) && !visited[(ny * N + nx) as usize])
                    .then_some((ny * N + nx, d))
            })
            .collect();
        match next.choose(&mut rng) {
            Some(&(n, d)) => {
                open[c as usize][d] = true;
                open[n as usize][(d + 2) % 4] = true;
                visited[n as usize] = true;
                stack.push(n);
            }
            None => {
                stack.pop();
            }
        }
    }
    let mut map = Map {
        width: w,
        height: h,
        cells: vec![
            Cell {
                ground: VOID,
                object: None,
                config: String::new(),
                terrain: 0,
                region: 0
            };
            (w * h) as usize
        ],
        spawns: vec![],
    };
    let origin = |c: i32| {
        let (cx, cy) = cell(c);
        (2 + cx * S, 2 + cy * S)
    };
    let floor = |map: &mut Map, x: i32, y: i32, region: u8, rng: &mut rand::rngs::StdRng| {
        let c = &mut map.cells[idx(x, y)];
        c.ground = if rng.gen_bool(0.18) { theme.floor2 } else { theme.floor };
        c.region = region;
    };
    for c in 0..N * N {
        let (x0, y0) = origin(c);
        for dy in 0..4 {
            for dx in 0..4 {
                floor(&mut map, x0 + dx, y0 + dy, (c % 250) as u8 + 2, &mut rng);
            }
        }
        if open[c as usize][0] {
            for dy in 0..4 {
                floor(&mut map, x0 + 4, y0 + dy, 0, &mut rng);
            }
        }
        if open[c as usize][1] {
            for dx in 0..4 {
                floor(&mut map, x0 + dx, y0 + 4, 0, &mut rng);
            }
        }
    }
    // Distance in chambers from the entrance decides the boss hall and the mob tiers.
    let mut dist = vec![-1i32; (N * N) as usize];
    let mut queue = VecDeque::from([0i32]);
    dist[0] = 0;
    while let Some(c) = queue.pop_front() {
        let (cx, cy) = cell(c);
        for (dx, dy, d) in [(1, 0, 0usize), (0, 1, 1), (-1, 0, 2), (0, -1, 3)] {
            let n = (cy + dy) * N + cx + dx;
            if open[c as usize][d] && dist[n as usize] < 0 {
                dist[n as usize] = dist[c as usize] + 1;
                queue.push_back(n);
            }
        }
    }
    let boss = (0..N * N).max_by_key(|&c| dist[c as usize]).unwrap();
    let (bx, by) = cell(boss);
    let hall: Vec<i32> = (by - 1..=by + 1)
        .flat_map(|y| (bx - 1..=bx + 1).map(move |x| (x, y)))
        .filter(|(x, y)| (0..N).contains(x) && (0..N).contains(y))
        .map(|(x, y)| y * N + x)
        .collect();
    let (hx0, hy0) = origin(*hall.iter().min().unwrap());
    let (hx1, hy1) = origin(*hall.iter().max().unwrap());
    for y in hy0..hy1 + 4 {
        for x in hx0..hx1 + 4 {
            floor(&mut map, x, y, (boss % 250) as u8 + 2, &mut rng);
        }
    }
    // The entrance is a four-chamber hall so arrivals have room around the portal.
    let start_hall = [0, 1, N, N + 1];
    for y in origin(0).1..origin(N + 1).1 + 4 {
        for x in origin(0).0..origin(N + 1).0 + 4 {
            floor(&mut map, x, y, 2, &mut rng);
        }
    }
    // Closed walls between neighbouring chambers become breakable shortcuts (never into the hall).
    let mut shortcuts: Vec<_> = (0..N * N)
        .flat_map(|c| [(c, 0usize), (c, 1)])
        .filter(|&(c, d)| {
            let (cx, cy) = cell(c);
            let (nx, ny) = if d == 0 { (cx + 1, cy) } else { (cx, cy + 1) };
            nx < N && ny < N
                && !open[c as usize][d]
                && !hall.contains(&c)
                && !hall.contains(&(ny * N + nx))
                && !start_hall.contains(&c)
                && !start_hall.contains(&(ny * N + nx))
        })
        .collect();
    shortcuts.shuffle(&mut rng);
    let mut seals = 0;
    for (c, d) in shortcuts.into_iter().take(rng.gen_range(5..=8)) {
        let (x0, y0) = origin(c);
        for i in 0..4 {
            let (x, y) = if d == 0 { (x0 + 4, y0 + i) } else { (x0 + i, y0 + 4) };
            floor(&mut map, x, y, 0, &mut rng);
            map.cells[idx(x, y)].object = Some(theme.breakable);
        }
        seals += 1;
    }
    let is_floor = |m: &Map, x: i32, y: i32| m.cells[idx(x, y)].ground != VOID;
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            if !is_floor(&map, x, y)
                && (-1..=1).any(|dy| (-1..=1).any(|dx| is_floor(&map, x + dx, y + dy)))
            {
                map.cells[idx(x, y)].object = Some(theme.wall);
            }
        }
    }
    let max = dist.iter().copied().max().unwrap().max(1) as f64;
    for c in 1..N * N {
        if hall.contains(&c) || start_hall.contains(&c) {
            continue;
        }
        let (x0, y0) = origin(c);
        let depth = dist[c as usize].max(0) as f64 / max;
        let mut sites = vec![(x0 + 1, y0 + 1), (x0 + 2, y0 + 2), (x0 + 1, y0 + 2), (x0 + 2, y0 + 1)];
        sites.shuffle(&mut rng);
        for _ in 0..rng.gen_range(0..=2) {
            let (x, y) = sites.pop().unwrap();
            let tier = ((rng.gen_range(0.0..2.0) + depth * 2.5) as usize).min(3);
            map.cells[idx(x, y)].object = Some(theme.mobs[tier]);
        }
        if rng.gen_bool(0.25) {
            let (x, y) = sites.pop().unwrap();
            map.cells[idx(x, y)].object = Some(theme.deco);
        }
    }
    let (bx0, by0) = origin(boss);
    for (dx, dy) in [(1, 1), (-5, 1), (6, 1)] {
        let (x, y) = (bx0 + dx, by0 + dy);
        if is_floor(&map, x, y) && map.cells[idx(x, y)].object.is_none() && (dx, dy) != (1, 1) {
            map.cells[idx(x, y)].object = Some(theme.mobs[3]);
        }
    }
    map.cells[idx(bx0 + 1, by0 + 1)].object = Some(theme.boss);
    let (sx, sy) = (origin(0).0 + 4, origin(0).1 + 4);
    let start = &mut map.cells[idx(sx, sy)];
    start.object = Some(ENTRANCE);
    start.region = 1;
    map.spawns.push((sx as f32 + 0.5, sy as f32 + 0.5));
    for cell in &map.cells {
        ensure!(
            content.grounds.contains_key(&cell.ground),
            "unknown {} ground {:#x}",
            theme.name,
            cell.ground
        );
        if let Some(kind) = cell.object {
            content.get(kind)?;
        }
    }
    tracing::info!(seed, dungeon = theme.name, seals, "generated procedural labyrinth");
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> Content {
        Content::load(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/data"
        )))
        .unwrap()
    }

    #[test]
    fn roster_matches_selected_xml() {
        let content = content();
        for &(kind, ..) in ROSTER {
            crate::encounter::validate(&content, kind).unwrap();
            crate::loot::validate(&content, kind).unwrap();
            assert!(content.get(kind).unwrap().enemy, "{kind:#x}");
        }
        for t in &THEMES {
            let wall = content.get(t.breakable).unwrap();
            assert!(
                wall.enemy && wall.blocks && wall.class == "Wall",
                "{}",
                t.name
            );
            assert!(content.get(t.wall).unwrap().blocks && !content.get(t.wall).unwrap().enemy);
            assert!(content.grounds[&t.floor] > 0. && content.grounds[&t.floor2] > 0.);
            crate::loot::validate(&content, t.breakable).unwrap();
        }
    }

    #[test]
    fn dungeons_connect_through_breakable_seals_of_at_most_four() {
        let content = content();
        for t in &THEMES {
            for seed in 0..32 {
                let map = generate(&content, t, seed).unwrap();
                let at = |x: i32, y: i32| &map.cells[(y * map.width + x) as usize];
                let open = |x: i32, y: i32, breaking: bool| {
                    (0..map.width).contains(&x)
                        && (0..map.height).contains(&y)
                        && at(x, y).ground != VOID
                        && at(x, y).object.is_none_or(|k| {
                            !content.objects[&k].blocks || breaking && k == t.breakable
                        })
                };
                for (dx, dy) in [(1, 0), (0, 1)] {
                    for y in 0..map.height {
                        for x in 0..map.width {
                            let run = (0..)
                                .take_while(|&i| {
                                    let (px, py) = (x + dx * i, y + dy * i);
                                    px < map.width
                                        && py < map.height
                                        && at(px, py).object == Some(t.breakable)
                                })
                                .count();
                            assert!(
                                run <= MAX_BREAKABLE_RUN,
                                "{} seed {seed}: {run} breakable walls in a row",
                                t.name
                            );
                        }
                    }
                }
                assert!(
                    map.cells.iter().any(|c| c.object == Some(t.breakable)),
                    "{} seed {seed}: no seal",
                    t.name
                );
                let (sx, sy) = (map.spawns[0].0 as i32, map.spawns[0].1 as i32);
                assert_eq!(at(sx, sy).object, Some(ENTRANCE));
                let mut seen = std::collections::BTreeSet::from([(sx, sy)]);
                let mut todo = vec![(sx, sy)];
                while let Some((x, y)) = todo.pop() {
                    for p in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                        if open(p.0, p.1, true) && seen.insert(p) {
                            todo.push(p);
                        }
                    }
                }
                let walkable = (0..map.width * map.height)
                    .filter(|i| open(i % map.width, i / map.width, true))
                    .count();
                assert_eq!(
                    seen.len(),
                    walkable,
                    "{} seed {seed}: unreachable floor",
                    t.name
                );
                let bosses: Vec<_> = (0..map.width * map.height)
                    .filter(|&i| map.cells[i as usize].object == Some(t.boss))
                    .collect();
                assert_eq!(bosses.len(), 1, "{} seed {seed}: boss count", t.name);
            }
        }
    }
}
