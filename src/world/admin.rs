//! World-side effects of the moderation and admin chat commands (server::admin checks the rank).
use super::*;
use crate::content::Object;

const GRAVES: std::ops::RangeInclusive<u16> = 0x723..=0x735;

impl Directory {
    fn object_named(&self, name: &str) -> Result<&Object> {
        let kind = name
            .strip_prefix("0x")
            .and_then(|hex| u16::from_str_radix(hex, 16).ok());
        self.content
            .objects
            .values()
            .find(|o| Some(o.kind) == kind || o.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| controller("No object with that name or hex id."))
    }

    pub fn admin_command(&mut self, id: i32, command: &str, rest: &str, now: Instant) -> Result<String> {
        let rest = rest.trim();
        match command {
            "/online" => {
                let names: Vec<_> = self
                    .worlds
                    .values()
                    .flat_map(|w| w.players.values())
                    .filter(|p| p.controller == Controller::Human && p.character.alive)
                    .map(|p| p.character.name.as_str())
                    .collect();
                Ok(format!("{} online: {}", names.len(), names.join(", ")))
            }
            "/where" => {
                ensure!(!rest.is_empty(), controller("Usage: /where <player>"));
                self.worlds
                    .values()
                    .find(|w| w.players.values().any(|p| p.character.name.eq_ignore_ascii_case(rest)))
                    .map(|w| format!("{rest} is in {}.", w.name))
                    .ok_or_else(|| controller("Player not found."))
            }
            "/spawn" => {
                let (count, name) = rest
                    .split_once(char::is_whitespace)
                    .and_then(|(n, name)| n.parse::<usize>().ok().map(|n| (n, name.trim())))
                    .unwrap_or((1, rest));
                ensure!(!name.is_empty() && (1..=20).contains(&count), controller("Usage: /spawn [count 1-20] <object name or 0x id>"));
                let object = self.object_named(name)?;
                ensure!(object.class == "Character", controller("Only characters (enemies, pets) can be spawned."));
                let (kind, name) = (object.kind, object.name.clone());
                let world = self.resolve(Target::Player(id))?;
                let pos = world.players[&id].pos;
                let mut spawned = 0;
                for i in 0..count {
                    let angle = i as f32 * std::f32::consts::TAU / count as f32;
                    let at = Pos { x: pos.x + 2. * angle.cos(), y: pos.y + 2. * angle.sin() };
                    if world.spawn_entity(kind, at, vec![]).is_ok() {
                        spawned += 1;
                    }
                }
                tracing::warn!(player = id, kind, spawned, "admin spawn");
                Ok(format!("Spawned {spawned} x {name}."))
            }
            "/give" => {
                ensure!(!rest.is_empty(), controller("Usage: /give <item name or 0x id>"));
                let object = self.object_named(rest)?;
                ensure!(object.class == "Equipment" || object.consumable, controller("Only items can be given."));
                let (kind, name) = (i32::from(object.kind), object.name.clone());
                let world = self.resolve(Target::Player(id))?;
                let (pos, account) = (world.players[&id].pos, world.players[&id].character.account);
                world.loot_bag(pos, Some(account), vec![kind])?;
                tracing::warn!(player = id, kind, "admin item bag");
                Ok(format!("{name} dropped at your feet."))
            }
            "/max" => {
                let world = self.resolve(Target::Player(id))?;
                let p = world.players.get_mut(&id).context("player missing")?;
                p.character.base = world.content.classes[&p.character.class].caps;
                tracing::warn!(player = id, "admin stats maxed");
                Ok("All stats maxed (saved with your next progress).".into())
            }
            "/killall" => {
                let world = self.resolve(Target::Player(id))?;
                let enemies: Vec<i32> = world
                    .entities
                    .values()
                    .filter(|e| world.content.objects[&e.kind].enemy && e.hp > 0)
                    .map(|e| e.id)
                    .collect();
                for e in &enemies {
                    world.remove_entity(*e);
                }
                tracing::warn!(player = id, removed = enemies.len(), world = %world.name, "admin killall");
                Ok(format!("Removed {} enemies.", enemies.len()))
            }
            "/cleargraves" => {
                let world = self.resolve(Target::Player(id))?;
                let graves: Vec<i32> = world.entities.values().filter(|e| GRAVES.contains(&e.kind)).map(|e| e.id).collect();
                for g in &graves {
                    world.remove_entity(*g);
                }
                Ok(format!("Removed {} gravestones.", graves.len()))
            }
            "/tppos" => {
                let mut parts = rest.split_whitespace().map(str::parse::<f32>);
                let (Some(Ok(x)), Some(Ok(y)), None) = (parts.next(), parts.next(), parts.next()) else {
                    return Err(controller("Usage: /tppos <x> <y>"));
                };
                self.teleport_to(id, id, Pos { x, y }, now)?;
                Ok(format!("Teleported to {x:.1}, {y:.1}."))
            }
            "/tq" => {
                let world = self.resolve(Target::Player(id))?;
                let quest = world.quest_target(id).ok_or_else(|| controller("You have no quest here."))?;
                let pos = world.entities[&quest].pos;
                self.teleport_to(id, id, pos, now)?;
                Ok("Teleported to your quest.".into())
            }
            _ => Err(controller("Unknown admin command.")),
        }
    }
}
