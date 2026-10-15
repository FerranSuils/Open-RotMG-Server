use super::*;
use crate::content::Object;

impl World {
    pub(super) fn spend_ability(
        &mut self,
        id: i32,
        slot: Slot,
        item: &Object,
        pos: Pos,
    ) -> Result<()> {
        let p = &self.players[&id];
        ensure!(
            slot.slot == 1 && item.slot_type == self.content.get(p.character.class)?.slots[1],
            "ability must be equipped by a compatible class"
        );
        ensure!(
            p.conditions(self.now) & crate::condition::bit(crate::condition::QUIET) == 0,
            "cannot cast while Quiet"
        );
        ensure!(p.character.mp >= item.mana_cost, "not enough mana");
        ensure!(
            p.last_ability
                .is_none_or(|last| self.now >= last + item.cooldown_ms),
            "ability cooldown"
        );
        ensure!(
            pos.x.is_finite() && pos.y.is_finite() && p.pos.distance(pos) <= 15.,
            "invalid ability target or target too far"
        );
        let p = self.players.get_mut(&id).unwrap();
        p.character.mp -= item.mana_cost;
        p.last_ability = Some(self.now);
        tracing::info!(
            player = id,
            item = item.kind,
            mana_cost = item.mana_cost,
            "ability activated"
        );
        Ok(())
    }
    pub(super) fn condition_ability(
        &mut self,
        id: i32,
        slot: Slot,
        item: &Object,
        pos: Pos,
    ) -> Result<()> {
        ensure!(
            item.tier.is_some()
                && matches!(item.slot_type, 12 | 13 | 16)
                && !item.condition_activations.is_empty()
                && item.condition_activations.len() == item.activations,
            "unsupported condition ability layout"
        );
        let origin = self.players[&id].pos;
        let mut applications = Vec::new();
        for (effect, duration, range) in &item.condition_activations {
            for (target, p) in &self.players {
                if p.character.alive
                    && !p.death_pending
                    && !p.departing
                    && !p.paused
                    && p.persistence_failure.is_none()
                    && match range {
                        Some(range) => origin.distance(p.pos) <= *range,
                        None => *target == id,
                    }
                {
                    applications.push((*target, *effect, *duration));
                }
            }
        }
        self.spend_ability(id, slot, item, pos)?;
        for (target, effect, duration) in applications {
            self.apply_condition(target, effect, duration)?;
            tracing::debug!(
                caster = id,
                target,
                effect,
                duration,
                item = item.kind,
                "ability condition applied"
            );
        }
        Ok(())
    }
}
