use super::*;
use crate::endgame::{CASTLE, HEROES, RealmCycle, RealmStage};

pub(super) struct RealmDirectory {
    pub active: i32,
    cycles: BTreeMap<i32, RealmCycle>,
    forced: BTreeMap<i32, (i32, i32)>,
    retry: BTreeMap<i32, u64>,
}
impl Default for RealmDirectory {
    fn default() -> Self {
        Self {
            active: -3,
            cycles: BTreeMap::new(),
            forced: BTreeMap::new(),
            retry: BTreeMap::new(),
        }
    }
}
pub struct ForcedTransfer {
    pub player: i32,
    pub output: Output,
    pub destination: i32,
    pub key_time: i32,
    pub key: Vec<u8>,
    pub name: String,
}
impl ForcedTransfer {
    pub fn packet(&self) -> Packet {
        let mut o = Writer::new();
        o.utf(&self.name)
            .utf("")
            .i32(2050)
            .i32(self.destination)
            .i32(self.key_time)
            .i16(self.key.len() as i16);
        o.0.extend_from_slice(&self.key);
        Packet { id: 39, body: o.0 }
    }
}
impl Directory {
    pub(super) fn bind_realm_heroes(&mut self, origin: i32, heroes: BTreeSet<i32>) -> Result<()> {
        let world = self.worlds.get(&origin).context("closure Realm missing")?;
        ensure!(
            world.name == "Realm" && world.admission_open,
            "closure roster requires an open Realm"
        );
        ensure!(
            !self.endgame.cycles.contains_key(&origin),
            "Realm roster already bound"
        );
        for id in &heroes {
            let hero = world.entities.get(id).context("closure hero missing")?;
            ensure!(
                HEROES.contains(&hero.kind) && hero.hp > 0,
                "closure identity is not a living qualifying hero"
            );
        }
        let cycle = RealmCycle::new(heroes)?;
        tracing::info!(
            origin,
            heroes = cycle.remaining.len(),
            "finite Realm closure roster bound"
        );
        self.endgame.cycles.insert(origin, cycle);
        Ok(())
    }
    pub(super) fn endgame_step(&mut self) {
        let now = self.tick * 50;
        for (origin, world) in &mut self.worlds {
            let deaths = std::mem::take(&mut world.critical_deaths);
            let Some(cycle) = self.endgame.cycles.get_mut(origin) else {
                continue;
            };
            for id in deaths {
                if cycle.defeated(id, now) {
                    tracing::info!(
                        origin,
                        hero = id,
                        remaining = cycle.remaining.len(),
                        "qualifying Realm hero defeated"
                    );
                    if !cycle.admits() {
                        world.admission_open = false;
                        world.oryx_notice(
                            "My minions have failed me! This Realm will close in one minute.",
                        );
                        self.grants.retain(|_, g| g.destination != *origin);
                    }
                }
            }
            if cycle.advance(now) && matches!(cycle.stage, RealmStage::Sealed { .. }) {
                world.oryx_notice(
                    "I have closed this Realm! I will bring you to my Castle in 22 seconds.",
                );
            }
        }
        let transferring: Vec<_> = self
            .endgame
            .cycles
            .iter()
            .filter(|(id, c)| {
                c.stage == RealmStage::Transferring
                    && self.endgame.active == **id
                    && self.endgame.retry.get(id).is_none_or(|t| self.tick >= *t)
            })
            .map(|(id, _)| *id)
            .collect();
        for origin in transferring {
            if let Err(error) = self.replace_realm(origin) {
                self.endgame.retry.insert(origin, self.tick + 100);
                tracing::error!(origin, %error, "Realm transfer/replacement withheld; retry in five seconds");
            }
        }
        self.endgame.forced.retain(|player, (origin, _)| {
            self.worlds
                .get(origin)
                .is_some_and(|w| w.players.contains_key(player))
        });
        let retired: Vec<_> = self
            .endgame
            .cycles
            .iter()
            .filter(|(id, c)| {
                **id != self.endgame.active
                    && c.stage == RealmStage::Transferring
                    && self.worlds.get(id).is_some_and(|w| w.players.is_empty())
            })
            .map(|(id, _)| *id)
            .collect();
        for origin in retired {
            self.portals.retain(|_, p| p.origin != origin);
            self.grants.retain(|_, g| g.destination != origin);
            self.worlds.remove(&origin);
            self.idle.remove(&origin);
            self.endgame.cycles.remove(&origin);
            self.endgame.retry.remove(&origin);
            tracing::info!(origin, "empty retired Realm removed");
        }
    }
    fn replace_realm(&mut self, origin: i32) -> Result<()> {
        ensure!(
            self.worlds.len() + 2 <= 12,
            "no room for collective Castle and replacement Realm"
        );
        let castle = self.make(CASTLE, "Oryx's Castle", false, None)?;
        let realm = self.make("realm-shore", "Realm", false, None)?;
        let castle_id = self.next_world;
        let realm_id = castle_id
            .checked_add(1)
            .context("world identity exhausted")?;
        let next = realm_id
            .checked_add(1)
            .context("world identity exhausted")?;
        ensure!(
            !self.worlds.contains_key(&castle_id) && !self.worlds.contains_key(&realm_id),
            "world identity collision"
        );
        let players: Vec<_> = self.worlds[&origin]
            .players
            .iter()
            .filter(|(_, p)| {
                p.character.alive
                    && p.character.hp > 0
                    && !p.death_pending
                    && !p.departing
                    && p.persistence_failure.is_none()
            })
            .map(|(id, _)| *id)
            .collect();
        self.worlds.insert(castle_id, castle);
        self.worlds.insert(realm_id, realm);
        self.next_world = next;
        self.idle.insert(castle_id, self.tick);
        self.endgame.active = realm_id;
        self.endgame.retry.remove(&origin);
        for player in &players {
            self.endgame.forced.insert(*player, (origin, castle_id));
        }
        self.worlds
            .get_mut(&origin)
            .unwrap()
            .oryx_notice("Your fate awaits in my Castle!");
        tracing::info!(
            origin,
            castle = castle_id,
            replacement = realm_id,
            recipients = players.len(),
            "Realm closed; collective Castle route and fresh Realm installed"
        );
        Ok(())
    }
    pub fn forced_transfer_candidates(&self) -> Vec<i32> {
        self.endgame.forced.keys().copied().collect()
    }
    pub fn forced_transfer(&mut self, player: i32) -> Result<Option<ForcedTransfer>> {
        let Some(&(origin, destination)) = self.endgame.forced.get(&player) else {
            return Ok(None);
        };
        let Some(p) = self
            .worlds
            .get(&origin)
            .and_then(|w| w.players.get(&player))
        else {
            self.endgame.forced.remove(&player);
            return Ok(None);
        };
        if !p.character.alive
            || p.character.hp <= 0
            || p.death_pending
            || p.departing
            || p.persistence_failure.is_some()
        {
            self.endgame.forced.remove(&player);
            return Ok(None);
        }
        if p.pending || p.experience_queued > 0 || p.caves_queued > 0 {
            return Ok(None);
        }
        let output =
            p.tx.clone()
                .context("collective transfer recipient lacks an owned RECONNECT output")?;
        let (account, character) = (p.character.account, p.character.id);
        let (key_time, key, name) = self.issue(account, character, destination)?;
        let world = self.worlds.get_mut(&origin).unwrap();
        world.cancel_trade(player, "Trade cancelled by Realm closure.");
        let p = world.players.get_mut(&player).unwrap();
        // Freeze in the same actor job as grant issuance, before an enemy can kill the departing avatar.
        p.departing = true;
        p.tx = None;
        p.bot_goal = None;
        p.bot_aim = None;
        self.endgame.forced.remove(&player);
        Ok(Some(ForcedTransfer {
            player,
            output,
            destination,
            key_time,
            key,
            name,
        }))
    }
}
impl World {
    fn oryx_notice(&self, text: &str) {
        let packet = chat::text_packet("Oryx the Mad God", -1, 0, text);
        for p in self
            .players
            .values()
            .filter(|p| !p.departing && p.persistence_failure.is_none())
        {
            send(p, packet.id, packet.body.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn recipient(d: &mut Directory, account: i32) -> (i32, mpsc::Receiver<Packet>) {
        let w = d.worlds.get_mut(&-3).unwrap();
        let mut ch = super::super::tests::ch(w);
        ch.account = account;
        ch.id = account;
        let (packets, mut rx) = mpsc::channel(64);
        let (lost, _) = watch::channel(false);
        let player = w.enter(ch, Some(Output { packets, lost })).unwrap();
        while rx.try_recv().is_ok() {}
        (player, rx)
    }
    fn arm(d: &mut Directory) -> [i32; 2] {
        let w = d.worlds.get_mut(&-3).unwrap();
        let pos = Pos {
            x: w.map.spawns[0].0,
            y: w.map.spawns[0].1,
        };
        let a = w.spawn_entity(HEROES[0], pos, vec![]).unwrap();
        let b = w.spawn_entity(HEROES[1], pos, vec![]).unwrap();
        d.bind_realm_heroes(-3, BTreeSet::from([a, b])).unwrap();
        [a, b]
    }
    fn defeat(d: &mut Directory, heroes: [i32; 2]) {
        let w = d.worlds.get_mut(&-3).unwrap();
        for id in heroes {
            w.remove_entity(id);
            w.critical_deaths.push(id);
        }
        d.endgame_step();
    }
    #[test]
    fn finite_closure_seals_both_admission_paths_and_replaces_without_erasing_owned_players() {
        let mut d = super::super::tests::directory();
        let (a, mut ar) = recipient(&mut d, 1);
        let (b, _) = recipient(&mut d, 2);
        let (time, key, _) = d.issue(3, 3, -3).unwrap();
        let heroes = arm(&mut d);
        assert!(d.bind_realm_heroes(-3, BTreeSet::from(heroes)).is_err());
        defeat(&mut d, heroes);
        assert!(!d.worlds[&-3].admission_open);
        assert!(d.initial(-3).is_err());
        assert!(d.consume(3, -3, time, key).is_err());
        let ch = super::super::tests::ch(&d.worlds[&-3]);
        assert!(d.worlds.get_mut(&-3).unwrap().enter(ch, None).is_err());
        assert_eq!(ar.try_recv().unwrap().id, wire::TEXT);
        d.tick = 1200;
        d.endgame_step();
        assert!(matches!(
            d.endgame.cycles[&-3].stage,
            RealmStage::Sealed { .. }
        ));
        d.tick = 1640;
        d.endgame_step();
        let realm = d.initial(-3).unwrap();
        assert!(realm > 0 && realm != -3);
        let a_output = d.worlds[&-3].players[&a]
            .tx
            .as_ref()
            .unwrap()
            .packets
            .clone();
        let b_output = d.worlds[&-3].players[&b]
            .tx
            .as_ref()
            .unwrap()
            .packets
            .clone();
        assert_eq!(d.location(a).unwrap(), -3);
        assert_eq!(d.location(b).unwrap(), -3);
        let transfers: Vec<_> = d
            .forced_transfer_candidates()
            .into_iter()
            .filter_map(|id| d.forced_transfer(id).unwrap())
            .collect();
        assert_eq!(transfers.len(), 2);
        assert!(
            transfers
                .iter()
                .find(|t| t.player == a)
                .unwrap()
                .output
                .packets
                .same_channel(&a_output)
        );
        assert!(
            transfers
                .iter()
                .find(|t| t.player == b)
                .unwrap()
                .output
                .packets
                .same_channel(&b_output)
        );
        assert_eq!(transfers[0].destination, transfers[1].destination);
        assert_eq!(d.worlds[&transfers[0].destination].name, "Oryx's Castle");
        assert!(transfers.iter().all(|t| t.packet().id == 39));
        assert!(
            d.worlds[&-3]
                .players
                .values()
                .all(|p| p.departing && p.tx.is_none())
        );
        assert!(d.forced_transfer_candidates().is_empty());
        assert_eq!(d.worlds[&-3].players.len(), 2);
    }
    #[test]
    fn pending_save_dead_quarantined_and_disconnected_recipients_do_not_duplicate_transfer() {
        let mut d = super::super::tests::directory();
        let (pending, _) = recipient(&mut d, 1);
        let (dead, _) = recipient(&mut d, 2);
        let (failed, _) = recipient(&mut d, 3);
        let (gone, _) = recipient(&mut d, 4);
        let heroes = arm(&mut d);
        defeat(&mut d, heroes);
        d.tick = 1200;
        d.endgame_step();
        d.tick = 1640;
        d.endgame_step();
        let w = d.worlds.get_mut(&-3).unwrap();
        w.players.get_mut(&pending).unwrap().pending = true;
        w.players.get_mut(&dead).unwrap().character.hp = 0;
        w.poison(failed, "fixture persistence failure".into())
            .unwrap();
        w.players.remove(&gone);
        assert!(
            d.forced_transfer_candidates()
                .into_iter()
                .all(|id| d.forced_transfer(id).unwrap().is_none())
        );
        assert!(d.worlds.contains_key(&-3));
        d.worlds
            .get_mut(&-3)
            .unwrap()
            .players
            .get_mut(&pending)
            .unwrap()
            .pending = false;
        let transfers: Vec<_> = d
            .forced_transfer_candidates()
            .into_iter()
            .filter_map(|id| d.forced_transfer(id).unwrap())
            .collect();
        assert_eq!(transfers.len(), 1);
        assert_eq!(transfers[0].player, pending);
        assert!(d.forced_transfer_candidates().is_empty());
    }
}
