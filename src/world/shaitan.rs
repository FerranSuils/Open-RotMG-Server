use super::*;
use crate::{
    condition,
    dungeon::shaitan::{
        self as sh, BALLOON, CREEPY_HANDS, CREEPY_HEAD, HAND_L, HAND_R, HEAD, LAVA, SMASH_L, SMASH_R,
    },
    encounter::{Motion, attacks::{Schedule, shaitan_schedule}, nr_speed},
};

const DORMANT: u64 = 0;
const GUARD: u64 = 1;
const MOVE: u64 = 2;
const FIRE: u64 = 3;
const RETURN: u64 = 4;
const ENTRY: u64 = 5;
const DYING: u64 = 6;
const TAUNTS: [&str; 5] = [
    "You disturb an ancient evil...",
    "Let loose the fists of war!",
    "Let loose the fists of war!",
    "YOUR ARE BEGINNING TO UPSET ME. ENJOY A FAST DEATH!",
    "BE CONSUMED BY FLAMES!",
];
const HAND_TAUNTS: [&str; 6] = [
    "Hah. Weakings. This will be slightly enjoyable.",
    "You are in the presence of demi-god, motral",
    "What hath the keepers brought Shaitan?",
    "You think it wise to use such cheap tricks?",
    "You make a foolish mistake, mortal.",
    "Yes, little mortals. Meet your doom at the hands of SHAITAN!",
];

/// The head's script. Its entity `stage` mirrors `phase * 10 + step` for the attack schedule.
pub(super) struct Fight {
    step: u64,
    phase: u64,
    waypoint: usize,
    since: u64,
    serial: u64,
    clock: u64,
    lava: Vec<(u64, usize)>,
    next_lava: u64,
    smash_at: u64,
    fresh: bool,
    hand_homes: [Pos; 2],
}

impl World {
    fn shaitan_hands(&self, head: i32) -> Vec<i32> {
        self.actors()
            .filter(|e| sh::is_hand(e.kind) && e.parent == Some(head))
            .map(|e| e.id)
            .collect()
    }
    fn shaitan_set(&mut self, step: u64) {
        let now = self.now;
        let f = self.shaitan.as_mut().expect("Shaitan fight");
        (f.step, f.since, f.serial, f.fresh) = (step, now, f.serial + 1, true);
    }
    /// Moves an entity toward `to` without pathing, keeping the spatial index in step.
    fn shaitan_glide(&mut self, id: i32, to: Pos, rate: f32) -> bool {
        let f = self.shaitan.as_mut().expect("Shaitan fight");
        let dt = self.now.saturating_sub(f.clock).min(200);
        f.clock = self.now;
        let e = &self.entities[&id];
        let (old, d) = (e.pos, e.pos.distance(to));
        if d < 0.05 {
            return true;
        }
        let step = (nr_speed(rate) * dt as f32 / 1000.).min(d);
        let new = Pos {
            x: old.x + (to.x - old.x) / d * step,
            y: old.y + (to.y - old.y) / d * step,
        };
        let width = self.map.width as usize;
        let cell = |p: Pos| p.y as usize * width + p.x as usize;
        if cell(old) != cell(new) {
            assert!(self.cells.remove(cell(old), id), "gliding NPC absent from spatial index");
            self.cells.insert(cell(new), id, true);
        }
        let e = self.entities.get_mut(&id).unwrap();
        e.pos = new;
        e.last_step = Pos {
            x: new.x - old.x,
            y: new.y - old.y,
        };
        step >= d
    }
    fn shaitan_spawn(&mut self, head: i32, kind: u16, pos: Pos) -> Result<i32> {
        ensure!(
            self.summon_occupancy() < self.summon_limit(),
            "Shaitan summon budget exhausted"
        );
        let at = (0..16)
            .map(|i| {
                let a = i as f32 * std::f32::consts::TAU / 16.;
                Pos {
                    x: pos.x + (i / 4) as f32 * 0.5 * a.cos(),
                    y: pos.y + (i / 4) as f32 * 0.5 * a.sin(),
                }
            })
            .find(|p| self.walkable(*p))
            .context("no walkable Shaitan summon position")?;
        let id = self.spawn_entity(kind, at, vec![])?;
        let e = self.entities.get_mut(&id).unwrap();
        e.activated = Some(self.now);
        e.parent.set(Some(head), id, &mut self.timed_ids);
        Ok(id)
    }
    fn shaitan_respawn_hands(&mut self, head: i32) -> Result<()> {
        let homes = self.shaitan.as_ref().expect("Shaitan fight").hand_homes;
        for (kind, home) in [(HAND_R, homes[0]), (HAND_L, homes[1])] {
            let hand = self.shaitan_spawn(head, kind, home)?;
            let e = self.entities.get_mut(&hand).unwrap();
            e.stage = Some(2);
            e.next_aux = self.now;
            e.home = home;
            e.condition = 0;
        }
        Ok(())
    }

    pub(super) fn shaitan_mechanics(&mut self, id: i32, target: Option<Pos>) -> Result<()> {
        match self.entities[&id].kind {
            HEAD => self.shaitan_head(id, target),
            HAND_R | HAND_L => {
                self.shaitan_hand(id);
                Ok(())
            }
            BALLOON => {
                let e = self.entities.get_mut(&id).unwrap();
                if e.stage.is_none() {
                    e.stage = Some(0);
                    e.next_aux = self.now + 5000;
                } else if self.now >= e.next_aux {
                    e.condition &= !condition::bit(condition::INVULNERABLE);
                }
                Ok(())
            }
            SMASH_R | SMASH_L => {
                let e = &self.entities[&id];
                if e.stage.is_none() && target.is_some_and(|p| e.pos.distance(p) < 2.) {
                    let e = self.entities.get_mut(&id).unwrap();
                    e.stage = Some(1);
                    e.activated = Some(self.now);
                    e.expires.set(Some(self.now + 650), id, &mut self.timed_ids);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    fn shaitan_hand(&mut self, id: i32) {
        let e = &self.entities[&id];
        let Some(old) = e.stage else { return };
        let start = e.next_aux;
        let (stage, protected) = if self.now < start {
            (1, true)
        } else {
            (
                match (self.now - start) % 8000 {
                    0..4000 => 2,
                    4000..6000 => 3,
                    _ => 4,
                },
                false,
            )
        };
        let taunt = stage == 4 && old != 4;
        let bits = condition::bit(condition::INVINCIBLE) | condition::bit(condition::INVULNERABLE);
        let e = self.entities.get_mut(&id).unwrap();
        e.stage = Some(stage);
        e.condition &= !bits;
        if protected {
            e.condition |= bits;
        }
        if taunt {
            let line = HAND_TAUNTS[self.rng.gen_range(0..HAND_TAUNTS.len())];
            self.broadcast_text("Hand of Shaitan", line);
        }
    }
    fn shaitan_head(&mut self, id: i32, target: Option<Pos>) -> Result<()> {
        let (home, hp) = (self.entities[&id].home, self.entities[&id].hp);
        let max = self.content.objects[&HEAD].hp;
        if self.shaitan.is_none() {
            let homes: Vec<_> = self
                .actors()
                .filter(|e| sh::is_hand(e.kind))
                .map(|e| (e.kind, e.home))
                .collect();
            let find = |kind| homes.iter().find(|(k, _)| *k == kind).map(|(_, p)| *p);
            self.shaitan = Some(Fight {
                step: DORMANT,
                phase: 0,
                waypoint: 0,
                since: self.now,
                serial: 0,
                clock: self.now,
                lava: vec![],
                next_lava: 0,
                smash_at: u64::MAX,
                fresh: false,
                hand_homes: [
                    find(HAND_R).context("Lair has no right hand")?,
                    find(HAND_L).context("Lair has no left hand")?,
                ],
            });
        }
        let now = self.now;
        let (step, phase, since) = {
            let f = self.shaitan.as_ref().unwrap();
            (f.step, f.phase, f.since)
        };
        match step {
            DORMANT => {
                if target.is_some_and(|p| p.distance(home) < 5.) {
                    for hand in self.actors().filter(|e| sh::is_hand(e.kind)).map(|e| e.id).collect::<Vec<_>>() {
                        let e = self.entities.get_mut(&hand).unwrap();
                        e.stage = Some(1);
                        e.next_aux = now + 7000;
                        e.parent.set(Some(id), hand, &mut self.timed_ids);
                    }
                    self.broadcast_text("Shaitan the Advisor", TAUNTS[0]);
                    self.shaitan_set(GUARD);
                }
            }
            GUARD => {
                if now > since + 7000 && self.shaitan_hands(id).is_empty() {
                    self.shaitan_set(if phase == 0 { MOVE } else { ENTRY });
                }
            }
            MOVE => {
                let to = Pos {
                    x: home.x + sh::SWEEP[self.shaitan.as_ref().unwrap().waypoint],
                    y: home.y,
                };
                if self.shaitan_glide(id, to, 0.8) && now >= since + 2000 {
                    self.shaitan_set(FIRE);
                }
            }
            FIRE => {
                let pct = i64::from(hp) * 100 / i64::from(max);
                let next = [80, 60, 40, 20].iter().filter(|t| pct < **t).count() as u64;
                if next >= 4 {
                    self.broadcast_text(
                        "Shaitan the Advisor",
                        "THE MAD GOD SHALL KNOW OF THESE TRANSGRESSIONS!",
                    );
                    self.entities.get_mut(&id).unwrap().expires.set(Some(now + 2000), id, &mut self.timed_ids);
                    self.shaitan_set(DYING);
                } else if next > phase {
                    self.broadcast_text("Shaitan the Advisor", TAUNTS[next as usize]);
                    self.shaitan.as_mut().unwrap().phase = next;
                    self.shaitan_set(ENTRY);
                } else if now >= since + 5000 {
                    let f = self.shaitan.as_mut().unwrap();
                    f.waypoint += 1;
                    let done = f.waypoint >= sh::SWEEP.len();
                    self.shaitan_set(if done { RETURN } else { MOVE });
                }
            }
            RETURN => {
                if self.shaitan_glide(id, home, 0.8) && now >= since + 2000 {
                    self.shaitan.as_mut().unwrap().waypoint = 0;
                    self.shaitan_respawn_hands(id)?;
                    self.shaitan_set(GUARD);
                }
            }
            ENTRY => self.shaitan_entry(id, home, phase)?,
            _ => {}
        }
        self.shaitan_lava_step();
        let step = self.shaitan.as_ref().unwrap();
        let stage = step.phase * 10 + step.step;
        let bits = condition::bit(condition::INVULNERABLE)
            | condition::bit(condition::INVINCIBLE)
            | condition::bit(condition::ARMOR_BROKEN);
        let e = self.entities.get_mut(&id).unwrap();
        e.stage = Some(stage);
        e.condition &= !bits;
        e.condition |= match step.step {
            DORMANT => condition::bit(condition::INVINCIBLE),
            FIRE if step.phase >= 2 => condition::bit(condition::ARMOR_BROKEN),
            FIRE => 0,
            _ => condition::bit(condition::INVULNERABLE),
        };
        Ok(())
    }
    /// Each loop after the first phase: creepy heads, creepy hands, smashing hands and a rain of lava.
    fn shaitan_entry(&mut self, id: i32, home: Pos, phase: u64) -> Result<()> {
        let now = self.now;
        let first = std::mem::take(&mut self.shaitan.as_mut().unwrap().fresh);
        self.shaitan_glide(id, home, 0.8);
        if first {
            let angles: &[f32] = match phase {
                1 => &[90.],
                2 | 3 => &[65., 105.],
                _ => &[65., 90., 105.],
            };
            for angle in angles {
                let a = angle.to_radians();
                let at = Pos {
                    x: home.x + 5. * a.cos(),
                    y: home.y + 5. * a.sin(),
                };
                self.manor_toss(id, CREEPY_HEAD, at, 60000)?;
            }
            self.spawn_children(id, CREEPY_HANDS, 2, 4.)?;
            let homes = self.shaitan.as_ref().unwrap().hand_homes;
            for (kind, at) in [(SMASH_R, homes[0]), (SMASH_L, homes[1])] {
                self.shaitan_spawn(id, kind, at)?;
            }
            self.shaitan.as_mut().unwrap().smash_at = now + 2000;
        }
        let f = self.shaitan.as_mut().unwrap();
        if now >= f.smash_at {
            f.smash_at = u64::MAX;
            let homes = f.hand_homes;
            for (kind, at) in [(SMASH_R, homes[0]), (SMASH_L, homes[1])] {
                self.shaitan_spawn(id, kind, at)?;
            }
        }
        let f = self.shaitan.as_mut().unwrap();
        if now >= f.next_lava && now < f.since + 5500 {
            f.next_lava = now + 500;
            let region = self.map.cell(home.x, home.y).map(|c| c.region).unwrap_or(0);
            let width = self.map.width as usize;
            let picks: Vec<_> = self
                .map
                .cells
                .iter()
                .enumerate()
                .filter(|(i, c)| {
                    c.region == region
                        && c.object.is_none()
                        && c.ground != LAVA
                        && Pos {
                            x: (i % width) as f32 + 0.5,
                            y: (i / width) as f32 + 0.5,
                        }
                        .distance(home)
                            <= 14.
                })
                .map(|(i, _)| i)
                .collect();
            if let Some(i) = picks.choose(&mut self.rng).copied() {
                let at = Pos {
                    x: (i % width) as f32 + 0.5,
                    y: (i / width) as f32 + 0.5,
                };
                self.telegraph_throw(id, at, 0xffff6000);
                self.shaitan.as_mut().unwrap().lava.push((now + 1500, i));
            }
        }
        if now >= self.shaitan.as_ref().unwrap().since + 5500 {
            self.shaitan.as_mut().unwrap().waypoint = 0;
            self.shaitan_set(MOVE);
        }
        Ok(())
    }
    /// Landed lava makers scorch a 3x3 patch for five seconds.
    fn shaitan_lava_step(&mut self) {
        let now = self.now;
        let f = self.shaitan.as_mut().unwrap();
        let (due, rest): (Vec<_>, Vec<_>) = f.lava.drain(..).partition(|(at, _)| *at <= now);
        f.lava = rest;
        let width = self.map.width as usize;
        for (_, i) in due {
            for (dx, dy) in (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (dx, dy))) {
                let j = (i as i64 + dy * width as i64 + dx) as usize;
                if self.map.cells[j].ground != LAVA && self.map.cells[j].ground != 0xfe {
                    let old = self.map.cells[j].ground;
                    self.lava_patches.insert(j, (now + 5000, old));
                    self.change_ground(j, LAVA);
                }
            }
        }
    }
    pub(super) fn shaitan_movement(&self, id: i32) -> (Motion, f32) {
        let e = &self.entities[&id];
        let p = sh::profile(e.kind).expect("validated Shaitan profile");
        match e.kind {
            HEAD | BALLOON => (Motion::Stationary, 0.),
            HAND_R | HAND_L => (sh::hand_motion(e.stage), p.speed),
            SMASH_R | SMASH_L if e.stage.is_some() => (Motion::Stationary, 0.),
            _ => (p.motion, p.speed),
        }
    }
    pub(super) fn shaitan_schedule(&self, id: i32) -> Schedule {
        let e = &self.entities[&id];
        let now = self.now;
        if e.kind == HEAD {
            let f = self.shaitan.as_ref();
            let stage = e.stage.unwrap_or(0);
            return shaitan_schedule(
                e.kind,
                stage,
                f.map_or(0, |f| f.serial),
                f.map_or(0, |f| now.saturating_sub(f.since)),
            );
        }
        if sh::is_hand(e.kind) {
            let stage = e.stage.unwrap_or(0);
            let start = e.next_aux;
            let cycle = now.saturating_sub(start) / 8000;
            let age = match stage {
                2 => (now.saturating_sub(start)) % 8000,
                3 => (now.saturating_sub(start)) % 8000 - 4000,
                _ => 0,
            };
            return shaitan_schedule(e.kind, stage, cycle * 8 + stage, age);
        }
        let age = e.activated.map_or(0, |a| now.saturating_sub(a));
        shaitan_schedule(e.kind, e.stage.unwrap_or(0), e.stage.unwrap_or(0), age)
    }
    pub(super) fn shaitan_death(&mut self, defeated: &Entity) -> Result<()> {
        if defeated.kind != HEAD {
            return Ok(());
        }
        let followers: Vec<_> = self
            .actors()
            .filter(|e| e.parent == Some(defeated.id) || sh::ROSTER.contains(&e.kind))
            .map(|e| e.id)
            .collect();
        for id in followers {
            self.remove_entity(id);
        }
        self.manor_births.retain(|t| t.parent != defeated.id);
        self.shaitan = None;
        self.broadcast_text("Oryx the Mad God", "Shaitan is no more. His lair is yours.");
        let balloon = self.spawn_entity(BALLOON, defeated.pos, vec![])?;
        self.entities.get_mut(&balloon).unwrap().activated = Some(self.now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn ids(w: &World, kind: u16) -> Vec<i32> {
        w.actors().filter(|e| e.kind == kind).map(|e| e.id).collect()
    }
    fn tick(w: &mut World, near: Pos, ms: u64) {
        for _ in 0..ms / 50 {
            w.now += 50;
            for id in ids(w, HEAD).into_iter().chain(ids(w, HAND_R)).chain(ids(w, HAND_L)) {
                w.shaitan_mechanics(id, Some(near)).unwrap();
            }
            w.mechanic_expiry();
        }
    }

    #[test]
    fn the_fight_runs_through_hands_sweeps_and_phases_to_a_clear() {
        let content = Arc::new(Content::load(Path::new("data")).unwrap());
        let map = Arc::new(sh::generate(&content, 5).unwrap());
        let mut w = World::new(content, map).unwrap();
        w.name = sh::NAME.into();
        w.encounter = true;
        w.now = 100_000;
        let p = w.enter(super::super::tests::ch(&w), None).unwrap();
        let head = ids(&w, HEAD)[0];
        let home = w.entities[&head].home;
        let near = Pos { x: home.x, y: home.y + 3. };
        w.players.get_mut(&p).unwrap().pos = near;
        // Asleep and untouchable until someone walks up; hands only open after a seven second grace.
        assert_eq!(w.damage_enemy(p, head, 5000, true, &[], None).unwrap(), 0);
        tick(&mut w, Pos { x: home.x + 30., y: home.y }, 100);
        assert_eq!(w.shaitan.as_ref().unwrap().step, DORMANT);
        tick(&mut w, near, 50);
        assert_eq!(w.shaitan.as_ref().unwrap().step, GUARD);
        let hands = [ids(&w, HAND_R)[0], ids(&w, HAND_L)[0]];
        assert_eq!(w.damage_enemy(p, hands[0], 5000, true, &[], None).unwrap(), 0);
        tick(&mut w, near, 7100);
        assert_eq!(w.entities[&hands[0]].stage, Some(2));
        for hand in hands {
            let hp = w.entities[&hand].hp;
            assert!(w.damage_enemy(p, hand, hp + 100, true, &[], None).unwrap() > 0);
        }
        assert_eq!(w.damage_enemy(p, head, 5000, true, &[], None).unwrap(), 0, "head shielded while hands live");
        tick(&mut w, near, 50);
        assert_eq!(w.shaitan.as_ref().unwrap().step, MOVE);
        let mut seen = BTreeSet::new();
        for _ in 0..1000 {
            tick(&mut w, near, 50);
            seen.insert(w.shaitan.as_ref().unwrap().step);
            if w.shaitan.as_ref().unwrap().step == GUARD {
                break;
            }
        }
        assert!(seen.is_superset(&BTreeSet::from([MOVE, FIRE, RETURN])), "{seen:?}");
        assert_eq!(ids(&w, HAND_R).len() + ids(&w, HAND_L).len(), 2, "hands come back");
        // A hurt head in its fire window advances a phase and starts the entry set piece.
        for hand in ids(&w, HAND_R).into_iter().chain(ids(&w, HAND_L)) {
            let hp = w.entities[&hand].hp;
            w.damage_enemy(p, hand, hp + 100, true, &[], None).unwrap();
        }
        for _ in 0..600 {
            tick(&mut w, near, 50);
            if w.shaitan.as_ref().unwrap().step == FIRE {
                break;
            }
        }
        w.entities.get_mut(&head).unwrap().hp = 65000 * 7 / 10;
        tick(&mut w, near, 100);
        assert_eq!((w.shaitan.as_ref().unwrap().phase, w.shaitan.as_ref().unwrap().step), (1, ENTRY));
        tick(&mut w, near, 3000);
        assert!(!ids(&w, SMASH_R).is_empty() && !ids(&w, CREEPY_HANDS).is_empty());
        assert!(w.map.cells.iter().any(|c| c.ground == LAVA), "lava rain");
        // Dropping below a fifth ends the fight and the head pays out.
        tick(&mut w, near, 4000);
        for _ in 0..1000 {
            tick(&mut w, near, 50);
            if w.shaitan.as_ref().unwrap().step == FIRE {
                break;
            }
        }
        w.entities.get_mut(&head).unwrap().hp = 65000 / 10;
        tick(&mut w, near, 100);
        assert_eq!(w.shaitan.as_ref().unwrap().step, DYING);
        tick(&mut w, near, 2200);
        assert!(w.cleared && w.shaitan.is_none());
        assert!(ids(&w, HEAD).is_empty() && ids(&w, SMASH_R).is_empty());
        let balloon = ids(&w, BALLOON)[0];
        w.shaitan_mechanics(balloon, Some(near)).unwrap();
        assert_eq!(w.damage_enemy(p, balloon, 1000, true, &[], None).unwrap(), 0, "balloon rises untouchable");
        w.now += 5050;
        w.shaitan_mechanics(balloon, Some(near)).unwrap();
        assert!(w.damage_enemy(p, balloon, 90000, true, &[], None).unwrap() > 0);
        assert!(ids(&w, BALLOON).is_empty());
    }
}
