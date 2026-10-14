use super::tests::receive;
use super::*;
use crate::content::WIZARD;
use crate::wire::Reader;

#[test]
fn restricted_view_excludes_private_inventory_ai_and_undiscovered_collision() {
    let mut d = super::tests::directory();
    let (a, _ar) = receive(&mut d, -2, 1, WIZARD);
    let (b, _br) = receive(&mut d, -2, 2, WIZARD);
    let w = d.worlds.get_mut(&-2).unwrap();
    w.players.get_mut(&b).unwrap().character.items[11] = 88888;
    let pos = w.players[&a].pos;
    let bag = w.spawn_entity(0x508, pos, vec![0xa22]).unwrap();
    w.entities.get_mut(&bag).unwrap().extra_stats.push((54, 1));
    let hidden = w.spawn_entity(0x508, pos, vec![0xa23]).unwrap();
    w.entities
        .get_mut(&hidden)
        .unwrap()
        .extra_stats
        .push((54, 2));
    let staged = w.spawn_entity(0x71d, pos, vec![]).unwrap();
    w.entities.get_mut(&staged).unwrap().published = false;
    w.updates(a, false).unwrap();
    let view = d.player_view(a).unwrap();
    assert_eq!(view.session.world, -2);
    let peer = view.objects.iter().find(|o| o.id == b).unwrap();
    assert_eq!(peer.equipped.len(), 4);
    assert!(peer.contents.is_empty());
    assert_eq!(
        view.objects.iter().find(|o| o.id == bag).unwrap().contents,
        vec![0xa22]
    );
    assert!(
        !view
            .objects
            .iter()
            .any(|o| o.id == hidden || o.id == staged)
    );
    assert!(view.tiles.len() < d.worlds[&-2].map.cells.len());
    assert!(!view.tiles.iter().any(|t| t.x == 0 && t.y == 0));
    let json = serde_json::to_string(&view).unwrap();
    for private in [
        "88888",
        "next_shot",
        "damage",
        "hit",
        "home",
        "goal",
        "config",
        "terrain",
        "region",
    ] {
        assert!(!json.contains(private), "private field leaked: {private}");
    }
}
#[test]
fn own_condition_view_matches_native_stat29_without_exposing_other_statuses() {
    let mut d = super::tests::directory();
    let (a, _ar) = receive(&mut d, -2, 1, WIZARD);
    let (b, _br) = receive(&mut d, -2, 2, WIZARD);
    let effects = [
        crate::condition::SLOWED,
        crate::condition::SICK,
        crate::condition::QUIET,
    ];
    let w = d.worlds.get_mut(&-2).unwrap();
    for effect in effects {
        w.apply_condition(a, effect, 2000).unwrap();
    }
    w.apply_condition(b, crate::condition::STUNNED, 5000)
        .unwrap();
    w.updates(a, false).unwrap();
    for now in [0, 1000, 2000] {
        let w = d.worlds.get_mut(&-2).unwrap();
        w.now = now;
        let before = w.players[&a].statuses.clone();
        let bytes = w.stats(a).unwrap();
        let mut r = Reader::new(&bytes);
        assert_eq!(r.i32().unwrap(), a);
        r.f32().unwrap();
        r.f32().unwrap();
        let count = r.i16().unwrap();
        let mut mask = None;
        for _ in 0..count {
            let kind = r.u8().unwrap();
            if matches!(kind, 31 | 62) {
                r.utf().unwrap();
            } else {
                let value = r.i32().unwrap();
                if kind == 29 {
                    mask = Some(value);
                }
            }
        }
        r.done().unwrap();
        let view = d.player_view(a).unwrap();
        assert_eq!(Some(view.character.condition_mask), mask);
        assert_eq!(
            view.character.condition_mask,
            if now < 2000 {
                effects
                    .into_iter()
                    .map(crate::condition::bit)
                    .fold(0, |a, b| a | b)
            } else {
                0
            }
        );
        let json = serde_json::to_value(view).unwrap();
        assert!(
            json["objects"]
                .as_array()
                .unwrap()
                .iter()
                .all(|o| o.get("condition_mask").is_none())
        );
        assert_eq!(d.worlds[&-2].players[&a].statuses, before);
    }
}
#[test]
fn projectile_view_is_observed_trajectory_not_rng_hit_ledger_or_old_shots() {
    let mut d = super::tests::directory();
    let (a, _ar) = receive(&mut d, -2, 1, WIZARD);
    let w = d.worlds.get_mut(&-2).unwrap();
    w.now = 1000;
    let pos = w.players[&a].pos;
    w.shoot(a, 1000, 1, 0xa97, pos, 0.).unwrap();
    assert_eq!(d.player_view(a).unwrap().projectiles.len(), 1);
    let w = d.worlds.get_mut(&-2).unwrap();
    w.bullets[0].damage = 99999;
    w.bullets[0].hit.insert(54321);
    let json = serde_json::to_string(&d.player_view(a).unwrap()).unwrap();
    assert!(!json.contains("99999") && !json.contains("54321"));
    let w = d.worlds.get_mut(&-2).unwrap();
    w.players.get_mut(&a).unwrap().known_since.insert(a, 1000);
    assert!(d.player_view(a).unwrap().projectiles.is_empty());
}
#[test]
fn actual_scoped_chat_stream_and_old_session_remain_distinct_from_replacement_view() {
    let mut d = super::tests::directory();
    let (a, mut ar) = receive(&mut d, -2, 1, WIZARD);
    let (_b, mut br) = receive(&mut d, -2, 2, WIZARD);
    let (_c, mut cr) = receive(&mut d, -3, 3, WIZARD);
    let old = d.player_view(a).unwrap().session;
    d.chat(a, "Ready.".into(), Instant::now()).unwrap();
    assert_eq!(br.try_recv().unwrap().id, wire::TEXT);
    assert!(cr.try_recv().is_err());
    let ch = d.worlds.get_mut(&-2).unwrap().leave(a).unwrap();
    let (packets, mut new_rx) = mpsc::channel(128);
    let (lost, _) = watch::channel(false);
    let new = d
        .worlds
        .get_mut(&-3)
        .unwrap()
        .enter(ch, Some(Output { packets, lost }))
        .unwrap();
    while new_rx.try_recv().is_ok() {}
    let view = d.player_view(new).unwrap();
    assert_ne!(
        (old.world, old.player),
        (view.session.world, view.session.player)
    );
    assert_eq!(old.character, view.session.character);
    assert!(d.player_view(old.player).is_err());
    assert!(new_rx.try_recv().is_err());
    let packet = ar.try_recv().unwrap();
    let mut r = Reader::new(&packet.body);
    r.utf().unwrap();
    assert_eq!(r.i32().unwrap(), old.player);
    r.i32().unwrap();
    r.u8().unwrap();
    r.utf().unwrap();
    assert_eq!(r.utf().unwrap(), "Ready.");
    assert_eq!(r.utf().unwrap(), "");
    r.done().unwrap();
}
#[test]
fn view_reports_busy_quarantine_lost_closed_and_detached_without_fabricating_success() {
    let mut d = super::tests::directory();
    let (a, _ar) = receive(&mut d, -2, 1, WIZARD);
    let p = d.worlds.get_mut(&-2).unwrap().players.get_mut(&a).unwrap();
    p.pending = true;
    p.persistence_failure = Some("database details stay in logs".into());
    for _ in 0..129 {
        send(p, wire::TEXT, vec![]);
    }
    let view = d.player_view(a).unwrap();
    assert!(view.lifecycle.busy && view.lifecycle.quarantined);
    assert_eq!(view.character.inventory_revision, 0);
    let json = serde_json::to_value(view).unwrap();
    assert_eq!(json["lifecycle"]["output"], "lost");
    assert!(!json.to_string().contains("database details"));
    let p = d.worlds.get_mut(&-2).unwrap().players.get_mut(&a).unwrap();
    p.tx = None;
    assert_eq!(
        serde_json::to_value(d.player_view(a).unwrap()).unwrap()["lifecycle"]["output"],
        "detached"
    );
    d.worlds.get_mut(&-2).unwrap().players.remove(&a);
    assert!(d.player_view(a).is_err());
}
