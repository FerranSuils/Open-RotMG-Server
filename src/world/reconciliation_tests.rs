use super::reconciliation::{NativeInput, goto};
use super::tests::{ch, world};
use super::*;
use std::time::Duration;

fn native() -> (World, i32, mpsc::Receiver<Packet>, watch::Receiver<bool>) {
    let mut w = world();
    let (packets, rx) = mpsc::channel(4096);
    let (lost, loss) = watch::channel(false);
    let id = w.enter(ch(&w), Some(Output { packets, lost })).unwrap();
    w.attack_epoch = Instant::now() - Duration::from_secs(2);
    let p = w.players.get_mut(&id).unwrap();
    p.last_move = 1000;
    p.last_move_at = 0;
    p.movement_credit = 0.;
    (w, id, rx, loss)
}

#[test]
fn correction_discards_buffered_input_without_granting_authority() {
    let (mut w, id, _rx, _loss) = native();
    let pos = w.players[&id].pos;
    let received = w.attack_epoch + Duration::from_millis(100);
    let forged = Pos {
        x: pos.x + 0.7,
        y: pos.y,
    };
    let error = w
        .native_move_received(id, forged, (1100, received))
        .unwrap_err();
    assert!(error.to_string().contains("server time budget"));
    w.correct_position(id, 1100, &error.to_string()).unwrap();
    assert_eq!(w.players[&id].last_move, 1000);
    assert_eq!(
        w.native_move_received(id, forged, (1200, received))
            .unwrap(),
        NativeInput::DiscardedUntilGotoAck
    );
    assert_eq!(
        w.native_shoot_received(
            id,
            (1100, received),
            1,
            0xa97,
            Pos {
                x: pos.x + 100.,
                y: pos.y
            },
            0.
        )
        .unwrap(),
        NativeInput::DiscardedUntilGotoAck
    );
    for (time, bullet, kind, angle) in [
        (1000, 2, 0xa97, 0.),
        (1200, 99, 0xa97, 0.),
        (1200, 2, 0xa97, f32::NAN),
        (1200, 2, 0xa98, 0.),
    ] {
        assert!(
            w.native_shoot_received(id, (time, received), bullet, kind, pos, angle)
                .is_err()
        );
        assert_eq!(w.players[&id].next_bullet, 2);
    }
    let p = &w.players[&id];
    assert_eq!(p.pos.distance(pos), 0.);
    assert_eq!(p.movement_credit, 0.);
    assert_eq!(p.last_move_at, 0);
    assert_eq!(p.last_attack, -1000);
    assert_eq!(p.last_attack_at, 0);
    assert_eq!(p.attack_credit, 1000.);
    assert_eq!(p.attack_count, 0);
    assert_eq!(p.character.shots, 0);
    assert!(w.bullets.is_empty());
    assert_eq!(p.next_bullet, 2);
    w.acknowledge_goto(id, 1100).unwrap();
    assert_eq!(w.players[&id].movement_credit, 0.);
    assert_eq!(w.players[&id].last_move_at, 0);
    // An ACK cannot grant future processing-time funding.
    assert!(
        w.native_move_received(id, forged, (1200, received))
            .is_err()
    );
    w.native_move_received(
        id,
        Pos {
            x: pos.x + 0.4,
            y: pos.y,
        },
        (1200, received),
    )
    .unwrap();
    assert_eq!(w.players[&id].last_move_at, 100);
}

#[test]
fn a_discarded_volley_stays_discarded_across_ack_and_sequence_recovers() {
    let (mut w, id, _rx, _loss) = native();
    let pos = w.players[&id].pos;
    let received = w.attack_epoch + Duration::from_millis(500);
    w.correct_position(id, 1100, "fixture rejected movement")
        .unwrap();
    for bullet in 1..=2 {
        if bullet == 2 {
            w.acknowledge_goto(id, 1100).unwrap();
        }
        assert_eq!(
            w.native_shoot_received(id, (1100, received), bullet, 0xa97, pos, 0.)
                .unwrap(),
            NativeInput::DiscardedUntilGotoAck
        );
    }
    assert!(w.bullets.is_empty());
    assert!(
        w.native_shoot_received(id, (1100, received), 3, 0xa97, pos, 0.)
            .is_err()
    );
    assert_eq!(w.players[&id].next_bullet, 3);
    assert!(
        w.native_shoot_received(
            id,
            (2100, received),
            3,
            0xa97,
            Pos {
                x: pos.x + 10.,
                y: pos.y
            },
            0.
        )
        .is_err()
    );
    assert_eq!(w.players[&id].next_bullet, 3);
    assert_eq!(w.players[&id].character.shots, 0);
    assert_eq!(
        w.native_shoot_received(
            id,
            (2100, received),
            3,
            0xa97,
            Pos {
                x: pos.x + 0.3,
                y: pos.y
            },
            0.
        )
        .unwrap(),
        NativeInput::Applied
    );
    assert_eq!(w.players[&id].character.shots, 1);
    assert_eq!(w.players[&id].next_bullet, 4);
    assert_eq!(w.bullets.len(), 1);
}

#[test]
fn goto_ack_fifo_includes_other_players_and_rejects_unsolicited_or_bad_clocks() {
    let (mut w, id, _rx, _loss) = native();
    goto(w.players.get_mut(&id).unwrap(), false, vec![]);
    w.correct_position(id, 1100, "fixture correction").unwrap();
    assert!(w.acknowledge_goto(id, -1).is_err());
    assert_eq!(w.players[&id].goto_acks.len(), 2);
    w.acknowledge_goto(id, 1500).unwrap();
    assert_eq!(w.players[&id].last_move, 1000);
    assert!(w.players[&id].correcting().unwrap());
    assert!(w.acknowledge_goto(id, 999).is_err());
    assert_eq!(w.players[&id].goto_acks.len(), 1);
    w.acknowledge_goto(id, 1100).unwrap();
    assert!(!w.players[&id].correcting().unwrap());
    assert!(w.acknowledge_goto(id, 1100).is_err());
}

#[test]
fn goto_ack_backlog_and_real_time_timeout_fail_closed() {
    let (mut w, id, _rx, loss) = native();
    for _ in 0..64 {
        goto(w.players.get_mut(&id).unwrap(), false, vec![]);
    }
    assert!(!*loss.borrow());
    goto(w.players.get_mut(&id).unwrap(), false, vec![]);
    assert!(*loss.borrow());
    assert_eq!(w.players[&id].goto_acks.len(), 64);
    let (mut w, id, _rx, loss) = native();
    w.correct_position(id, 1100, "fixture correction").unwrap();
    w.players
        .get_mut(&id)
        .unwrap()
        .goto_acks
        .front_mut()
        .unwrap()
        .1 = Instant::now() - Duration::from_secs(6);
    assert!(w.acknowledge_goto(id, 1100).is_err());
    w.step();
    assert!(w.players[&id].departing);
    assert!(*loss.borrow());
    assert_eq!(w.players[&id].movement_credit, 0.);
}
