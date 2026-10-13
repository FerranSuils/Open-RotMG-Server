use super::tests::{ch, world};
use super::*;

#[test]
fn controllers_are_exclusive_and_health_counts_real_players() {
    let mut w = world();
    let (packets, _rx) = mpsc::channel(64);
    let (lost, _) = watch::channel(false);
    let human = w.enter(ch(&w), Some(Output { packets, lost })).unwrap();
    assert!(w.autonomous_action(human, Action::Stop).is_err());
    assert!(w.action(human, Action::Stop).is_err());
    let mut character = ch(&w);
    character.account = 2;
    character.id = 2;
    let qa = w.enter(character, None).unwrap();
    assert!(w.autonomous_action(qa, Action::Stop).is_err());
    let mut character = ch(&w);
    character.account = 3;
    character.id = 3;
    let (packets, mut rx) = mpsc::channel(64);
    let (lost, _) = watch::channel(false);
    let bot = w
        .enter_autonomous(character, Output { packets, lost })
        .unwrap();
    while rx.try_recv().is_ok() {}
    assert!(w.action(bot, Action::Stop).is_err());
    let initial = w.players[&bot].pos;
    w.autonomous_action(
        bot,
        Action::Move {
            x: initial.x + 2.,
            y: initial.y,
        },
    )
    .unwrap();
    w.step();
    assert!(w.players[&bot].pos.distance(initial) > 0.);
    assert!(w.players[&bot].pos.distance(initial) < 0.5);
    assert!(w.players[&human].bot_goal.is_none());
    w.step();
    assert!(rx.try_recv().is_ok());
    let mut d = tests::directory();
    d.worlds.clear();
    d.worlds.insert(-2, w);
    let health = d.health(Instant::now());
    assert_eq!(health["players"], 1);
    assert_eq!(health["qa_actors"], 1);
    assert_eq!(health["bots"], 1);
}

#[test]
fn autonomous_actions_use_normal_inventory_death_and_quarantine_guards() {
    let mut w = world();
    let (packets, _rx) = mpsc::channel(64);
    let (lost, _) = watch::channel(false);
    let bot = w
        .enter_autonomous(ch(&w), Output { packets, lost })
        .unwrap();
    let items = w.players[&bot].character.items.clone();
    let action = Action::Swap {
        from: Slot {
            object: bot,
            slot: 4,
            kind: items[4],
        },
        to: Slot {
            object: bot,
            slot: 0,
            kind: items[0],
        },
    };
    assert!(w.autonomous_action(bot, action).is_err());
    w.players.get_mut(&bot).unwrap().pending = true;
    assert!(
        w.autonomous_action(
            bot,
            Action::Use {
                slot: 4,
                x: 0.,
                y: 0.
            }
        )
        .is_err()
    );
    w.players.get_mut(&bot).unwrap().pending = false;
    w.players.get_mut(&bot).unwrap().death_pending = true;
    assert!(
        w.autonomous_action(bot, Action::Shoot { angle: 0. })
            .is_err()
    );
    w.players.get_mut(&bot).unwrap().death_pending = false;
    w.poison(bot, "test persistence failure".into()).unwrap();
    assert!(
        w.autonomous_action(bot, Action::Move { x: 1., y: 1. })
            .is_err()
    );
    assert_eq!(w.players[&bot].character.items, items);
}
