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
            let e = self.remove_entity(target);
            let definition = &self.content.objects[&e.kind];
            let reward = (definition.hp as f32 * definition.xp_multiplier / 10.).ceil() as i32;
            for (id, p) in &mut self.players {
                if p.character.alive
                    && !p.death_pending
                    && !p.departing
                    && p.pos.distance(e.pos) < 25.
                {
                    let cap = (crate::store::experience_goal(p.character.level)
                        .expect("validated player level") as f32
                        * if definition.quest { 0.5 } else { 0.1 })
                        as i32;
                    let amount = reward.min(cap);
                    p.experience_queued = p
                        .experience_queued
                        .checked_add(amount)
                        .expect("experience queue overflow");
                    if amount > 0 {
                        tracing::debug!(player = id, enemy = e.kind, amount, "experience queued");
                    }
                }
            }
            if self.name == "Realm"
                && let Some((portal, chance)) = crate::dungeon::portal_drop(e.kind)
                && self.rng.gen_bool(chance)
            {
                self.portal_requests.push((portal, e.pos));
                tracing::info!(
                    enemy = e.kind,
                    entity = target,
                    portal,
                    "defeat earned a dungeon portal"
                );
            }
            if let Err(error) = self.encounter_death(&e) {
                tracing::error!(enemy=e.kind, %error, "defeat mechanic failed");
            }
            if self.encounter
                && let Err(error) = self.drop_loot(&e)
            {
                tracing::error!(enemy=e.kind,%error,"earned encounter loot creation failed");
            }
            tracing::info!(entity = target, owner = owner, "enemy defeated");
        }
        Ok(actual)
    }
}
