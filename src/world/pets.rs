use super::*;
use crate::pets::{Ability, PETS, PULSE_MS, ability, find, name};

/// The Dragon breathes this weapon's bolt (a plain Fire Wand shot, without the owner's ATT).
const FIRE_WAND: u16 = 0xa04;

fn line(p: &Player, text: &str) {
    let packet = chat::chat_info(text);
    send(p, packet.id, packet.body);
}
/// Confirms a saved /pet change to its owner.
pub(super) fn announce(p: &Player) {
    match p.character.pet_kind.and_then(name) {
        Some(pet) => line(p, &format!("{pet} now follows you.")),
        None => line(p, "Your pet went home."),
    }
}

impl World {
    pub(crate) fn pet_command(&mut self, id: i32, argument: &str) -> Result<()> {
        let p = self.players.get_mut(&id).context("pet owner missing")?;
        if argument.is_empty() {
            let current = match (
                p.character.pet_kind.and_then(name),
                ability(p.character.pet_kind),
            ) {
                (Some(pet), Some((_, text))) => format!("{pet} ({text})"),
                _ => "none".into(),
            };
            line(
                p,
                &format!(
                    "Pet: {current}. Type /pet <number or name> to choose one, /pet none to dismiss it. Skins are cosmetic; each species helps you:"
                ),
            );
            for chunk in PETS.chunks(5) {
                let start = PETS.iter().position(|e| e.0 == chunk[0].0).unwrap() + 1;
                let names: Vec<String> = chunk
                    .iter()
                    .enumerate()
                    .map(|(i, (_, n))| format!("{}) {n}", start + i))
                    .collect();
                let text = ability(Some(chunk[0].0)).map_or("", |(_, text)| text);
                line(p, &format!("{} - {text}", names.join("  ")));
            }
            return Ok(());
        }
        let choice = if argument.eq_ignore_ascii_case("none") {
            None
        } else {
            Some(find(argument).ok_or_else(|| controller("Unknown pet. Type /pet to list them."))?)
        };
        ensure!(
            p.pet_choice.is_none(),
            controller("Already changing your pet; wait a moment.")
        );
        p.pet_choice = Some(choice);
        tracing::info!(player = id, pet = ?choice, "pet chosen");
        Ok(())
    }

    /// Timed pet abilities: Puppy heals, Owl restores MP, Dragon breathes fire.
    pub(super) fn pet_step(&mut self) {
        let now = self.now;
        let due: Vec<(i32, Ability)> = self
            .players
            .iter()
            .filter(|(_, p)| now >= p.pet_at && cooperative::ready(p) && !p.paused)
            .filter_map(|(id, p)| Some((*id, ability(p.character.pet_kind)?.0)))
            .filter(|(_, a)| matches!(a, Ability::Heal(_) | Ability::Mana(_) | Ability::Fireball))
            .collect();
        for (id, ability) in due {
            let p = self.players.get_mut(&id).unwrap();
            p.pet_at = now + PULSE_MS;
            let result = match ability {
                Ability::Heal(amount)
                    if p.conditions(now) & crate::condition::bit(crate::condition::SICK) == 0 =>
                {
                    self.pet_restore(id, 0, amount, 0xff00ff00)
                }
                Ability::Mana(amount) => self.pet_restore(id, 1, amount, 0xff9000ff),
                Ability::Fireball => self.pet_fireball(id),
                _ => Ok(()),
            };
            if let Err(error) = result {
                tracing::warn!(player = id, %error, "pet ability skipped");
            }
        }
    }

    fn pet_restore(&mut self, id: i32, stat: usize, amount: i32, color: u32) -> Result<()> {
        let p = self.players.get_mut(&id).unwrap();
        let cap = p.effective(&self.content, self.now)?[stat];
        let current = if stat == 0 {
            &mut p.character.hp
        } else {
            &mut p.character.mp
        };
        let amount = (cap - *current).clamp(0, amount);
        *current += amount;
        self.consumable_restored(id, amount, color);
        Ok(())
    }

    fn pet_fireball(&mut self, id: i32) -> Result<()> {
        let spec = self.content.get(FIRE_WAND)?.projectiles[&0].clone();
        let p = &self.players[&id];
        let range = spec.speed * spec.lifetime / 10_000.;
        let Some(aim) = self
            .actors_near(p.pos, range)
            .filter(|e| e.pos.distance(p.pos) <= range && self.shootable(p, e, spec.passes_cover))
            .map(|e| e.pos)
            .min_by(|a, b| a.distance(p.pos).total_cmp(&b.distance(p.pos)))
        else {
            return Ok(());
        };
        if self.bullets.len() >= 2048 {
            return Ok(());
        }
        let angle = (aim.y - p.pos.y).atan2(aim.x - p.pos.x);
        self.ally_bullet(id, FIRE_WAND, &spec, p.pos, angle, 1.);
        tracing::debug!(player = id, "pet fireball breathed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::tests::{arena, ch};

    #[test]
    fn chosen_pet_is_saved_spawns_follows_and_can_be_dismissed() {
        let mut w = arena(0);
        let p = w.enter(ch(&w), None).unwrap();
        assert!(w.pet_command(p, "unicorn").is_err());
        w.pet_command(p, "frost fox").unwrap();
        let (pid, snapshot) = w
            .pending_progress_where(|_, _| true)
            .into_iter()
            .next()
            .unwrap();
        let fox = find("Frost Fox").unwrap();
        assert_eq!(
            snapshot.pet_kind,
            Some(fox),
            "the choice is persisted with the character"
        );
        w.finish_progress(pid, snapshot).unwrap();
        let pet = w
            .entities
            .values()
            .find(|e| e.kind == fox)
            .expect("pet spawned")
            .id;
        let target = Pos {
            x: w.players[&p].pos.x + 6.,
            y: w.players[&p].pos.y,
        };
        w.players.get_mut(&p).unwrap().pos = target;
        for _ in 0..80 {
            w.step();
        }
        assert!(
            w.entities[&pet].pos.distance(target) < 2.,
            "the pet follows its owner"
        );
        w.pet_command(p, "none").unwrap();
        let (pid, snapshot) = w
            .pending_progress_where(|_, _| true)
            .into_iter()
            .next()
            .unwrap();
        w.finish_progress(pid, snapshot).unwrap();
        assert!(!w.entities.contains_key(&pet), "dismissed pets leave");
    }

    fn with_pet(name: &str) -> (World, i32) {
        let mut w = arena(0);
        let mut c = ch(&w);
        c.pet_kind = find(name);
        let p = w.enter(c, None).unwrap();
        (w, p)
    }

    #[test]
    fn stat_pets_raise_effective_stats_and_fox_adds_experience() {
        let (w, plain) = with_pet("none");
        let base = w.players[&plain].effective(&w.content, w.now).unwrap();
        for (pet, index, bonus) in [
            ("Classic Cat", 5, 4),
            ("Frost Slime", 0, 40),
            ("Ember Bunny", 4, 4),
            ("Gilded Turtle", 3, 4),
        ] {
            let (w, p) = with_pet(pet);
            let values = w.players[&p].effective(&w.content, w.now).unwrap();
            assert_eq!(values[index], base[index] + bonus, "{pet}");
            assert_eq!(
                values.iter().sum::<i32>(),
                base.iter().sum::<i32>() + bonus,
                "{pet} only raises its own stat"
            );
        }
        assert_eq!(crate::pets::experience(find("Shadow Fox"), 100), 110);
    }

    #[test]
    fn puppy_heals_and_owl_restores_mana_every_few_seconds() {
        // Natural regeneration is the same with and without a pet, so compare against a plain owner.
        for (pet, stat, amount) in [("Classic Puppy", 0, 20), ("Classic Owl", 1, 15)] {
            let mut worlds = [with_pet("none"), with_pet(pet)];
            let cap = worlds[0].0.players[&worlds[0].1]
                .effective(&worlds[0].0.content, 0)
                .unwrap()[stat];
            let gain = |worlds: &mut [(World, i32); 2], steps: usize| {
                let mut levels = [0; 2];
                for (i, (w, p)) in worlds.iter_mut().enumerate() {
                    for _ in 0..steps {
                        w.step();
                    }
                    let c = &w.players[p].character;
                    levels[i] = if stat == 0 { c.hp } else { c.mp };
                }
                levels[1] - levels[0]
            };
            for (w, p) in &mut worlds {
                let c = &mut w.players.get_mut(p).unwrap().character;
                *(if stat == 0 { &mut c.hp } else { &mut c.mp }) = cap - 80;
            }
            assert_eq!(gain(&mut worlds, 1), amount, "{pet} restores at once");
            assert_eq!(
                gain(&mut worlds, 20),
                amount,
                "{pet} waits for the next pulse"
            );
            assert_eq!(gain(&mut worlds, 40), 2 * amount, "{pet} pulses every 3 s");
            gain(&mut worlds, 400);
            let c = &worlds[1].0.players[&worlds[1].1].character;
            assert_eq!(
                if stat == 0 { c.hp } else { c.mp },
                cap,
                "{pet} stops at the maximum"
            );
        }
    }

    #[test]
    fn dragon_breathes_fire_at_the_nearest_enemy() {
        let (mut w, p) = with_pet("Ember Dragon");
        let at = w.players[&p].pos;
        let crab = *w
            .content
            .objects
            .iter()
            .find(|(_, o)| o.name == "Shore Crab")
            .unwrap()
            .0;
        let crab = w
            .spawn_entity(
                crab,
                Pos {
                    x: at.x,
                    y: at.y + 3.,
                },
                vec![],
            )
            .unwrap();
        w.entities.get_mut(&crab).unwrap().condition |=
            crate::condition::bit(crate::condition::PARALYZED);
        let mut angle = None;
        for _ in 0..40 {
            w.apply_condition(p, crate::condition::INVULNERABLE, 200)
                .unwrap();
            w.step();
            if let Some(b) = w.bullets.iter().find(|b| b.owner == p && b.id >= 128) {
                angle = Some(b.angle);
                break;
            }
        }
        let angle = angle.expect("the dragon fired");
        assert!((angle - std::f32::consts::FRAC_PI_2).abs() < 0.2, "{angle}");
    }
}
