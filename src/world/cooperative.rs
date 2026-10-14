use super::*;
use crate::content::{Object, PRIEST};
use std::time::Duration;

#[derive(Debug, Serialize)]
pub struct TeleportReceipt {
    pub player: i32,
    pub character: i32,
    pub world: i32,
    pub target: i32,
    pub position: Pos,
    pub tick: u64,
}

fn ready(p: &Player) -> bool {
    p.character.alive
        && p.character.hp > 0
        && !p.pending
        && !p.death_pending
        && !p.departing
        && p.persistence_failure.is_none()
        && p.tx
            .as_ref()
            .is_none_or(|tx| !*tx.lost.borrow() && !tx.packets.is_closed())
}

fn effect(kind: u8, target: i32, position: Pos) -> Vec<u8> {
    let mut out = Writer::new();
    out.u8(kind)
        .i32(target)
        .f32(position.x)
        .f32(position.y)
        .f32(0.)
        .f32(0.)
        .u32(0xffffffff);
    out.0
}

impl Directory {
    pub fn teleport(&mut self, id: i32, target: i32, received: Instant) -> Result<TeleportReceipt> {
        let scope = self.location(id)?;
        let world = self.resolve(Target::Player(id))?;
        let position = world.teleport_destination(id, target)?;
        let account = world.players[&id].character.account;
        if let Some(last) = self.teleports.get(&account) {
            ensure!(
                received
                    .checked_duration_since(*last)
                    .context("teleport receive clock regressed")?
                    >= Duration::from_secs(10),
                "Too soon to teleport again; wait ten seconds."
            );
        }
        self.teleports
            .retain(|_, last| received.saturating_duration_since(*last) < Duration::from_secs(10));
        ensure!(
            self.teleports.len() < 4096,
            "Teleport rate registry is full."
        );
        let world = self.resolve(Target::Player(id))?;
        let p = world.players.get_mut(&id).unwrap();
        p.pos = position;
        p.movement_credit = 0.78;
        p.last_move_at = world.now;
        let receipt = TeleportReceipt {
            player: id,
            character: p.character.id,
            world: scope,
            target,
            position,
            tick: world.tick,
        };
        world.updates(id, false)?;
        let mut out = Writer::new();
        out.i32(id).f32(position.x).f32(position.y);
        for p in world
            .players
            .values()
            .filter(|p| ready(p) && p.known.contains(&id))
        {
            send(p, wire::GOTO, out.0.clone());
            send(p, wire::SHOW_EFFECT, effect(2, id, position));
        }
        tracing::info!(player=id, account, target, x=position.x, y=position.y, world=%world.name, "player teleported");
        self.teleports.insert(account, received);
        Ok(receipt)
    }
}

impl World {
    pub(super) fn allow_teleport(&self) -> bool {
        matches!(self.name.as_str(), "Nexus" | "Realm" | "Pirate Cave")
    }

    fn teleport_destination(&self, id: i32, target: i32) -> Result<Pos> {
        ensure!(self.allow_teleport(), "Cannot teleport in this world.");
        let p = self
            .players
            .get(&id)
            .context("Teleport sender is no longer in this world.")?;
        ensure!(
            ready(p),
            "Busy, dead, departing or quarantined players cannot teleport."
        );
        ensure!(!p.paused, "Cannot teleport while paused.");
        ensure!(id != target, "You are already at yourself.");
        let q = self
            .players
            .get(&target)
            .context("Teleport target is not a live player in this world.")?;
        ensure!(
            ready(q),
            "Teleport target is busy, dead, departing or quarantined."
        );
        ensure!(!q.paused, "Cannot teleport to a paused player.");
        ensure!(
            p.known.contains(&target),
            "Teleport target has not been observed."
        );
        ensure!(
            self.walkable(q.pos),
            "Teleport target's position is blocked."
        );
        Ok(q.pos)
    }

    pub(super) fn heal_nova(&mut self, id: i32, slot: Slot, item: &Object) -> Result<()> {
        let p = self.players.get(&id).context("healer missing")?;
        ensure!(
            ready(p),
            "Busy, dead, departing or quarantined players cannot heal."
        );
        ensure!(
            p.character.class == PRIEST
                && slot.object == id
                && slot.slot == 1
                && p.character.items[1] == slot.kind
                && slot.kind == item.kind as i32
                && item.slot_type == 4,
            "Healing requires an equipped Priest tome."
        );
        ensure!(
            item.tier.is_some_and(|tier| tier <= 6) && item.activations == 1,
            "Only single-activation tiered healing tomes are implemented."
        );
        ensure!(
            item.heal > 0
                && item.heal_range.is_finite()
                && item.heal_range > 0.
                && item.heal_range <= 20.
                && item.mana_cost > 0
                && item.cooldown_ms > 0,
            "Invalid healing tome definition."
        );
        ensure!(p.character.mp >= item.mana_cost, "Not enough mana to heal.");
        ensure!(
            self.now >= p.last_ability + item.cooldown_ms,
            "Healing ability cooldown."
        );
        let position = p.pos;
        let healing = self
            .players
            .iter()
            .filter(|(_, q)| ready(q) && q.pos.distance(position) < item.heal_range)
            .map(|(id, q)| {
                Ok((
                    *id,
                    (q.character.effective(&self.content)?[0] - q.character.hp)
                        .max(0)
                        .min(item.heal),
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let combat = self.entities.iter().any(|(enemy_id, enemy)| {
            enemy.hp > 0
                && self.content.objects[&enemy.kind].enemy
                && healing.iter().any(|(target, amount)| {
                    *amount > 0
                        && self.players[target].known.contains(enemy_id)
                        && self.players[target].pos.distance(enemy.pos) <= 15.
                })
        });
        let p = self.players.get_mut(&id).unwrap();
        p.character.mp -= item.mana_cost;
        p.last_ability = self.now;
        for (target, amount) in &healing {
            self.players.get_mut(target).unwrap().character.hp += amount;
            if *amount > 0 {
                let mut out = Writer::new();
                out.i32(*target).utf(&format!("+{amount}")).u32(0xff00ff00);
                for p in self
                    .players
                    .values()
                    .filter(|p| ready(p) && p.known.contains(target))
                {
                    send(p, wire::NOTIFICATION, out.0.clone());
                    send(p, wire::SHOW_EFFECT, effect(1, *target, position));
                }
            }
        }
        for p in self
            .players
            .values()
            .filter(|p| ready(p) && p.known.contains(&id))
        {
            send(
                p,
                wire::SHOW_EFFECT,
                effect(
                    5,
                    id,
                    Pos {
                        x: item.heal_range,
                        y: 0.,
                    },
                ),
            );
        }
        tracing::info!(
            player = id,
            character = self.players[&id].character.id,
            world = %self.name,
            time_ms = self.now,
            combat,
            item = item.kind,
            mana = item.mana_cost,
            range = item.heal_range,
            ?healing,
            "Priest healing applied"
        );
        Ok(())
    }
}
