//! /realm, /vault and /glands (/godlands): chat shortcuts from the Nexus, vaults and the Realm.
use super::*;

/// Mountains, the God Lands' terrain (content::Object::terrain).
const MOUNTAINS: u8 = 1;
/// No enemy may stand this close to a God Lands landing cell.
const CLEARANCE: f32 = 6.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Travel {
    Realm,
    Vault,
    GodLands,
}

impl Travel {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "/realm" => Some(Self::Realm),
            "/vault" => Some(Self::Vault),
            "/glands" | "/godlands" => Some(Self::GodLands),
            _ => None,
        }
    }
}

impl Directory {
    /// The transfer (account, character, destination, landing) a travel command asks for, or None
    /// when /glands was served inside the Realm by a teleport.
    pub fn travel(
        &mut self,
        id: i32,
        travel: Travel,
        received: Instant,
    ) -> Result<Option<(i32, i32, i32, Option<Pos>)>> {
        self.chat_permit(id, received)?;
        let origin = self.location(id)?;
        let world = &self.worlds[&origin];
        ensure!(
            world.safe || origin == self.endgame.active,
            controller("Leave the dungeon first.")
        );
        let p = &world.players[&id];
        ensure!(
            p.character.alive && p.character.hp > 0 && !p.death_pending && !p.departing,
            controller("You cannot travel right now.")
        );
        let (account, character) = (p.character.account, p.character.id);
        let destination = if travel == Travel::Vault {
            self.instance("vault", Some(account))?
        } else {
            self.initial(-3)?
        };
        ensure!(
            destination != origin || travel == Travel::GodLands,
            controller(if travel == Travel::Vault {
                "You are already in your vault."
            } else {
                "You are already in the Realm."
            })
        );
        if travel != Travel::GodLands {
            return Ok(Some((account, character, destination, None)));
        }
        let realm = self.worlds.get_mut(&destination).unwrap();
        if destination == origin {
            realm.teleport_ready(id)?;
        }
        let landing = realm
            .god_lands_cell()
            .ok_or_else(|| controller("No safe God Lands spot right now; try again."))?;
        if destination == origin {
            self.teleport_to(id, -1, landing, received)?;
            return Ok(None);
        }
        Ok(Some((account, character, destination, Some(landing))))
    }

    /// Makes the character arrive at `pos` when its transfer into `destination` enters the world.
    pub(crate) fn land(&mut self, destination: i32, character: i32, pos: Pos) {
        let pending: BTreeSet<i32> = self
            .grants
            .values()
            .filter(|g| g.destination == destination)
            .map(|g| g.character)
            .collect();
        if let Some(world) = self.worlds.get_mut(&destination) {
            world.arrivals.retain(|c, _| pending.contains(c));
            world.arrivals.insert(character, pos);
        }
    }
}

impl World {
    /// A random free Mountains cell with no enemy within reach.
    pub(super) fn god_lands_cell(&mut self) -> Option<Pos> {
        let width = self.map.width as usize;
        let mountains: Vec<usize> = (self.map.cells.iter().enumerate())
            .filter(|(_, c)| c.terrain == MOUNTAINS)
            .map(|(i, _)| i)
            .collect();
        for _ in 0..64 {
            let i = *mountains.choose(&mut self.rng)?;
            let pos = Pos {
                x: (i % width) as f32 + 0.5,
                y: (i / width) as f32 + 0.5,
            };
            if self.walkable(pos)
                && self.player_position_valid(None, pos)
                && !self
                    .entities_near(pos, CLEARANCE)
                    .any(|e| self.content.objects[&e.kind].enemy)
            {
                return Some(pos);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::tests::{ch, directory};

    fn cell(w: &World, pos: Pos) -> &crate::content::Cell {
        w.map.cell(pos.x, pos.y).unwrap()
    }

    #[test]
    fn commands_parse_case_insensitively() {
        assert_eq!(Travel::parse(" /GLands "), Some(Travel::GodLands));
        assert_eq!(Travel::parse("/godlands"), Some(Travel::GodLands));
        assert_eq!(Travel::parse("/realm"), Some(Travel::Realm));
        assert_eq!(Travel::parse("/vault"), Some(Travel::Vault));
        assert_eq!(Travel::parse("/realm now"), None);
    }

    #[test]
    fn glands_from_the_nexus_lands_on_a_safe_mountains_cell() {
        let mut d = directory();
        let w = d.worlds.get_mut(&-2).unwrap();
        let p = w.enter(ch(w), None).unwrap();
        let received = w.attack_epoch + Duration::from_secs(1);
        let (account, character, realm, landing) =
            d.travel(p, Travel::GodLands, received).unwrap().unwrap();
        assert_eq!((account, character, realm), (1, 1, d.endgame.active));
        let landing = landing.unwrap();
        let w = &d.worlds[&realm];
        assert_eq!(cell(w, landing).terrain, MOUNTAINS);
        assert!(w.walkable(landing) && w.player_position_valid(None, landing));
        assert!(
            !w.entities_near(landing, CLEARANCE)
                .any(|e| w.content.objects[&e.kind].enemy)
        );
        // The arrival is consumed by the transfer's entry and only by it.
        d.issue(account, character, realm).unwrap();
        d.land(realm, character, landing);
        let w = d.worlds.get_mut(&realm).unwrap();
        let presentation = AccountPresentation {
            guild: None,
            credits: 0,
            fame: 0,
            best_fame: BTreeMap::new(),
        };
        let entered = w.enter_at(ch(w), None, None, presentation).unwrap();
        assert_eq!(w.players[&entered].pos.distance(landing), 0.);
        assert!(w.arrivals.is_empty());
    }

    #[test]
    fn glands_inside_the_realm_teleports_and_shares_the_teleport_cooldown() {
        let mut d = directory();
        let realm = d.endgame.active;
        let w = d.worlds.get_mut(&realm).unwrap();
        let p = w.enter(ch(w), None).unwrap();
        let received = w.attack_epoch + Duration::from_secs(1);
        assert!(d.travel(p, Travel::GodLands, received).unwrap().is_none());
        let w = &d.worlds[&realm];
        assert_eq!(cell(w, w.players[&p].pos).terrain, MOUNTAINS);
        assert!(d.teleports.contains_key(&1));
        let error = d
            .travel(p, Travel::GodLands, received + Duration::from_secs(2))
            .unwrap_err();
        assert!(error.to_string().contains("Too soon"), "{error:#}");
        let error = d.travel(p, Travel::Realm, received + Duration::from_secs(4));
        assert!(
            error
                .unwrap_err()
                .to_string()
                .contains("already in the Realm")
        );
    }

    #[test]
    fn realm_and_vault_refuse_from_a_dungeon_but_work_from_the_nexus() {
        let mut d = directory();
        let dungeon = d.instance("pirate-cave", None).unwrap();
        let w = d.worlds.get_mut(&dungeon).unwrap();
        let p = w.enter(ch(w), None).unwrap();
        let received = w.attack_epoch + Duration::from_secs(1);
        for (i, travel) in [Travel::Realm, Travel::Vault, Travel::GodLands]
            .into_iter()
            .enumerate()
        {
            let error = d
                .travel(p, travel, received + Duration::from_secs(i as u64 * 2))
                .unwrap_err();
            assert!(error.to_string().contains("Leave the dungeon first."));
        }
        let w = d.worlds.get_mut(&-2).unwrap();
        let mut other = ch(w);
        other.id = 2;
        other.account = 2;
        let q = w.enter(other, None).unwrap();
        let (_, _, vault, landing) = d.travel(q, Travel::Vault, received).unwrap().unwrap();
        assert_eq!(d.worlds[&vault].owner, Some(2));
        assert!(landing.is_none());
        let (_, _, realm, _) = (d.travel(q, Travel::Realm, received + Duration::from_secs(2)))
            .unwrap()
            .unwrap();
        assert_eq!(realm, d.endgame.active);
    }
}
