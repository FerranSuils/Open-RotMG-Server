use super::*;

impl World {
    pub(super) fn damage_enemy(
        &mut self,
        owner: i32,
        target: i32,
        raw: i32,
        piercing: bool,
        effects: &[(u8, u64)],
        bullet: Option<u8>,
    ) -> Result<i32> {
        let Some(e) = self.entities.get(&target) else {
            tracing::debug!(owner, target, "damage target already removed");
            return Ok(0);
        };
        let damage = crate::condition::damage(
            raw,
            self.content.objects[&e.kind].defense,
            e.conditions(self.now),
            piercing,
        );
        let actual = damage.min(e.hp).max(0);
        if damage > 0 {
            for (index, duration) in effects {
                self.apply_condition(target, *index, *duration)?;
            }
        }
        let e = self.entities.get_mut(&target).unwrap();
        if let Some(p) = self.players.get(&owner) {
            *e.damage.entry(p.character.id).or_default() += damage.min(e.hp).max(0);
        }
        e.hp -= damage;
        let kill = e.hp <= 0;
        let quest = e.kind == 0x6b1 || self.content.objects[&e.kind].quest;
        if let Some(p) = self.players.get_mut(&owner) {
            if damage > 0 && bullet.is_some() {
                p.character.hits += 1;
            }
            if kill {
                p.character.kills += 1;
                if quest {
                    p.character.quests += 1;
                }
            }
        }
        let mut o = Writer::new();
        o.i32(target)
            .u8(if damage > 0 { effects.len() as u8 } else { 0 });
        if damage > 0 {
            for (index, _) in effects {
                o.u8(*index);
            }
        }
        o.i16(damage as i16)
            .u8(u8::from(kill))
            .u8(bullet.unwrap_or(0))
            .i32(if bullet.is_some() { owner } else { 0 });
        for p in self.players.values() {
            if p.known.contains(&target) {
                send(p, 47, o.0.clone());
            }
        }
        if kill {
            self.defeat_entity(target)?;
            tracing::info!(entity = target, owner = owner, "enemy defeated");
        }
        Ok(actual)
    }
}
