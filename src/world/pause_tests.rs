use super::*;
use crate::{content::WIZARD, wire::Reader};

fn condition(bytes: &[u8]) -> i32 {
    let mut r = Reader::new(bytes);
    r.i32().unwrap();
    r.f32().unwrap();
    r.f32().unwrap();
    for _ in 0..r.i16().unwrap() {
        let stat = r.u8().unwrap();
        if matches!(stat, 31 | 62) {
            r.utf().unwrap();
        } else {
            let value = r.i32().unwrap();
            if stat == 29 {
                return value;
            }
        }
    }
    panic!("condition stat missing");
}

#[test]
fn pause_command_is_private_toggle_with_selected_condition_and_native_ticks() {
    let mut d = tests::directory();
    let (a, mut ar) = tests::receive(&mut d, -2, 1, WIZARD);
    let (b, mut br) = tests::receive(&mut d, -2, 2, WIZARD);
    d.chat(a, "/pause".into(), Instant::now()).unwrap();
    let reply = ar.try_recv().unwrap();
    assert_eq!(reply.id, wire::TEXT);
    assert!(String::from_utf8_lossy(&reply.body).contains("Game paused."));
    assert!(br.try_recv().is_err());
    let w = d.worlds.get_mut(&-2).unwrap();
    assert_eq!(condition(&w.stats(a).unwrap()), 0x100000);
    let pos = w.players[&a].pos;
    w.move_to(
        a,
        Pos {
            x: pos.x + 0.1,
            y: pos.y,
        },
        100,
    )
    .unwrap();
    let correction = ar.try_recv().unwrap();
    assert_eq!(correction.id, wire::GOTO);
    let mut r = Reader::new(&correction.body);
    assert_eq!(r.i32().unwrap(), a);
    assert_eq!((r.f32().unwrap(), r.f32().unwrap()), (pos.x, pos.y));
    r.done().unwrap();
    w.step();
    w.step();
    let status = w.stats(a).unwrap();
    for rx in [&mut ar, &mut br] {
        let packets: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert!(
            packets.iter().any(
                |p| p.id == wire::NEW_TICK && p.body.windows(status.len()).any(|s| s == status)
            )
        );
    }
    assert!(d.teleport(a, b, Instant::now()).is_err());
    assert!(d.teleport(b, a, Instant::now()).is_err());
    assert!(d.teleports.is_empty());
    d.chat(a, "still here".into(), Instant::now()).unwrap();
    d.chat(a, "/pause".into(), Instant::now()).unwrap();
    assert_eq!(condition(&d.worlds[&-2].stats(a).unwrap()), 0);
    assert!(d.chat(a, "/pause extra".into(), Instant::now()).is_err());
}

#[test]
fn pause_safety_boundary_resume_and_session_lifecycle() {
    let mut d = tests::directory();
    let (a, _rx) = tests::receive(&mut d, -2, 1, WIZARD);
    let w = d.worlds.get_mut(&-2).unwrap();
    let pos = w.players[&a].pos;
    let enemy = w.spawn_entity(0x600, pos, vec![]).unwrap();
    for distance in [0., 7.999, 8.] {
        d.worlds
            .get_mut(&-2)
            .unwrap()
            .entities
            .get_mut(&enemy)
            .unwrap()
            .pos
            .x = pos.x + distance;
        assert!(d.chat(a, "/pause".into(), Instant::now()).is_err());
        assert!(!d.worlds[&-2].players[&a].paused);
    }
    d.worlds
        .get_mut(&-2)
        .unwrap()
        .entities
        .get_mut(&enemy)
        .unwrap()
        .pos
        .x = pos.x + 8.01;
    d.chat(a, "/pause".into(), Instant::now()).unwrap();
    d.worlds
        .get_mut(&-2)
        .unwrap()
        .entities
        .get_mut(&enemy)
        .unwrap()
        .pos = pos;
    d.chat(a, "/pause".into(), Instant::now()).unwrap(); // Safety check never traps a paused player.
    d.worlds.get_mut(&-2).unwrap().remove_entity(enemy);
    for mode in 0..4 {
        let p = d.worlds.get_mut(&-2).unwrap().players.get_mut(&a).unwrap();
        p.character.alive = mode != 0;
        p.death_pending = mode == 1;
        p.departing = mode == 2;
        p.persistence_failure = (mode == 3).then(|| "failed save".into());
        assert!(d.chat(a, "/pause".into(), Instant::now()).is_err());
    }
    let w = d.worlds.get_mut(&-2).unwrap();
    let p = w.players.get_mut(&a).unwrap();
    p.persistence_failure = None;
    p.paused = true;
    let ch = w.leave(a).unwrap();
    let replacement = w.enter(ch, None).unwrap();
    assert!(!w.players[&replacement].paused);
}

#[test]
fn paused_native_move_shoot_and_item_use_resume_without_clock_or_rng_desync() {
    let mut w = tests::world();
    let a = w.enter(tests::ch(&w), None).unwrap();
    let p = w.players.get_mut(&a).unwrap();
    p.controller = Controller::Human;
    p.paused = true;
    p.character.hp = 50;
    p.character.mp = 20;
    let credit = p.movement_credit;
    let pos = p.pos;
    for i in 1..=40 {
        w.step();
        w.move_to(a, Pos { x: -1., y: -1. }, i * 50).unwrap();
    }
    assert_eq!(w.players[&a].pos.distance(pos), 0.);
    assert_eq!(
        (w.players[&a].character.hp, w.players[&a].character.mp),
        (50, 20)
    );
    assert_eq!(w.players[&a].movement_credit, credit);
    // The first pause tick is acknowledged with ordinary coordinates.
    w.move_to(
        a,
        Pos {
            x: pos.x + 0.1,
            y: pos.y,
        },
        2100,
    )
    .unwrap();
    assert_eq!(w.players[&a].pos.distance(pos), 0.);
    let start = Pos {
        x: pos.x + 0.3,
        y: pos.y,
    };
    w.shoot(a, 2000, 1, 0xa97, start, 0.).unwrap(); // Already queued before the client saw the pause bit.
    let seed = w.players[&a].seed;
    assert_ne!(seed, 1);
    assert_eq!(w.players[&a].next_bullet, 2);
    assert_eq!(w.players[&a].character.shots, 0);
    assert!(!w.bullets.iter().any(|b| b.owner == a));
    let slot = Slot {
        object: a,
        slot: 1,
        kind: w.players[&a].character.items[1],
    };
    assert!(w.use_item(a, slot, pos).is_err());
    w.players.get_mut(&a).unwrap().character.items[4] = 0xa22;
    assert!(
        w.use_item(
            a,
            Slot {
                object: a,
                slot: 4,
                kind: 0xa22
            },
            pos
        )
        .is_err()
    );
    assert_eq!(w.players[&a].character.items[4], 0xa22);
    w.players.get_mut(&a).unwrap().paused = false;
    // The first resume tick still carries the paused sentinel.
    w.move_to(a, Pos { x: -1., y: -1. }, 2200).unwrap();
    w.now += 100;
    w.move_to(
        a,
        Pos {
            x: pos.x + 0.1,
            y: pos.y,
        },
        2300,
    )
    .unwrap();
    w.now += 1000;
    let pos = w.players[&a].pos;
    w.shoot(
        a,
        3000,
        2,
        0xa97,
        Pos {
            x: pos.x + 0.3,
            y: pos.y,
        },
        0.,
    )
    .unwrap();
    assert_eq!(w.players[&a].character.shots, 1);
    assert_eq!(
        w.players[&a].seed,
        ((u64::from(seed) * 16807) % 2147483647) as u32
    );
    assert!(w.bullets.iter().any(|b| b.owner == a));
    for _ in 0..60 {
        w.step();
    }
    assert!(w.players[&a].character.hp > 50);
    assert!(w.players[&a].character.mp > 20);
}

#[test]
fn paused_players_are_not_ai_targets_or_projectile_shields() {
    let mut w = tests::world();
    for id in w.entities.keys().copied().collect::<Vec<_>>() {
        w.remove_entity(id);
    }
    let a = w.enter(tests::ch(&w), None).unwrap();
    let mut ch = tests::ch(&w);
    ch.id = 2;
    ch.account = 2;
    let b = w.enter(ch, None).unwrap();
    let pos = w.players[&a].pos;
    w.players.get_mut(&a).unwrap().paused = true;
    w.players.get_mut(&b).unwrap().pos = pos;
    let spec = w.content.objects[&0x683].projectiles[&0].clone();
    w.bullets.push(Bullet {
        owner: 500,
        source_kind: 0x683,
        id: 1,
        friendly: false,
        start: pos,
        pos,
        angle: 0.,
        born: 0,
        damage: 8,
        spec,
        hit: BTreeSet::new(),
    });
    w.step();
    assert_eq!(w.players[&a].character.hp, 100);
    assert_eq!(w.players[&b].character.hp, 92);
    w.players.get_mut(&b).unwrap().departing = true;
    let enemy = w
        .spawn_entity(
            0x600,
            Pos {
                x: pos.x + 2.,
                y: pos.y,
            },
            vec![],
        )
        .unwrap();
    w.encounter = true;
    let before = w.entities[&enemy].pos;
    assert!(w.npc_step().is_empty());
    assert_eq!(w.entities[&enemy].pos.distance(before), 0.);
    // A shot already in flight is consumed without damage while its owner is paused.
    w.players.get_mut(&a).unwrap().paused = false;
    w.now = 1000;
    w.shoot(
        a,
        1000,
        1,
        0xa97,
        Pos {
            x: pos.x + 0.3,
            y: pos.y,
        },
        0.,
    )
    .unwrap();
    w.players.get_mut(&a).unwrap().paused = true;
    let hp = w.entities[&enemy].hp;
    for _ in 0..5 {
        w.step();
    }
    assert_eq!(w.entities[&enemy].hp, hp);
    assert!(!w.bullets.iter().any(|b| b.owner == a));
    w.players.get_mut(&a).unwrap().paused = false;
    assert!(!w.npc_step().is_empty());
}
