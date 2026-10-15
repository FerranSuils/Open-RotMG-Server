use super::tests::{ch, world};
use super::*;

fn native() -> (World, i32, mpsc::Receiver<Packet>, watch::Receiver<bool>) {
    let mut w = world();
    let (packets, rx) = mpsc::channel(4096);
    let (lost, loss) = watch::channel(false);
    let id = w.enter(ch(&w), Some(Output { packets, lost })).unwrap();
    (w, id, rx, loss)
}
fn offer(w: &mut World, id: i32, owner: Option<i32>, bullet: u8, damage: i32) -> i32 {
    let pos = w.players[&id].pos;
    let owner = owner.unwrap_or_else(|| w.spawn_entity(0x804, pos, vec![]).unwrap());
    w.players.get_mut(&id).unwrap().known.insert(owner);
    let spec = w.content.get(0x804).unwrap().projectiles[&0].clone();
    w.bullets.push(Bullet {
        owner,
        source_kind: 0x804,
        id: bullet,
        friendly: false,
        start: pos,
        pos,
        angle: 0.,
        born: w.now,
        damage,
        spec,
        hit: BTreeSet::new(),
    });
    let mut body = Writer::new();
    body.u8(bullet)
        .i32(owner)
        .u8(0)
        .f32(pos.x)
        .f32(pos.y)
        .f32(0.)
        .i16(damage as i16);
    w.send_enemy_shot(owner, body.0);
    owner
}

#[test]
fn native_miss_takes_no_independent_damage_and_report_commits_exactly_once_without_echo() {
    let (mut w, id, mut rx, _) = native();
    let owner = offer(&mut w, id, None, 3, 20);
    w.acknowledge_shoot(id, 100).unwrap();
    let hp = w.players[&id].character.hp;
    for _ in 0..2 {
        w.step();
    }
    assert_eq!(w.players[&id].character.hp, hp);
    w.native_player_hit(id, owner, 3).unwrap();
    assert_eq!(w.players[&id].character.hp, hp - 20);
    w.native_player_hit(id, owner, 3).unwrap();
    assert_eq!(w.players[&id].character.hp, hp - 20);
    while let Ok(packet) = rx.try_recv() {
        assert_ne!(packet.id, 47);
    }
}

#[test]
fn report_requires_actual_offer_and_ack_but_not_a_living_owner_or_server_bullet() {
    let (mut w, id, _rx, _loss) = native();
    let owner = offer(&mut w, id, None, 3, 20);
    let hp = w.players[&id].character.hp;
    w.native_player_hit(id, owner, 3).unwrap();
    assert_eq!(w.players[&id].character.hp, hp);
    w.acknowledge_shoot(id, 100).unwrap();
    w.bullets.clear();
    w.remove_entity(owner);
    w.now = 4000;
    w.native_player_hit(id, owner, 3).unwrap();
    assert_eq!(w.players[&id].character.hp, hp - 20);
}

#[test]
fn expired_and_unknown_reports_are_observable_denials_not_disconnects_or_damage() {
    let (mut w, id, _rx, loss) = native();
    let owner = offer(&mut w, id, None, 3, 20);
    w.acknowledge_shoot(id, 100).unwrap();
    let hp = w.players[&id].character.hp;
    w.now = 10000;
    w.expire_native_projectiles();
    w.native_player_hit(id, owner, 3).unwrap();
    w.native_player_hit(id, owner, 99).unwrap();
    assert_eq!(w.players[&id].character.hp, hp);
    assert!(!*loss.borrow());
}

#[test]
fn fifo_ack_separates_reused_bullet_ids_and_owner_nova_acknowledgements() {
    let (mut w, id, _rx, _loss) = native();
    assert!(native_hits::expect_shoot_ack(
        w.players.get_mut(&id).unwrap(),
        vec![]
    ));
    let owner = offer(&mut w, id, None, 3, 20);
    let hp = w.players[&id].character.hp;
    w.acknowledge_shoot(id, 90).unwrap();
    assert!(w.players[&id].visible_projectiles.is_empty());
    w.acknowledge_shoot(id, 100).unwrap();
    w.native_player_hit(id, owner, 3).unwrap();
    w.bullets.clear();
    w.now += 50;
    offer(&mut w, id, Some(owner), 3, 25);
    w.native_player_hit(id, owner, 3).unwrap();
    assert_eq!(w.players[&id].character.hp, hp - 20);
    w.acknowledge_shoot(id, 200).unwrap();
    w.native_player_hit(id, owner, 3).unwrap();
    assert_eq!(w.players[&id].character.hp, hp - 45);
    assert!(w.acknowledge_shoot(id, 300).is_err());
}

#[test]
fn rejected_creation_ack_never_authorizes_a_hit() {
    let (mut w, id, _rx, _loss) = native();
    let owner = offer(&mut w, id, None, 3, 20);
    w.acknowledge_shoot(id, -1).unwrap();
    let hp = w.players[&id].character.hp;
    w.native_player_hit(id, owner, 3).unwrap();
    assert_eq!(w.players[&id].character.hp, hp);
}

#[test]
fn reported_fatal_hit_uses_existing_permanent_death_state_and_conditions_engine() {
    let (mut w, id, _rx, _loss) = native();
    let owner = offer(&mut w, id, None, 3, 20);
    w.acknowledge_shoot(id, 100).unwrap();
    w.players.get_mut(&id).unwrap().character.hp = 20;
    w.native_player_hit(id, owner, 3).unwrap();
    assert!(w.players[&id].death_pending);
    assert_eq!(w.players[&id].character.hp, 0);
    assert_eq!(w.players[&id].killer, "East Tutorial Gun");
    w.native_player_hit(id, owner, 3).unwrap();
    assert_eq!(w.players[&id].character.hp, 0);
}

#[test]
fn native_shoot_ack_backlog_and_timeout_keep_existing_transport_bounds() {
    let (mut w, id, _rx, loss) = native();
    let p = w.players.get_mut(&id).unwrap();
    for _ in 0..64 {
        assert!(native_hits::expect_shoot_ack(p, vec![]));
    }
    assert!(!native_hits::expect_shoot_ack(p, vec![]));
    assert!(*loss.borrow());
    assert_eq!(p.shoot_acks.len(), 64);
    p.shoot_acks.clear();
    p.shoot_acks
        .push_back((Instant::now() - std::time::Duration::from_secs(6), vec![]));
    w.expire_native_projectiles();
    assert!(*loss.borrow());
}
