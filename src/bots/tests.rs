use super::*;
use crate::content::WIZARD;
use crate::world::view::{
    Lifecycle, MapInfo, OwnCharacter, Session, Tile, VisibleObject, VisibleProjectile,
};
fn content() -> Content {
    Content::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("data")
            .as_path(),
    )
    .unwrap()
}
fn view(content: &Content) -> PlayerView {
    let class = content.get(WIZARD).unwrap();
    PlayerView {
        session: Session {
            world: -3,
            player: 1,
            account: 1,
            character: 1,
        },
        tick: 100,
        time_ms: 5000,
        position: Pos { x: 11.5, y: 11.5 },
        map: MapInfo {
            name: "Realm".into(),
            width: 40,
            height: 40,
        },
        character: OwnCharacter {
            class: WIZARD,
            level: 1,
            experience: 0,
            hp: 100,
            mp: 100,
            stats: [100, 100, 12, 0, 10, 15, 12, 12],
            items: class.equipment.clone(),
            inventory_revision: 0,
            pirate_caves: 0,
            ability_ready_ms: 0,
        },
        lifecycle: Lifecycle {
            alive: true,
            death_pending: false,
            departing: false,
            busy: false,
            quarantined: false,
            output: OutputState::Live,
        },
        tiles: (0..30)
            .flat_map(|y| {
                (0..30).map(move |x| Tile {
                    x,
                    y,
                    ground: 0,
                    ground_speed: 1.,
                })
            })
            .collect(),
        objects: vec![],
        projectiles: vec![],
    }
}
fn object(id: i32, kind: u16, x: f32, y: f32) -> VisibleObject {
    VisibleObject {
        id,
        kind,
        name: String::new(),
        position: Pos { x, y },
        hp: 100,
        max_hp: 100,
        equipped: vec![],
        contents: vec![],
        blocks: false,
    }
}
#[test]
fn same_observation_produces_resource_recovery_and_companion_choices() {
    let content = content();
    let mut v = view(&content);
    v.character.hp = 50;
    let enemy = content.objects.values().find(|o| o.enemy).unwrap().kind;
    v.objects.push(object(2, enemy, 21.5, 11.5));
    let bag = content
        .objects
        .values()
        .find(|o| o.class == "Container")
        .unwrap()
        .kind;
    let mut loot = object(3, bag, 13., 11.5);
    loot.contents = vec![0xa22];
    v.objects.push(loot);
    let mut ally = object(4, WIZARD, 22.5, 11.5);
    ally.name = "Mossy".into();
    ally.equipped = vec![0xa97];
    v.objects.push(ally);
    let goals: Vec<_> = PROFILES[..3]
        .iter()
        .map(|p| Brain::new(*p, &v).decide(&v, &content).unwrap().goal)
        .collect();
    assert_eq!(goals, vec![Goal::Recover, Goal::Loot(3), Goal::Follow(4)]);
}
#[test]
fn explores_known_frontiers_toward_an_observed_distant_encounter() {
    let content = content();
    let mut v = view(&content);
    let enemy = content.objects.values().find(|o| o.enemy).unwrap().kind;
    v.objects.push(object(2, enemy, 39.5, 11.5));
    let mut brain = Brain::new(PROFILES[1], &v);
    brain.heading = std::f32::consts::PI;
    let decision = brain.decide(&v, &content).unwrap();
    assert_eq!(decision.goal, Goal::Explore(29, 11));
    let grid = Grid::new(&v);
    assert!(!grid.known.contains(&(39, 11)));
    assert!(grid.segment(v.position, decision.movement.unwrap()));
    assert!(decision.aim.is_none());
}

#[test]
fn busy_clock_lifecycle_and_session_do_not_fabricate_actions() {
    let content = content();
    let mut v = view(&content);
    let mut b = Brain::new(PROFILES[0], &v);
    v.lifecycle.busy = true;
    let d = b.decide(&v, &content).unwrap();
    assert!(d.movement.is_none() && d.aim.is_none() && d.item.is_none());
    assert!(b.decide(&v, &content).is_err());
    v.tick += 1;
    v.session.character = 2;
    assert!(b.decide(&v, &content).is_err());
    v.session.character = 1;
    for state in [OutputState::Detached, OutputState::Lost] {
        v.lifecycle.output = state;
        assert!(b.decide(&v, &content).is_err());
    }
    v.lifecycle.output = OutputState::Live;
    v.lifecycle.death_pending = true;
    assert!(b.decide(&v, &content).is_err());
    v.lifecycle.death_pending = false;
    v.lifecycle.quarantined = true;
    assert!(b.decide(&v, &content).is_err());
}
#[test]
fn projectile_dodge_uses_observed_trajectory_and_only_known_routes() {
    let content = content();
    let mut v = view(&content);
    v.projectiles.push(VisibleProjectile {
        owner: 9,
        id: 1,
        start: Pos { x: 9.5, y: 11.5 },
        position: Pos { x: 9.5, y: 11.5 },
        angle: 0.,
        speed: 50.,
        lifetime_ms: 1000.,
        amplitude: 0.,
        frequency: 0.,
        passes_cover: false,
    });
    let d = Brain::new(PROFILES[1], &v).decide(&v, &content).unwrap();
    assert_eq!(d.goal, Goal::Dodge);
    let movement = d.movement.unwrap();
    assert!(Grid::new(&v).segment(v.position, movement));
    assert!(movement.distance(v.position) > 1.);
    let grid = Grid {
        walkable: BTreeSet::from([(0, 0), (0, 1), (1, 1), (2, 1)]),
        known: BTreeSet::from([(0, 0), (0, 1), (1, 0), (1, 1), (2, 0), (2, 1)]),
    };
    assert!(grid.next(center((0, 0)), center((3, 1))).is_none());
    let next = grid.next(center((0, 0)), center((2, 1))).unwrap();
    assert_eq!(tile(next), (0, 1));
    assert!(!grid.frontier((1, 1), 3, 2));
    assert!(!grid.frontier((2, 1), 3, 2));
    let mut straight = Grid {
        walkable: (0..=24).map(|x| (x, 0)).collect(),
        known: (0..=24).map(|x| (x, 0)).collect(),
    };
    assert_eq!(
        tile(straight.next(center((0, 0)), center((24, 0))).unwrap()),
        (24, 0)
    );
    straight.walkable.remove(&(13, 0));
    assert!(!straight.segment(center((0, 0)), center((24, 0))));
    assert!(straight.next(center((0, 0)), center((24, 0))).is_none());
}
#[test]
fn held_resources_compatible_upgrades_and_margin_survive_seeded_replay() {
    let content = content();
    let mut v = view(&content);
    v.character.hp = 1;
    v.character.items[4..].fill(-1);
    let mut brain = Brain::new(PROFILES[0], &v);
    assert!(brain.decide(&v, &content).unwrap().item.is_none());
    v.tick += 1;
    v.character.items[4] = 0xa22;
    v.character.stats[0] = 200;
    assert!(matches!(
        brain.decide(&v, &content).unwrap().item,
        Some(Action::Use { slot: 4, .. })
    ));
    assert!(!useful(&content, &v, 0xa01).unwrap());
    let mut left = Brain::new(PROFILES[2], &v);
    let mut right = Brain::new(PROFILES[2], &v);
    v.character.hp = 200;
    for tick in 110..140 {
        v.tick = tick;
        let a = left.decide(&v, &content).unwrap();
        let b = right.decide(&v, &content).unwrap();
        assert_eq!(a.goal, b.goal);
        assert_eq!(format!("{:?}", a.movement), format!("{:?}", b.movement));
        left.goal = a.goal;
        right.goal = b.goal;
    }
    let enemy = content.objects.values().find(|o| o.enemy).unwrap().kind;
    v.objects = vec![object(2, enemy, 18.5, 11.5), object(3, enemy, 17.5, 11.5)];
    v.tick = 200;
    v.time_ms = 10000;
    left.goal = Goal::Fight(2);
    assert_eq!(left.decide(&v, &content).unwrap().goal, Goal::Fight(2));
}
#[test]
fn only_addressed_native_chat_changes_companion_memory() {
    let content = content();
    let mut v = view(&content);
    let mut brain = Brain::new(PROFILES[2], &v);
    let packet = |text: &str| {
        let mut w = crate::wire::Writer::new();
        w.utf("Human").i32(9).i32(0).u8(5).utf("").utf(text).utf("");
        Packet {
            id: wire::TEXT,
            body: w.0,
        }
    };
    assert!(!brain.receive(packet("Trellis follow me"), &v).unwrap());
    assert!(brain.companion.is_none());
    let mut sender = object(9, WIZARD, 12.5, 11.5);
    sender.equipped = v.character.items[..4].to_vec();
    v.objects.push(sender);
    assert!(!brain.receive(packet("Bayleaf follow me"), &v).unwrap());
    assert!(brain.companion.is_none());
    assert!(brain.receive(packet("Trellis follow me"), &v).unwrap());
    assert_eq!(brain.companion, Some(9));
}

#[test]
fn fresh_crewmate_rejoins_through_scoped_expiring_public_chat() {
    let content = content();
    let mut v = view(&content);
    let packet = |name: &str, actor, recipient: &str, text: &str| {
        let mut w = crate::wire::Writer::new();
        w.utf(name)
            .i32(actor)
            .i32(0)
            .u8(5)
            .utf(recipient)
            .utf(text)
            .utf("");
        Packet {
            id: wire::TEXT,
            body: w.0,
        }
    };
    let mut follower = Brain::new(PROFILES[2], &v);
    assert_eq!(follower.decide(&v, &content).unwrap().chat, Some("tp?"));
    v.tick += 5;
    assert!(follower.decide(&v, &content).unwrap().chat.is_none());
    for (name, actor, recipient) in [("Reed", 9, ""), ("Mossy", -1, ""), ("Mossy", 2, "Human")] {
        follower
            .receive(packet(name, actor, recipient, "tp here"), &v)
            .unwrap();
        assert!(follower.regroup_call.is_none());
    }
    let mut leader_view = view(&content);
    leader_view.session.player = 2;
    let mut leader = Brain::new(PROFILES[1], &leader_view);
    leader
        .receive(packet("Juniper", 9, "", "tp?"), &leader_view)
        .unwrap();
    assert!(leader.regroup_call.is_none());
    leader
        .receive(packet("Trellis", 1, "", "tp?"), &leader_view)
        .unwrap();
    assert_eq!(
        leader.decide(&leader_view, &content).unwrap().chat,
        Some("tp here")
    );
    follower
        .receive(packet("Mossy", 2, "", "tp here"), &v)
        .unwrap();
    v.tick += 5;
    let decision = follower.decide(&v, &content).unwrap();
    assert!(matches!(decision.journey, Some(Journey::Regroup(2))));
    assert!(decision.aim.is_none() && decision.item.is_none() && decision.movement.is_none());
    assert!(v.objects.is_empty());
    follower
        .receive(packet("Mossy", 2, "", "tp here"), &v)
        .unwrap();
    v.tick += 5;
    assert!(follower.decide(&v, &content).unwrap().journey.is_none());
    let mut expired = Brain::new(PROFILES[2], &v);
    expired
        .receive(packet("Mossy", 2, "", "tp here"), &v)
        .unwrap();
    v.time_ms += 5001;
    assert!(expired.decide(&v, &content).unwrap().journey.is_none());
    let mut injured = Brain::new(PROFILES[2], &v);
    injured
        .receive(packet("Mossy", 2, "", "tp here"), &v)
        .unwrap();
    v.character.hp = 70;
    assert!(injured.decide(&v, &content).unwrap().journey.is_none());
    v.character.hp = 100;
    let mut endangered = Brain::new(PROFILES[1], &leader_view);
    endangered
        .receive(packet("Trellis", 1, "", "tp?"), &leader_view)
        .unwrap();
    let kind = content.objects.values().find(|o| o.enemy).unwrap().kind;
    leader_view.objects.push(object(9, kind, 14.5, 11.5));
    assert!(
        endangered
            .decide(&leader_view, &content)
            .unwrap()
            .chat
            .is_none()
    );
}

#[test]
fn selected_target_controls_aim_and_equipped_spell_cooldown() {
    let content = content();
    let mut v = view(&content);
    let enemy = content.objects.values().find(|o| o.enemy).unwrap().kind;
    v.objects = vec![object(2, enemy, 15.5, 11.5), object(3, enemy, 11.5, 14.5)];
    v.objects[0].hp = 1000;
    let mut brain = Brain::new(PROFILES[1], &v);
    brain.goal = Goal::Fight(2);
    let first = brain.decide(&v, &content).unwrap();
    assert_eq!(first.goal, Goal::Fight(2));
    assert!(first.aim.unwrap().abs() < 0.01);
    assert!(matches!(first.item, Some(Action::Use { slot: 1, x, y }) if x == 15.5 && y == 11.5));
    v.character.ability_ready_ms = v.time_ms
        + content
            .get(v.character.items[1] as u16)
            .unwrap()
            .cooldown_ms;
    v.tick += 4;
    v.time_ms += 200;
    assert!(brain.decide(&v, &content).unwrap().item.is_none());
    v.tick += 20;
    v.time_ms += 1000;
    v.character.mp = 0;
    assert!(brain.decide(&v, &content).unwrap().item.is_none());
}

#[test]
fn kites_from_close_targets_and_does_not_fire_through_known_cover() {
    let content = content();
    let mut v = view(&content);
    let enemy = content.objects.values().find(|o| o.enemy).unwrap().kind;
    v.objects.push(object(2, enemy, 13.5, 11.5));
    let mut brain = Brain::new(PROFILES[1], &v);
    let decision = brain.decide(&v, &content).unwrap();
    assert_eq!(decision.goal, Goal::Fight(2));
    assert!(decision.movement.unwrap().distance(v.objects[0].position) > 2.);
    let mut wall = object(9, WIZARD, 12.5, 11.5);
    wall.blocks = true;
    v.objects.push(wall);
    v.tick += 4;
    v.time_ms += 200;
    let blocked = brain.decide(&v, &content).unwrap();
    assert!(blocked.aim.is_none());
    assert!(blocked.item.is_none());
    assert!(blocked.movement.is_some());
    let mut path = blocked;
    for _ in 0..8 {
        brain.goal = path.goal.clone();
        v.position = path
            .movement
            .expect("blocked target needs an approach route");
        v.tick += 4;
        v.time_ms += 200;
        path = brain.decide(&v, &content).unwrap();
        if path.aim.is_some() {
            break;
        }
    }
    assert!(
        path.aim.is_some(),
        "must reach a firing lane instead of orbiting behind cover"
    );
}

#[test]
fn committed_disengagement_uses_nexus_before_the_last_hit_without_held_healing() {
    let content = content();
    let mut v = view(&content);
    v.character.hp = 30;
    v.character
        .items
        .iter_mut()
        .skip(4)
        .for_each(|item| *item = -1);
    let mut brain = Brain::new(PROFILES[0], &v);
    let escaped = brain.decide(&v, &content).unwrap();
    assert_eq!(escaped.goal, Goal::Recover);
    assert!(matches!(escaped.journey, Some(Journey::Nexus)));
    assert!(escaped.aim.is_none());
    v.character.items[4] = 2594;
    let mut brain = Brain::new(PROFILES[0], &v);
    let stocked = brain.decide(&v, &content).unwrap();
    assert!(stocked.journey.is_none());
    assert!(matches!(stocked.item, Some(Action::Use { slot: 4, .. })));
}

#[test]
fn observed_motion_leads_aim_and_recovery_has_a_release_threshold() {
    let content = content();
    let mut v = view(&content);
    let enemy = content.objects.values().find(|o| o.enemy).unwrap().kind;
    v.objects.push(object(2, enemy, 15.5, 11.5));
    let mut brain = Brain::new(PROFILES[0], &v);
    brain.decide(&v, &content).unwrap();
    v.tick += 6;
    v.time_ms += 300;
    v.objects[0].position.y += 0.6;
    assert!(
        brain.decide(&v, &content).unwrap().aim.unwrap() > angle(v.position, v.objects[0].position)
    );
    v.tick += 6;
    v.time_ms += 300;
    v.character.hp = 50;
    let recovery = brain.decide(&v, &content).unwrap();
    assert_eq!(recovery.goal, Goal::Recover);
    assert!(recovery.aim.is_none());
    brain.goal = recovery.goal;
    v.tick += 100;
    v.time_ms += 5000;
    v.character.hp = 65;
    assert_eq!(brain.decide(&v, &content).unwrap().goal, Goal::Recover);
    v.tick += 6;
    v.time_ms += 300;
    v.character.hp = 90;
    assert_eq!(brain.decide(&v, &content).unwrap().goal, Goal::Fight(2));
}

#[test]
fn dodge_scores_the_swept_player_path_not_only_the_endpoint() {
    let content = content();
    let mut v = view(&content);
    v.projectiles.push(VisibleProjectile {
        owner: 9,
        id: 0,
        start: Pos { x: 12.125, y: 10. },
        position: Pos { x: 12.125, y: 10. },
        angle: std::f32::consts::FRAC_PI_2,
        speed: 100.,
        lifetime_ms: 1000.,
        amplitude: 0.,
        frequency: 0.,
        passes_cover: false,
    });
    let crossing = Pos { x: 12.75, y: 11.5 };
    let away = Pos { x: 10.25, y: 11.5 };
    assert!(combat::clearance(&v, crossing) < 0.2);
    assert!(combat::clearance(&v, away) > 0.6);
}

#[test]
fn cohorts_choose_distinct_observed_leaders_not_an_uninvited_human() {
    let content = content();
    let mut v = view(&content);
    for (id, name, x, y) in [
        (2, "Mossy", 21.5, 11.5),
        (3, "Reed", 11.5, 21.5),
        (4, "Human", 10.5, 11.5),
    ] {
        let mut peer = object(id, WIZARD, x, y);
        peer.name = name.into();
        peer.equipped = v.character.items[..4].to_vec();
        v.objects.push(peer);
    }
    assert_eq!(
        Brain::new(PROFILES[2], &v)
            .decide(&v, &content)
            .unwrap()
            .goal,
        Goal::Follow(2)
    );
    assert_eq!(
        Brain::new(PROFILES[3], &v)
            .decide(&v, &content)
            .unwrap()
            .goal,
        Goal::Follow(3)
    );
    v.objects.retain(|o| o.name != "Mossy");
    assert!(matches!(
        Brain::new(PROFILES[1], &v)
            .decide(&v, &content)
            .unwrap()
            .goal,
        Goal::Explore(..)
    ));
}

#[test]
fn priest_heals_actual_public_injury_and_respects_range_mana_and_cooldown() {
    let content = content();
    let mut v = view(&content);
    v.character.class = crate::content::PRIEST;
    v.character.items = content.get(v.character.class).unwrap().equipment.clone();
    let mut peer = object(2, WIZARD, 12.5, 11.5);
    peer.equipped = vec![0xa97];
    peer.name = "Mossy".into();
    v.objects.push(peer);
    let mut brain = Brain::new(PROFILES[0], &v);
    assert!(brain.decide(&v, &content).unwrap().item.is_none());
    v.tick += 6;
    v.time_ms += 300;
    v.objects[0].hp = 40;
    assert!(matches!(
        brain.decide(&v, &content).unwrap().item,
        Some(Action::Use { slot: 1, .. })
    ));
    v.character.ability_ready_ms = v.time_ms
        + content
            .get(v.character.items[1] as u16)
            .unwrap()
            .cooldown_ms;
    v.tick += 6;
    v.time_ms += 300;
    assert!(brain.decide(&v, &content).unwrap().item.is_none());
    v.tick += 100;
    v.time_ms += 5000;
    v.character.mp = 0;
    assert!(brain.decide(&v, &content).unwrap().item.is_none());
    v.tick += 6;
    v.time_ms += 300;
    v.character.mp = 100;
    v.objects[0].position.x = 25.5;
    assert!(brain.decide(&v, &content).unwrap().item.is_none());
}

#[test]
fn leader_waits_for_a_real_straggler_and_invitation_expires() {
    let content = content();
    let mut v = view(&content);
    let mut peer = object(2, WIZARD, 23.5, 11.5);
    peer.name = "Trellis".into();
    peer.equipped = v.character.items[..4].to_vec();
    v.objects.push(peer);
    let d = Brain::new(PROFILES[1], &v).decide(&v, &content).unwrap();
    assert_eq!(d.goal, Goal::Gather(2));
    assert!(d.movement.is_none());
    let mut brain = Brain::new(PROFILES[0], &v);
    brain.companion = Some(2);
    brain.invitation_until = v.time_ms;
    brain.decide(&v, &content).unwrap();
    assert!(brain.companion.is_none());
}

#[test]
fn regroup_requests_only_a_visible_healthy_peer_and_is_paced() {
    let content = content();
    let mut v = view(&content);
    let mut leader = object(2, WIZARD, 21.5, 11.5);
    leader.name = "Mossy".into();
    leader.equipped = v.character.items[..4].to_vec();
    v.objects.push(leader);
    let mut brain = Brain::new(PROFILES[2], &v);
    assert!(matches!(
        brain.decide(&v, &content).unwrap().journey,
        Some(Journey::Regroup(2))
    ));
    v.tick += 5;
    v.time_ms += 250;
    assert!(brain.decide(&v, &content).unwrap().journey.is_none());
    v.tick += 300;
    v.time_ms += 15000;
    v.objects[0].hp = 40;
    assert!(brain.decide(&v, &content).unwrap().journey.is_none());
    v.tick += 5;
    v.time_ms += 250;
    v.objects.clear();
    assert!(brain.decide(&v, &content).unwrap().journey.is_none());
}

#[test]
fn earned_portal_call_and_committed_cave_clear_drive_normal_transfers() {
    let content = content();
    let mut v = view(&content);
    v.character.level = 2;
    for (id, name) in [(2, "Bayleaf"), (3, "Trellis")] {
        let mut member = object(id, WIZARD, 12.5, 11.5);
        member.name = name.into();
        member.equipped = v.character.items[..4].to_vec();
        v.objects.push(member);
    }
    v.objects.push(object(4, 0x717, 12.5, 11.5));
    let leader = Brain::new(PROFILES[1], &v).decide(&v, &content).unwrap();
    assert_eq!(leader.goal, Goal::Portal(4));
    assert_eq!(leader.chat, Some("pc here"));
    assert!(matches!(leader.journey, Some(Journey::Portal(4))));
    v.objects[0].name = "Mossy".into();
    v.objects.remove(1);
    let mut follower = Brain::new(PROFILES[2], &v);
    follower.decide(&v, &content).unwrap();
    let mut out = crate::wire::Writer::new();
    out.utf("Mossy")
        .i32(2)
        .i32(0)
        .u8(5)
        .utf("")
        .utf("pc here")
        .utf("");
    follower
        .receive(
            Packet {
                id: wire::TEXT,
                body: out.0,
            },
            &v,
        )
        .unwrap();
    v.objects.remove(0);
    v.tick += 5;
    v.time_ms += 250;
    assert!(matches!(
        follower.decide(&v, &content).unwrap().journey,
        Some(Journey::Portal(4))
    ));
    v.map.name = "Pirate Cave".into();
    v.objects[0].kind = 0x71d;
    let mut cave = Brain::new(PROFILES[2], &v);
    assert!(cave.decide(&v, &content).unwrap().journey.is_none());
    v.tick += 5;
    v.time_ms += 250;
    v.character.pirate_caves += 1;
    assert!(matches!(
        cave.decide(&v, &content).unwrap().journey,
        Some(Journey::Portal(4))
    ));
}

#[test]
fn quiet_exploration_revisits_only_an_actually_observed_hunting_ground() {
    let content = content();
    let mut v = view(&content);
    let enemy = content.objects.values().find(|o| o.enemy).unwrap().kind;
    v.objects.push(object(2, enemy, 15.5, 11.5));
    let mut brain = Brain::new(PROFILES[1], &v);
    brain.decide(&v, &content).unwrap();
    v.objects.clear();
    v.position.x = 25.5;
    v.tick += 700;
    v.time_ms += 35000;
    assert_eq!(
        brain.decide(&v, &content).unwrap().goal,
        Goal::Revisit(15, 11)
    );
    v.tick += 4;
    v.time_ms += 200;
    let fresh = Brain::new(PROFILES[1], &v).decide(&v, &content).unwrap();
    assert!(matches!(fresh.goal, Goal::Explore(..)));
}

#[test]
fn crews_search_recent_observed_contacts_without_teleporting_to_hidden_peers() {
    let content = content();
    let mut v = view(&content);
    let mut peer = object(2, WIZARD, 19.5, 11.5);
    peer.name = "Mossy".into();
    peer.equipped = v.character.items[..4].to_vec();
    v.objects.push(peer);
    let mut follower = Brain::new(PROFILES[2], &v);
    follower.decide(&v, &content).unwrap();
    v.objects.clear();
    v.tick += 5;
    v.time_ms += 250;
    let search = follower.decide(&v, &content).unwrap();
    assert_eq!(search.goal, Goal::Follow(2));
    assert!(search.movement.is_some());
    assert!(search.journey.is_none());
    v.tick += 300;
    v.time_ms += 15000;
    assert!(matches!(
        follower.decide(&v, &content).unwrap().goal,
        Goal::Explore(..)
    ));

    let mut peer = object(2, WIZARD, 19.5, 11.5);
    peer.name = "Trellis".into();
    peer.equipped = v.character.items[..4].to_vec();
    v.objects.push(peer);
    let mut leader = Brain::new(PROFILES[1], &v);
    leader.decide(&v, &content).unwrap();
    v.objects.clear();
    v.position.x = 25.5;
    v.tick += 4;
    v.time_ms += 200;
    let search = leader.decide(&v, &content).unwrap();
    assert_eq!(search.goal, Goal::Gather(2));
    assert!(search.movement.is_some());
}
