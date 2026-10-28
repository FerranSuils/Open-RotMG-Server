//! Autoshot (/autoshot): while it is on the server fires the player's weapon at the enemy picked by
//! crate::autoshot, as often as the weapon allows. It never doubles manual fire: it only shoots when
//! the player has not shot for a couple of attack periods.
//!
//! The server does the whole shot. Its bullets use ids 128..=255, which the client never uses for its
//! own 0..127 counter, and the shot packet also goes to the shooter (a manual shot does not need it,
//! because the shooter's client draws that one itself).
use super::*;
use crate::autoshot::{self as logic, Candidate, Settings};
use crate::condition;

/// First bullet id of autoshot bullets; the client's own counter stays below it.
const FIRST_BULLET: u8 = 128;
/// Samples along the line to the target when looking for walls.
const SIGHT_STEP: f32 = 0.4;
/// Manual shots within this many attack periods keep autoshot quiet.
const MANUAL_GRACE: u64 = 2;
/// Enemies in this state take no damage, so they are never picked.
fn untouchable() -> i32 {
    condition::bit(condition::STASIS)
        | condition::bit(condition::INVINCIBLE)
        | condition::bit(condition::INVULNERABLE)
}

fn line(p: &Player, text: &str) {
    let packet = chat::chat_info(text);
    send(p, packet.id, packet.body);
}

/// Damage multiplier of a player attack: attack stat, Weak and Damaging (shared with manual shots).
pub(super) fn damage_multiplier(
    p: &Player,
    content: &Content,
    now: u64,
    condition: i32,
) -> Result<f64> {
    Ok(if condition & condition::bit(condition::WEAK) != 0 {
        0.5
    } else {
        (0.5 + (f64::from(p.effective(content, now)?[2]) / 75.) * (2. - 0.5))
            * if condition & condition::bit(condition::DAMAGING) != 0 {
                1.5
            } else {
                1.
            }
    })
}

impl Player {
    /// The settings in force: a pending /autoshot choice, otherwise the saved ones.
    pub(super) fn autoshot_settings(&self) -> Settings {
        self.autoshot_choice.unwrap_or(self.character.autoshot)
    }
}

impl World {
    pub(crate) fn autoshot_command(&mut self, id: i32, argument: &str) -> Result<()> {
        let p = self
            .players
            .get_mut(&id)
            .context("autoshot owner missing")?;
        let current = p.autoshot_settings();
        match logic::parse(current, p.autoshot_on, argument) {
            Err(message) => line(p, &message),
            Ok(parsed) => {
                p.autoshot_on = parsed.enabled;
                if parsed.settings != current {
                    p.autoshot_choice = Some(parsed.settings);
                }
                line(p, &logic::describe(parsed.enabled, &parsed.settings));
                tracing::info!(player = id, on = parsed.enabled, settings = ?parsed.settings, "autoshot changed");
            }
        }
        Ok(())
    }

    /// Nothing between the two points stops a friendly bullet (walls, void).
    fn shot_blocked(&self, from: Pos, to: Pos, passes_cover: bool) -> bool {
        let steps = (from.distance(to) / SIGHT_STEP).ceil() as i32;
        (1..steps).any(|i| {
            let t = i as f32 / steps as f32;
            let at = Pos {
                x: from.x + (to.x - from.x) * t,
                y: from.y + (to.y - from.y) * t,
            };
            self.map.cell(at.x, at.y).is_none_or(|c| c.ground == 255)
                || self.entities_at(at).any(|e| {
                    let o = &self.content.objects[&e.kind];
                    o.static_object
                        && !o.enemy
                        && (o.enemy_occupy_square || o.occupy_square && !passes_cover)
                })
        })
    }

    pub(super) fn autoshot_step(&mut self) {
        let ids: Vec<i32> = self
            .players
            .iter()
            .filter(|(_, p)| p.autoshot_on)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            if let Err(error) = self.autoshot_fire(id) {
                tracing::warn!(player = id, %error, "autoshot skipped");
            }
        }
    }

    fn autoshot_fire(&mut self, id: i32) -> Result<()> {
        let now = self.now;
        let p = &self.players[&id];
        let condition = p.conditions(now);
        if !p.active()
            || p.paused
            || p.panel_open
            || p.persistence_failure.is_some()
            || condition & condition::bit(condition::STUNNED) != 0
            || now < p.autoshot_at
        {
            return Ok(());
        }
        let Some(weapon) = u16::try_from(p.character.items[0]).ok() else {
            return Ok(());
        };
        let item = self.content.get(weapon)?;
        let Some(spec) = item.projectiles.get(&0).cloned() else {
            return Ok(());
        };
        if item.shots <= 0 {
            return Ok(());
        }
        let (shots, arc) = (item.shots, item.arc.to_radians());
        let cooldown =
            reconciliation::attack_cooldown(p, &self.content, now, item.rate)?.max(50.) as u64;
        // Manual fire has priority, so the two never add up.
        if now < p.last_attack_at + MANUAL_GRACE * cooldown {
            return Ok(());
        }
        let range = spec.speed * spec.lifetime / 10_000.;
        let candidates: Vec<(Candidate, Pos)> = self
            .actors_near(p.pos, range)
            .filter(|e| self.shootable(p, e, spec.passes_cover))
            .map(|e| {
                let o = &self.content.objects[&e.kind];
                let candidate = Candidate {
                    id: e.id,
                    distance: e.pos.distance(p.pos),
                    health: o.hp,
                    damage: o
                        .projectiles
                        .values()
                        .map(|s| s.min.max(s.max))
                        .max()
                        .unwrap_or(0),
                };
                (candidate, e.pos)
            })
            .collect();
        let list: Vec<Candidate> = candidates.iter().map(|(c, _)| *c).collect();
        let Some(target) = logic::choose(&p.autoshot_settings(), &list) else {
            return Ok(());
        };
        let aim = candidates.iter().find(|(c, _)| c.id == target).unwrap().1;
        if self.bullets.len() + shots as usize > 2048 {
            return Ok(());
        }
        let angle = (aim.y - p.pos.y).atan2(aim.x - p.pos.x);
        // The client displaces every muzzle along the central aim.
        let start = Pos {
            x: p.pos.x + 0.3 * angle.cos(),
            y: p.pos.y + 0.3 * angle.sin(),
        };
        let multiplier = damage_multiplier(p, &self.content, now, condition)?;
        for i in 0..shots {
            let angle = angle + arc * (i as f32 - (shots - 1) as f32 / 2.);
            self.players.get_mut(&id).unwrap().character.shots += 1;
            self.ally_bullet(id, weapon, &spec, start, angle, multiplier);
        }
        self.players.get_mut(&id).unwrap().autoshot_at = now + cooldown;
        tracing::debug!(player = id, target, "autoshot fired");
        Ok(())
    }

    /// The player sees this living enemy and can hit it with a shot from where it stands.
    pub(super) fn shootable(&self, p: &Player, e: &Entity, passes_cover: bool) -> bool {
        let o = &self.content.objects[&e.kind];
        e.hp > 0
            && o.enemy
            && o.destructible
            && o.class != "Wall"
            && p.known.contains(&e.id)
            && e.conditions(self.now) & untouchable() == 0
            && !self.shot_blocked(p.pos, e.pos, passes_cover)
    }

    /// Puts one server-fired bullet of the player's weapon `weapon` in the air (ids 128..=255) and
    /// sends it to the owner and every viewer as ALLYSHOOT.
    pub(super) fn ally_bullet(
        &mut self,
        id: i32,
        weapon: u16,
        spec: &Projectile,
        start: Pos,
        angle: f32,
        multiplier: f64,
    ) {
        let draw = if spec.min < spec.max {
            self.rng.gen_range(spec.min..=spec.max)
        } else {
            spec.min
        };
        let p = self.players.get_mut(&id).unwrap();
        let bullet = p.autoshot_bullet.max(FIRST_BULLET);
        p.autoshot_bullet = if bullet == u8::MAX {
            FIRST_BULLET
        } else {
            bullet + 1
        };
        self.bullets.push(Bullet {
            owner: id,
            source_kind: p.character.class,
            id: bullet,
            friendly: true,
            start,
            pos: start,
            angle,
            born: self.now,
            damage: (f64::from(draw) * multiplier) as i32,
            spec: spec.clone(),
            hit: BTreeSet::new(),
        });
        let mut out = Writer::new();
        out.u8(bullet).i32(id).i16(weapon as i16).f32(angle);
        for (other, viewer) in &self.players {
            if *other == id || viewer.known.contains(&id) {
                send(viewer, 74, out.0.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autoshot::{Mode, Stat};
    use crate::world::tests::{arena, ch};

    fn kind(w: &World, name: &str) -> u16 {
        *w.content
            .objects
            .iter()
            .find(|(_, o)| o.name == name)
            .unwrap_or_else(|| panic!("no object named {name}"))
            .0
    }
    /// A world with the player and two enemies: a weak crab close by and a tough minotaur further off.
    fn scene() -> (World, i32, i32, i32) {
        let mut w = arena(0);
        let p = w.enter(ch(&w), None).unwrap();
        let at = w.players[&p].pos;
        let crab = w
            .spawn_entity(
                kind(&w, "Shore Crab"),
                Pos {
                    x: at.x + 3.,
                    y: at.y,
                },
                vec![],
            )
            .unwrap();
        let minotaur = w
            .spawn_entity(
                kind(&w, "Axe Minotaur"),
                Pos {
                    x: at.x,
                    y: at.y + 5.,
                },
                vec![],
            )
            .unwrap();
        // Melee mobs would run up to the player; paralysed they stay where the test put them.
        for id in [crab, minotaur] {
            w.entities.get_mut(&id).unwrap().condition |= condition::bit(condition::PARALYZED);
        }
        (w, p, crab, minotaur)
    }
    /// Runs ticks (keeping the player alive) until the player has an
    /// autoshot bullet in the air, and returns its angle.
    fn first_autoshot_angle(w: &mut World, p: i32, ticks: usize) -> Option<f32> {
        for _ in 0..ticks {
            w.apply_condition(p, condition::INVULNERABLE, 200).unwrap();
            w.step();
            if let Some(b) = w
                .bullets
                .iter()
                .find(|b| b.owner == p && b.friendly && b.id >= FIRST_BULLET)
            {
                return Some(b.angle);
            }
        }
        None
    }

    #[test]
    fn autoshot_is_off_until_asked_then_shoots_at_the_nearest_enemy() {
        let (mut w, p, ..) = scene();
        assert_eq!(first_autoshot_angle(&mut w, p, 120), None, "off by default");
        w.autoshot_command(p, "on").unwrap();
        let angle = first_autoshot_angle(&mut w, p, 200).expect("autoshot fired");
        assert!(angle.abs() < 0.2, "the crab is straight ahead, got {angle}");
        let id = w.bullets.iter().find(|b| b.id >= FIRST_BULLET).unwrap().id;
        assert!(id >= 128);
        w.autoshot_command(p, "off").unwrap();
        w.bullets.clear();
        assert_eq!(first_autoshot_angle(&mut w, p, 120), None, "off again");
    }

    #[test]
    fn strongest_mode_goes_for_the_tough_enemy_and_weakest_for_the_frail_one() {
        let (mut w, p, ..) = scene();
        w.autoshot_command(p, "on strongest health radius 8")
            .unwrap();
        let angle = first_autoshot_angle(&mut w, p, 200).expect("autoshot fired");
        assert!(
            (angle - std::f32::consts::FRAC_PI_2).abs() < 0.2,
            "the minotaur is straight up, got {angle}"
        );
        let (mut w, p, ..) = scene();
        w.autoshot_command(p, "on weakest hp radius 8").unwrap();
        let angle = first_autoshot_angle(&mut w, p, 200).expect("autoshot fired");
        assert!(angle.abs() < 0.2, "the crab is the frail one, got {angle}");
        let (mut w, p, ..) = scene();
        w.autoshot_command(p, "on strongest radius 4").unwrap();
        let angle = first_autoshot_angle(&mut w, p, 200).expect("autoshot fired");
        assert!(
            angle.abs() < 0.2,
            "only the crab is inside 4 tiles, got {angle}"
        );
    }

    #[test]
    fn autoshot_credits_the_kills_to_its_owner() {
        let (mut w, p, crab, _) = scene();
        w.autoshot_command(p, "on").unwrap();
        w.entities.get_mut(&crab).unwrap().hp = 1;
        for _ in 0..400 {
            w.apply_condition(p, condition::INVULNERABLE, 200).unwrap();
            w.step();
            if !w.entities.contains_key(&crab) {
                break;
            }
        }
        assert!(!w.entities.contains_key(&crab), "the crab died to autoshot");
    }

    #[test]
    fn manual_shots_keep_autoshot_quiet() {
        let (mut w, p, ..) = scene();
        w.autoshot_command(p, "on").unwrap();
        for _ in 0..120 {
            w.apply_condition(p, condition::INVULNERABLE, 200).unwrap();
            w.players.get_mut(&p).unwrap().last_attack_at = w.now;
            w.step();
        }
        assert!(
            !w.bullets.iter().any(|b| b.id >= FIRST_BULLET),
            "no autoshot while the player shoots by hand"
        );
        assert!(
            first_autoshot_angle(&mut w, p, 300).is_some(),
            "autoshot resumes once the player stops"
        );
    }

    #[test]
    fn settings_are_saved_with_the_character_but_the_switch_is_not() {
        let (mut w, p, ..) = scene();
        w.autoshot_command(p, "on weakest damage radius 9").unwrap();
        let (pid, snapshot) = w
            .pending_progress_where(|_, _| true)
            .into_iter()
            .next()
            .unwrap();
        assert_eq!(snapshot.autoshot.mode, Mode::Weakest);
        assert_eq!(snapshot.autoshot.stat, Stat::Damage);
        assert_eq!(snapshot.autoshot.radius, 9);
        w.finish_progress(pid, snapshot).unwrap();
        assert_eq!(w.players[&p].character.autoshot.radius, 9);
        assert!(
            w.players[&p].autoshot_choice.is_none(),
            "nothing left pending"
        );
        assert!(
            w.players[&p].autoshot_on,
            "still on until the player turns it off"
        );
        let json = serde_json::to_string(&w.players[&p].character).unwrap();
        assert!(json.contains("\"autoshot\""));
        // characters saved before this feature load with the defaults
        let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
        value.as_object_mut().unwrap().remove("autoshot");
        let old: crate::store::Character = serde_json::from_value(value).unwrap();
        assert_eq!(old.autoshot, Settings::default());
    }

    #[test]
    fn bad_input_is_answered_in_chat_and_changes_nothing() {
        let (mut w, p, ..) = scene();
        w.autoshot_command(p, "radius 99").unwrap();
        w.autoshot_command(p, "fastest").unwrap();
        assert!(!w.players[&p].autoshot_on);
        assert!(w.players[&p].autoshot_choice.is_none());
    }
}
