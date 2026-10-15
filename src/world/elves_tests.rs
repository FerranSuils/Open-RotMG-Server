use super::*;
use crate::encounter::{Anchor, Motion};
use std::path::Path;

fn directory(seed: u64) -> Directory {
    Directory::prepared(Path::new("data"), tests::world().content, 16, seed).unwrap()
}
fn player(w: &mut World, pos: Pos) -> i32 {
    let character = tests::ch(w);
    let id = w.enter(character, None).unwrap();
    w.players.get_mut(&id).unwrap().pos = pos;
    id
}
fn live(w: &World, start: usize) -> usize {
    w.population[start..start + 8]
        .iter()
        .filter(|s| s.entity.is_some_and(|id| w.entities.contains_key(&id)))
        .count()
}

#[test]
fn elf_allocations_are_seeded_complete_and_reserve_at_most_32_bodies() {
    for seed in [0, 1, 42, 12351, u64::MAX] {
        let a = directory(seed);
        let b = directory(seed);
        let w = &a.worlds[&-3];
        assert!(!w.elf_camps.is_empty() && w.elf_camps.len() <= 4);
        assert!(w.population.len() <= 344);
        assert_eq!(w.hero_identities().len(), 6);
        let sites = |w: &World| {
            w.population
                .iter()
                .map(|s| (s.kind, s.pos.x.to_bits(), s.pos.y.to_bits()))
                .collect::<Vec<_>>()
        };
        assert_eq!(sites(w), sites(&b.worlds[&-3]));
        for (index, camp) in w.elf_camps.iter().enumerate() {
            assert_eq!(live(w, camp.start), 8);
            for slot in &w.population[camp.start..camp.start + 8] {
                assert_eq!(w.map.cell(slot.pos.x, slot.pos.y).unwrap().terrain, 10);
                assert!(w.walkable(slot.pos));
                let e = &w.entities[&slot.entity.unwrap()];
                assert_eq!(e.elf_camp, Some(index));
                assert!(e.parent.is_none() && e.expires.is_none());
                assert_eq!(e.hp, w.content.objects[&e.kind].hp);
            }
        }
    }
    let ordinary =
        Directory::with_seed(Path::new("data"), tests::world().content, 16, 12351).unwrap();
    assert!(ordinary.worlds[&-3].elf_camps.len() <= 4);
}

#[test]
fn elf_reinforcement_respects_each_role_clock_and_never_duplicates_a_slot() {
    let mut d = directory(12351);
    let w = d.worlds.get_mut(&-3).unwrap();
    let start = w.elf_camps[0].start;
    player(w, w.population[start].pos);
    let slots = [(start + 3, 7000), (start + 7, 8000), (start + 1, 15000)];
    let old: Vec<_> = slots
        .iter()
        .map(|(i, _)| w.population[*i].entity.unwrap())
        .collect();
    for id in &old {
        w.remove_entity(*id);
    }
    w.repopulate().unwrap();
    for (n, (slot, deadline)) in slots.iter().enumerate() {
        w.now = *deadline - 1;
        w.repopulate().unwrap();
        assert!(w.population[*slot].entity.is_none());
        w.now = *deadline;
        w.repopulate().unwrap();
        assert_ne!(w.population[*slot].entity.unwrap(), old[n]);
    }
    w.now = 15000;
    w.repopulate().unwrap();
    assert!(w.population[start + 1].entity.is_some());
    let ids: Vec<_> = w.population[start..start + 8]
        .iter()
        .map(|s| s.entity)
        .collect();
    for _ in 0..100 {
        w.repopulate().unwrap();
    }
    assert_eq!(live(w, start), 8);
    assert_eq!(
        ids,
        w.population[start..start + 8]
            .iter()
            .map(|s| s.entity)
            .collect::<Vec<_>>()
    );
}

#[test]
fn elf_leader_loss_keeps_orphans_and_full_wipe_replacement_waits_for_player_clearance() {
    let mut d = directory(12351);
    let w = d.worlds.get_mut(&-3).unwrap();
    let start = w.elf_camps[0].start;
    let home = w.population[start].pos;
    let p = player(w, home);
    let original: Vec<_> = w.population[start..start + 8]
        .iter()
        .map(|s| s.entity.unwrap())
        .collect();
    w.remove_entity(original[0]);
    w.repopulate().unwrap();
    w.mechanic_expiry();
    assert_eq!(live(w, start), 7);
    assert!(matches!(
        w.elf_movement(original[1], home, 0).unwrap().0,
        Motion::Wander
    ));
    w.now = 60000;
    w.repopulate().unwrap();
    assert!(w.population[start].entity.is_none());
    for id in &original[1..] {
        w.remove_entity(*id);
    }
    w.repopulate().unwrap();
    w.now = 119999;
    w.repopulate().unwrap();
    assert_eq!(live(w, start), 0);
    w.now = 120000;
    w.repopulate().unwrap();
    assert_eq!(live(w, start), 0);
    w.players.get_mut(&p).unwrap().pos = Pos { x: 0.5, y: 0.5 };
    w.now = 121000;
    w.repopulate().unwrap();
    assert_eq!(live(w, start), 8);
    assert!(original.iter().all(|id| !w.entities.contains_key(id)));
    assert_eq!(w.hero_identities().len(), 6);
    assert!(w.critical_deaths.is_empty());
}

#[test]
fn elf_anchors_never_adopt_another_camp_and_wizard_has_real_retreat_windows() {
    let mut d = directory(12351);
    let w = d.worlds.get_mut(&-3).unwrap();
    let a = w.elf_camps[0].start;
    let b = w.elf_camps[1].start;
    let archer = w.population[a + 1].entity.unwrap();
    let leader = w.population[a].entity.unwrap();
    let other = w.entities[&w.population[b].entity.unwrap()].pos;
    let own = w.entities[&leader].pos;
    assert_eq!(
        w.orbit_anchor(archer, Anchor::ElfLeader, other)
            .unwrap()
            .distance(own),
        0.
    );
    let target = Pos {
        x: own.x + 4.,
        y: own.y,
    };
    assert!(matches!(
        w.elf_movement(leader, target, 0).unwrap().0,
        Motion::Wiggle
    ));
    assert!(matches!(
        w.elf_movement(leader, target, 2000).unwrap().0,
        Motion::Chase { .. }
    ));
    assert!(matches!(
        w.elf_movement(leader, target, 4000).unwrap().0,
        Motion::Retreat { .. }
    ));
    for elapsed in [4000, 4500, 5999] {
        assert!(crate::encounter::phased_attacks(0x609, 200, 0., elapsed).is_empty());
    }
    assert_eq!(
        crate::encounter::phased_attacks(0x609, 200, 0., 6000).len(),
        3
    );
    w.entities.get_mut(&leader).unwrap().activated = Some(0);
    assert!(matches!(
        w.elf_movement(
            leader,
            Pos {
                x: own.x + 12.,
                y: own.y
            },
            6000
        )
        .unwrap()
        .0,
        Motion::Stationary
    ));
    assert!(w.entities[&leader].activated.is_none());
    w.remove_entity(leader);
    w.repopulate().unwrap();
    assert!(w.orbit_anchor(archer, Anchor::ElfLeader, other).is_err());
    assert!(matches!(
        w.elf_movement(archer, other, 0).unwrap().0,
        Motion::Wander
    ));
}

#[test]
fn elf_conditions_use_the_shared_npc_pipeline() {
    let mut distances = Vec::new();
    for condition in [
        None,
        Some(crate::condition::SLOWED),
        Some(crate::condition::PARALYZED),
        Some(crate::condition::STUNNED),
    ] {
        let mut d = directory(12351);
        let w = d.worlds.get_mut(&-3).unwrap();
        let start = w.elf_camps[0].start;
        let archer = w.population[start + 1].entity.unwrap();
        let pos = w.entities[&archer].pos;
        player(w, w.population[start].pos);
        if let Some(condition) = condition {
            w.apply_condition(archer, condition, 1000).unwrap();
        }
        w.step();
        distances.push(pos.distance(w.entities[&archer].pos));
        if condition == Some(crate::condition::STUNNED) {
            assert!(w.bullets.iter().all(|b| b.owner != archer));
        }
    }
    assert!(distances[0] > 0.);
    assert!((distances[1] * 2. - distances[0]).abs() < 0.0001);
    assert_eq!(distances[2], 0.);
}

#[test]
fn repeated_elf_camp_wipes_do_not_increase_population_or_reuse_defeated_ids() {
    let mut d = directory(12351);
    let w = d.worlds.get_mut(&-3).unwrap();
    let start = w.elf_camps[0].start;
    let count = w.entities.values().filter(|e| e.elf_camp.is_some()).count();
    let mut defeated = BTreeSet::new();
    for _ in 0..20 {
        let ids: Vec<_> = w.population[start..start + 8]
            .iter()
            .map(|s| s.entity.unwrap())
            .collect();
        for id in ids {
            assert!(defeated.insert(id));
            w.remove_entity(id);
        }
        w.repopulate().unwrap();
        w.now += 60000;
        w.repopulate().unwrap();
        assert_eq!(
            w.entities.values().filter(|e| e.elf_camp.is_some()).count(),
            count
        );
        assert_eq!(live(w, start), 8);
        assert!(w.population.len() <= 344);
    }
}
