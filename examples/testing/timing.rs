use super::{Pos, tests};

#[test]
fn example_timing() {
    let mut world = tests::world();
    let player = world.enter(tests::ch(&world), None).unwrap();
    let pos = world.players[&player].pos;
    world.players.get_mut(&player).unwrap().last_move = 1000;
    world.now = 100;
    let predicted = Pos {
        x: pos.x + 0.2,
        y: pos.y,
    };
    let muzzle = Pos {
        x: predicted.x + 0.3,
        y: predicted.y,
    };
    world.shoot(player, 1091, 1, 2711, muzzle, 0.).unwrap();
    world.move_to(player, predicted, 1091).unwrap();
    assert_eq!(world.players[&player].character.shots, 1);
}

#[test]
fn movement_clock_regression_is_explicit() {
    let mut world = tests::world();
    let player = world.enter(tests::ch(&world), None).unwrap();
    world.players.get_mut(&player).unwrap().last_move_at = 100;
    let error = world
        .move_to(player, world.players[&player].pos, 100)
        .unwrap_err();
    assert_eq!(error.to_string(), "movement server clock regressed");
}

#[test]
fn movement_failure_exposes_the_failed_budget() {
    let mut world = tests::world();
    let player = world.enter(tests::ch(&world), None).unwrap();
    let pos = world.players[&player].pos;
    let error = world
        .move_to(
            player,
            Pos {
                x: pos.x + 20.,
                y: pos.y,
            },
            100,
        )
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("distance=20")
            && error.contains("allowed=")
            && error.contains("client_elapsed_ms="),
        "{error}"
    );
}
