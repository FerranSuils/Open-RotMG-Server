use super::*;
use crate::{
    condition,
    dungeon::tomb::{self, ACTIVE, ANCHOR, BOSSES, INACTIVE, MUMMY, SCARAB},
    encounter::Motion,
};

impl World {
    pub(super) fn tomb_mechanics(&mut self, id: i32, target: Option<Pos>, elapsed: u64) -> Result<()> {
        let e = &self.entities[&id];
        let (kind, hp, pos) = (e.kind, e.hp, e.pos);
        if kind == INACTIVE {
            let guarded = self.actors().any(|p| {
                matches!(p.kind, tomb::PRIEST | tomb::PRIESTESS) && p.pos.distance(pos) < 14.
            });
            if !guarded {
                self.transform_fauna(id, ACTIVE)?;
                self.broadcast_text("Sarcophagus", "The seal is broken!");
            }
            return Ok(());
        }
        if let Some(slot) = tomb::STATUES.iter().position(|s| *s == kind) {
            if !self.actors().any(|s| matches!(s.kind, INACTIVE | ACTIVE)) {
                self.transform_fauna(id, BOSSES[slot])?;
            }
            return Ok(());
        }
        if kind == MUMMY {
            if target.is_some() && self.now >= e.next_aux {
                let brood = self
                    .actors()
                    .filter(|s| s.kind == SCARAB && s.parent == Some(id))
                    .count();
                self.spawn_children(id, SCARAB, (brood + 1).min(5), 1.)?;
                self.entities.get_mut(&id).unwrap().next_aux = self.now + 3000;
            }
            return Ok(());
        }
        if !BOSSES.contains(&kind) {
            return Ok(());
        }
        let stage = tomb::stage(kind, hp, self.content.objects[&kind].hp);
        let e = self.entities.get_mut(&id).unwrap();
        let entered = e.stage != Some(stage);
        if entered {
            e.stage = Some(stage);
            e.next_aux = 0;
            if tomb::shielded(kind, stage) {
                e.reaction_until = self.now + 3000;
            }
            tracing::info!(entity = id, kind, stage, "Tomb boss phase entered");
        }
        let bits = condition::bit(condition::INVULNERABLE) | condition::bit(condition::ARMORED);
        e.condition &= !bits;
        if tomb::armored(kind, stage) {
            e.condition |= condition::bit(condition::ARMORED);
        }
        if e.reaction_until > self.now {
            e.condition |= condition::bit(condition::INVULNERABLE);
        }
        if entered && let Some(line) = tomb::taunt(kind, stage) {
            self.broadcast_text(tomb::boss_name(kind), line);
        }
        if self.now >= self.entities[&id].next_aux {
            for (artifact, maximum) in tomb::artifacts(kind, stage) {
                self.spawn_children(id, *artifact, *maximum, 2.)?;
            }
            if kind == tomb::ATTACKER && stage >= 3 && (elapsed / 1000).is_multiple_of(10) {
                self.spawn_children(id, SCARAB, 3, 2.)?;
            }
            self.entities.get_mut(&id).unwrap().next_aux = self.now + 1000;
        }
        Ok(())
    }
    pub(super) fn tomb_movement(&self, id: i32) -> (Motion, f32) {
        let e = &self.entities[&id];
        if BOSSES.contains(&e.kind) {
            return tomb::boss_motion(e.kind, e.stage.unwrap_or(0));
        }
        let p = tomb::profile(e.kind).expect("validated Tomb profile");
        if let Some(guard) = tomb::artifact_guard(e.kind)
            && !self.actors().any(|b| b.kind == guard)
        {
            return (
                Motion::Chase {
                    sight: 20.,
                    stop: 1.,
                },
                p.speed,
            );
        }
        (p.motion, p.speed)
    }
    pub(super) fn tomb_death(&mut self, defeated: &Entity) -> Result<()> {
        if !BOSSES.contains(&defeated.kind) || self.actors().any(|b| BOSSES.contains(&b.kind)) {
            return Ok(());
        }
        let leftovers: Vec<_> = self
            .actors()
            .filter(|e| e.kind == ANCHOR || e.kind == SCARAB || tomb::ARTIFACTS.contains(&e.kind))
            .map(|e| e.id)
            .collect();
        for id in leftovers {
            self.remove_entity(id);
        }
        self.broadcast_text("Oryx the Mad God", "The ancients have fallen. This tomb is yours.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dungeon::tomb::{DEFENDER, PRIEST, PRIESTESS, STATUES};
    use std::path::Path;

    fn tomb() -> (World, i32) {
        let content = Arc::new(Content::load(Path::new("data")).unwrap());
        let map = Arc::new(tomb::generate(&content, 7).unwrap());
        let mut w = World::new(content, map).unwrap();
        w.name = tomb::NAME.into();
        w.encounter = true;
        let p = w.enter(super::super::tests::ch(&w), None).unwrap();
        (w, p)
    }
    fn ids(w: &World, kind: u16) -> Vec<i32> {
        w.actors().filter(|e| e.kind == kind).map(|e| e.id).collect()
    }
    fn slay(w: &mut World, p: i32, id: i32) {
        let hp = w.entities[&id].hp;
        assert!(w.damage_enemy(p, id, hp + 1000, true, &[], None).unwrap() > 0);
    }

    #[test]
    fn priests_open_sarcophagi_then_statues_wake_and_the_last_boss_clears() {
        let (mut w, p) = tomb();
        let sarcophagus = ids(&w, INACTIVE)[0];
        w.tomb_mechanics(sarcophagus, None, 0).unwrap();
        assert_eq!(ids(&w, INACTIVE).len(), 3, "guarded sarcophagus opened early");
        assert_eq!(w.damage_enemy(p, sarcophagus, 500, true, &[], None).unwrap(), 0);
        for priest in ids(&w, PRIEST).into_iter().chain(ids(&w, PRIESTESS)) {
            slay(&mut w, p, priest);
        }
        for id in ids(&w, INACTIVE) {
            w.tomb_mechanics(id, None, 0).unwrap();
        }
        assert_eq!((ids(&w, INACTIVE).len(), ids(&w, ACTIVE).len()), (0, 3));
        let statue = ids(&w, STATUES[2])[0];
        w.tomb_mechanics(statue, None, 0).unwrap();
        assert!(ids(&w, DEFENDER).is_empty(), "statue woke before the treasure was taken");
        for id in ids(&w, ACTIVE) {
            slay(&mut w, p, id);
        }
        for statue in STATUES {
            let id = ids(&w, statue)[0];
            w.tomb_mechanics(id, None, 0).unwrap();
        }
        let bosses: Vec<_> = BOSSES.iter().map(|k| ids(&w, *k)[0]).collect();
        let bes = bosses[2];
        w.tomb_mechanics(bes, None, 0).unwrap();
        assert_eq!(w.entities[&bes].stage, Some(0));
        assert_ne!(w.entities[&bes].condition & condition::bit(condition::ARMORED), 0);
        let max = w.content.objects[&DEFENDER].hp;
        w.entities.get_mut(&bes).unwrap().hp = max * 3 / 10;
        w.tomb_mechanics(bes, None, 0).unwrap();
        assert_eq!(w.entities[&bes].stage, Some(5));
        assert_ne!(w.entities[&bes].condition & condition::bit(condition::INVULNERABLE), 0);
        assert_eq!(ids(&w, 0xd22).len() + ids(&w, 0xd23).len(), 2, "artifacts");
        w.now += 3050;
        w.tomb_mechanics(bes, None, 3050).unwrap();
        assert_eq!(w.entities[&bes].condition & condition::bit(condition::INVULNERABLE), 0);
        assert_eq!(w.tomb_movement(bes).0, tomb::boss_motion(DEFENDER, 5).0);
        for boss in &bosses[..2] {
            slay(&mut w, p, *boss);
        }
        assert!(!w.cleared);
        slay(&mut w, p, bes);
        assert!(w.cleared);
        assert_eq!(w.entities[&w.clear_exit.unwrap()].kind, 0x704);
        assert!(ids(&w, ANCHOR).is_empty() && ids(&w, 0xd22).is_empty());
    }
}
