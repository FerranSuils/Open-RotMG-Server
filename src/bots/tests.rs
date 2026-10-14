use super::*;
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
            condition_mask: 0,
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
        equipped: vec![],
        contents: vec![],
        blocks: false,
        condition_mask: None,
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
    let goals: Vec<_> = PROFILES
        .iter()
        .map(|p| Brain::new(*p, &v).decide(&v, &content).unwrap().goal)
        .collect();
    assert_eq!(goals, vec![Goal::Recover, Goal::Loot(3), Goal::Follow(4)]);
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
    v.tick += 1;
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
