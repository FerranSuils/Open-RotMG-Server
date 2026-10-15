use super::{tests, *};

fn archer(quiver: i32) -> (World, i32) {
    let mut w = tests::world();
    w.encounter = true;
    for id in w.entities.keys().copied().collect::<Vec<_>>() {
        w.remove_entity(id);
    }
    for cell in &mut Arc::make_mut(&mut w.map).cells {
        cell.ground = 0x61;
    }
    let mut ch = tests::ch(&w);
    ch.class = crate::content::ARCHER;
    ch.base = w.content.classes[&ch.class].caps;
    ch.hp = ch.base[0];
    ch.mp = ch.base[1];
    ch.items = w.content.get(ch.class).unwrap().equipment.clone();
    ch.items[1] = quiver;
    let id = w.enter(ch, None).unwrap();
    w.players.get_mut(&id).unwrap().controller = Controller::Human;
    w.players.get_mut(&id).unwrap().seed = 12351;
    w.players.get_mut(&id).unwrap().next_bullet = 0;
    w.now = 1000;
    (w, id)
}
fn cast(w: &mut World, id: i32, pos: Pos) -> Result<Option<InventoryPlan>> {
    let kind = w.players[&id].character.items[1];
    let time = w.now as i32;
    let result = w.native_use_item(
        id,
        time,
        Slot {
            object: id,
            slot: 1,
            kind,
        },
        pos,
    )?;
    let origin = w.players[&id].pos;
    let angle = (pos.y - origin.y).atan2(pos.x - origin.x);
    let start = Pos {
        x: origin.x + 0.3 * angle.cos(),
        y: origin.y + 0.3 * angle.sin(),
    };
    w.native_shoot_received(
        id,
        (time, Instant::now()),
        w.players[&id].next_bullet,
        kind as u16,
        start,
        angle,
    )?;
    Ok(result)
}

#[test]
fn ordinary_quivers_match_native_projectiles_mana_and_shared_damage_rng() {
    for kind in [0xa61, 0xa62, 0xa63, 0xade, 0xa64, 0xa65, 0xb28] {
        let (mut w, id) = archer(kind);
        let pos = w.players[&id].pos;
        let target = Pos {
            x: pos.x + 4.,
            y: pos.y + 3.,
        };
        let item = w.content.get(kind as u16).unwrap().clone();
        let mp = w.players[&id].character.mp;
        let weapon_seed = w.players[&id].seed;
        assert!(cast(&mut w, id, target).unwrap().is_none());
        assert_eq!(w.players[&id].character.mp, mp - item.mana_cost);
        assert_eq!(w.players[&id].character.shots, 1);
        assert_eq!(
            w.players[&id].seed,
            ((u64::from(weapon_seed) * 16807) % 2147483647) as u32
        );
        assert_eq!(w.bullets.len(), 1);
        let b = &w.bullets[0];
        assert_eq!(b.owner, id);
        assert!((b.start.distance(pos) - 0.3).abs() < 0.00001);
        assert_eq!(b.pos.distance(b.start), 0.);
        assert!((b.angle - 3_f32.atan2(4.)).abs() < 0.00001);
        assert!((item.projectiles[&0].min..=item.projectiles[&0].max).contains(&b.damage));
        assert_eq!(b.spec.effects, item.projectiles[&0].effects);
        assert!(b.spec.multi_hit && b.spec.passes_cover);
    }
}

#[test]
fn quiver_failures_do_not_spend_mana_or_create_projectiles() {
    for rejection in 0..7 {
        let (mut w, id) = archer(0xa65);
        let pos = w.players[&id].pos;
        let mut target = Pos {
            x: pos.x + 4.,
            y: pos.y,
        };
        match rejection {
            0 => w.players.get_mut(&id).unwrap().character.mp = 74,
            1 => {
                w.apply_condition(id, crate::condition::QUIET, 1000)
                    .unwrap();
            }
            2 => w.players.get_mut(&id).unwrap().last_ability = w.now,
            3 => w.players.get_mut(&id).unwrap().character.class = crate::content::WIZARD,
            4 => target.x = f32::NAN,
            5 => target.x += 20.,
            6 => w.players.get_mut(&id).unwrap().character.items[1] = 0xc07,
            _ => unreachable!(),
        }
        let p = &w.players[&id];
        let before = (p.character.mp, p.character.shots, p.last_ability);
        assert!(cast(&mut w, id, target).is_err());
        let p = &w.players[&id];
        assert_eq!((p.character.mp, p.character.shots, p.last_ability), before);
        assert!(w.bullets.is_empty());
    }
}

#[test]
fn golden_quiver_paralyzes_on_collision_then_enemy_resumes_after_xml_duration() {
    let (mut w, id) = archer(0xa65);
    let origin = w.players[&id].pos;
    let target = w
        .spawn_entity(
            0x646,
            Pos {
                x: origin.x + 4.,
                y: origin.y,
            },
            vec![],
        )
        .unwrap();
    let aim = w.entities[&target].pos;
    cast(&mut w, id, aim).unwrap();
    let paralyzed = crate::condition::bit(crate::condition::PARALYZED);
    for _ in 0..20 {
        w.step();
        if w.entities[&target].conditions(w.now) & paralyzed != 0 {
            break;
        }
    }
    assert!(w.entities[&target].hp < 850);
    assert_ne!(w.entities[&target].conditions(w.now) & paralyzed, 0);
    let hit_time = w.now;
    let held = w.entities[&target].pos;
    for _ in 0..10 {
        w.step();
    }
    assert_eq!(w.entities[&target].pos.distance(held), 0.);
    w.now = hit_time + 3000;
    assert_eq!(w.entities[&target].conditions(w.now) & paralyzed, 0);
    w.step();
    assert!(w.entities[&target].pos.distance(held) > 0.);
}

#[test]
fn missed_quiver_cannot_damage_or_paralyze_an_enemy() {
    let (mut w, id) = archer(0xa65);
    let pos = w.players[&id].pos;
    let target = w
        .spawn_entity(
            0x646,
            Pos {
                x: pos.x + 4.,
                y: pos.y,
            },
            vec![],
        )
        .unwrap();
    cast(
        &mut w,
        id,
        Pos {
            x: pos.x - 4.,
            y: pos.y,
        },
    )
    .unwrap();
    for _ in 0..21 {
        w.step();
    }
    assert_eq!(w.entities[&target].hp, 850);
    assert_eq!(w.entities[&target].conditions(w.now), 0);
}

#[test]
fn native_quiver_requires_matching_use_clock_and_cannot_duplicate_its_arrow() {
    let (mut w, id) = archer(0xa65);
    let pos = w.players[&id].pos;
    let target = Pos {
        x: pos.x + 4.,
        y: pos.y,
    };
    let slot = Slot {
        object: id,
        slot: 1,
        kind: 0xa65,
    };
    let attack_clock = w.players[&id].last_attack;
    w.native_use_item(id, 1000, slot, target).unwrap();
    assert!(w.bullets.is_empty());
    let start = Pos {
        x: pos.x + 0.3,
        y: pos.y,
    };
    assert!(
        w.native_shoot_received(id, (1001, Instant::now()), 0, 0xa65, start, 0.)
            .is_err()
    );
    w.native_shoot_received(id, (1000, Instant::now()), 0, 0xa65, start, 0.)
        .unwrap();
    assert!(
        w.native_shoot_received(id, (1000, Instant::now()), 1, 0xa65, start, 0.)
            .is_err()
    );
    assert_eq!(w.bullets.len(), 1);
    assert_eq!(
        (
            w.players[&id].next_bullet,
            w.players[&id].last_attack,
            w.players[&id].attack_count
        ),
        (1, attack_clock, 0)
    );
    assert_eq!(w.players[&id].server_bullet, 128);
    let seed = 12351_u32 * 16807;
    assert_eq!(w.bullets[0].damage, 290 + (seed % 60) as i32);
    w.native_shoot_received(id, (1001, Instant::now()), 1, 0xa1a, start, 0.)
        .unwrap();
    assert_eq!(w.bullets.len(), 2);
    assert_eq!(w.players[&id].next_bullet, 2);
}

#[test]
fn rejected_predicted_quivers_preserve_counter_and_rng_alignment_for_the_next_bow_shot() {
    for rejection in 0..3 {
        let (mut w, id) = archer(0xa65);
        let pos = w.players[&id].pos;
        match rejection {
            0 => w.players.get_mut(&id).unwrap().character.mp = 74,
            1 => {
                w.apply_condition(id, crate::condition::QUIET, 1000)
                    .unwrap();
            }
            2 => w.players.get_mut(&id).unwrap().last_ability = w.now,
            _ => unreachable!(),
        }
        let mp = w.players[&id].character.mp;
        assert!(
            w.native_use_item(
                id,
                1000,
                Slot {
                    object: id,
                    slot: 1,
                    kind: 0xa65
                },
                pos
            )
            .is_err()
        );
        let start = Pos {
            x: pos.x + 0.3,
            y: pos.y,
        };
        w.native_shoot_received(id, (1000, Instant::now()), 0, 0xa65, start, 0.)
            .unwrap();
        assert!(w.bullets.is_empty());
        assert_eq!(
            (
                w.players[&id].character.mp,
                w.players[&id].character.shots,
                w.players[&id].next_bullet
            ),
            (mp, 0, 1)
        );
        w.native_shoot_received(id, (1001, Instant::now()), 1, 0xa1a, start, 0.)
            .unwrap();
        assert_eq!(w.bullets.len(), 1);
        assert_eq!(w.players[&id].next_bullet, 2);
        assert_eq!(
            u64::from(w.players[&id].seed),
            (12351_u64 * 16807 * 16807) % 2147483647
        );
    }
}

#[test]
fn quiver_in_flight_during_position_correction_consumes_prediction_without_damage() {
    let (mut w, id) = archer(0xa65);
    let pos = w.players[&id].pos;
    w.players
        .get_mut(&id)
        .unwrap()
        .goto_acks
        .push_back((true, Instant::now()));
    cast(
        &mut w,
        id,
        Pos {
            x: pos.x + 4.,
            y: pos.y,
        },
    )
    .unwrap();
    assert!(w.bullets.is_empty());
    assert_eq!(w.players[&id].next_bullet, 1);
    assert!(w.players[&id].quiver_shot.is_none());
}

#[test]
fn magic_quiver_applies_and_expires_its_xml_slow_effect() {
    let (mut w, id) = archer(0xa61);
    let pos = w.players[&id].pos;
    let target = w
        .spawn_entity(
            0x646,
            Pos {
                x: pos.x + 4.,
                y: pos.y,
            },
            vec![],
        )
        .unwrap();
    let aim = w.entities[&target].pos;
    cast(&mut w, id, aim).unwrap();
    for _ in 0..20 {
        w.step();
    }
    assert_ne!(
        w.entities[&target].conditions(w.now) & crate::content::SLOWED,
        0
    );
    assert!(w.entities[&target].hp < 850);
    w.now += 3000;
    assert_eq!(
        w.entities[&target].conditions(w.now) & crate::content::SLOWED,
        0
    );
}
