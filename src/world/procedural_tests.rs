use super::*;
use crate::dungeon::procedural::{PORTAL_DROPS, THEMES};

// Each themed dungeon is reachable from an overworld boss portal, validates its roster on
// creation, lets players break corridor seals, persists boss clears and runs its encounters.
#[test]
fn overworld_bosses_open_playable_themed_dungeons_with_breakable_seals() {
    let mut d = tests::directory();
    for (i, t) in THEMES.iter().enumerate() {
        let &(boss, ..) = PORTAL_DROPS
            .iter()
            .find(|&&(_, theme, chance)| THEMES[theme].portal == t.portal && chance >= 1.)
            .expect("every theme has a guaranteed overworld boss drop");
        let realm = d.worlds.get_mut(&-3).unwrap();
        let (x, y) = realm.map.spawns[0];
        let enemy = realm.spawn_entity(boss, Pos { x, y }, vec![]).unwrap();
        realm.defeat_entity(enemy).unwrap();
        d.step();
        let destination = d
            .portals
            .iter()
            .filter(|(id, p)| {
                p.origin == -3
                    && d.worlds[&-3]
                        .entities
                        .get(id)
                        .is_some_and(|e| e.kind == t.portal)
            })
            .find_map(|(_, p)| p.destination)
            .unwrap_or_else(|| panic!("boss defeat binds the themed portal: {}", t.name));
        let w = d.worlds.get_mut(&destination).unwrap();
        assert_eq!(w.name, t.name);

        let seal = w
            .entities
            .values()
            .find(|e| e.kind == t.breakable)
            .expect("dungeon has a seal")
            .id;
        w.damage_enemy(0, seal, 100_000, true, &[], None).unwrap();
        assert!(
            !w.entities.contains_key(&seal),
            "{}: breakable wall survived lethal damage",
            t.name
        );
        let wall = w
            .entities
            .values()
            .find(|e| e.kind == t.wall)
            .expect("dungeon has walls")
            .id;
        assert!(
            !w.content.objects[&t.wall].enemy,
            "ordinary walls stay indestructible"
        );
        assert!(w.entities.contains_key(&wall));

        let player = w.enter(tests::ch(w), None).unwrap();
        let near = w.players[&player].pos;
        let boss = w
            .actors()
            .find(|e| e.kind == t.boss)
            .expect("generated boss")
            .id;
        w.players.get_mut(&player).unwrap().pos = w.entities[&boss].pos;
        w.damage_enemy(player, boss, 1_000_000, true, &[], None)
            .unwrap();
        assert!(w.cleared, "{}: boss defeat did not clear", t.name);
        let (id, snapshot) = w
            .pending_progress_where(|_, _| true)
            .into_iter()
            .next()
            .expect("boss clear queues character progress");
        let (mut expected, mut more) = ([0; 4], [0; 5]);
        if i < 4 {
            expected[i] = 1;
        } else {
            more[i - 4] = 1;
        }
        assert_eq!(snapshot.themed_dungeons, expected, "{}", t.name);
        assert_eq!(snapshot.themed_more, more, "{}", t.name);
        w.finish_progress(id, snapshot).unwrap();
        assert_eq!(w.players[&player].character.themed_dungeons, expected);
        assert_eq!(w.players[&player].character.themed_more, more);
        let mut legacy = serde_json::to_value(&w.players[&player].character).unwrap();
        legacy.as_object_mut().unwrap().remove("themed_dungeons");
        let legacy: Character = serde_json::from_value(legacy).unwrap();
        assert_eq!(
            legacy.themed_dungeons, [0; 4],
            "older saves load with zero clears"
        );
        w.players.get_mut(&player).unwrap().pos = near;
        let fighters: Vec<_> = t
            .mobs
            .iter()
            .chain([&t.boss])
            .zip([(2., 0.), (-2., 0.), (0., 2.), (0., -2.), (2., 2.)])
            .map(|(kind, (dx, dy))| {
                let pos = Pos {
                    x: near.x + dx,
                    y: near.y + dy,
                };
                w.spawn_entity(*kind, pos, vec![]).unwrap()
            })
            .collect();
        let start: Vec<_> = fighters.iter().map(|id| w.entities[id].pos).collect();
        let hp = w.players[&player].character.hp;
        for _ in 0..200 {
            w.step();
        }
        let moved = fighters
            .iter()
            .zip(start)
            .any(|(id, p)| w.entities.get(id).is_none_or(|e| e.pos.distance(p) > 0.1));
        let hurt = w.players.get(&player).is_none_or(|p| p.character.hp < hp);
        assert!(
            moved || hurt,
            "{}: encounters stayed inert for ten seconds",
            t.name
        );
        // Nine themes would exceed the test directory's instance budget: release each one.
        d.worlds.remove(&destination);
    }
}
