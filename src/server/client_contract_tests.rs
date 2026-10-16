use super::*;

const INVALID: &str = "<Error>Account credentials not valid</Error>";

async fn response(response: Response) -> (StatusCode, String) {
    assert_eq!(response.headers()["content-type"], "application/xml");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

async fn list(a: &App, guid: &str, password: &str, extra: &[(&str, &str)]) -> (StatusCode, String) {
    let form = [("guid", guid), ("password", password)]
        .into_iter()
        .chain(extra.iter().copied())
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
    response(
        chars(
            State(a.clone()),
            ConnectInfo("127.0.0.1:1234".parse().unwrap()),
            HeaderMap::new(),
            Form(form),
        )
        .await,
    )
    .await
}

#[tokio::test]
async fn chars_translates_only_typed_credential_rejections_to_legacy_xml() {
    let (dir, a) = account_actions::tests::fixture();
    let account = a
        .store
        .register(
            "converted".into(),
            "owner@example.org".into(),
            "valid-password".into(),
        )
        .await
        .unwrap();
    for (guid, password) in [
        ("unknown@example.org", "valid-password"),
        ("owner@example.org", "wrong-password"),
        ("owner@example.org", ""),
        ("converted", ""),
        ("not an email", "valid-password"),
        ("owner@example.org", "x"),
    ] {
        assert_eq!(
            list(&a, guid, password, &[]).await,
            (StatusCode::OK, INVALID.into()),
            "{guid}"
        );
    }
    for (guid, password) in [("fresh-guest", ""), ("owner@example.org", "valid-password")] {
        let (status, xml) = list(&a, guid, password, &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            roxmltree::Document::parse(&xml)
                .unwrap()
                .root_element()
                .tag_name()
                .name(),
            "Chars"
        );
    }
    let (status, xml) = list(&a, "fresh-guest", "", &[("secret", "external-token")]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_ne!(xml, INVALID);
    assert!(xml.contains("external platform authentication"));
    let db = rusqlite::Connection::open(dir.join("game.db")).unwrap();
    db.execute(
        "UPDATE credentials SET password_hash='corrupt' WHERE account=?1",
        [account],
    )
    .unwrap();
    let (status, xml) = list(&a, "owner@example.org", "valid-password", &[]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_ne!(xml, INVALID);
    assert!(xml.contains("invalid stored password verifier"));
    db.execute_batch("DROP TABLE credentials").unwrap();
    let (status, xml) = list(&a, "owner@example.org", "valid-password", &[]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_ne!(xml, INVALID);
    assert!(xml.contains("no such table"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn chars_origin_and_rate_denials_do_not_masquerade_as_credentials() {
    let (dir, a) = account_actions::tests::fixture();
    let mut headers = HeaderMap::new();
    headers.insert("origin", "https://unapproved.example".parse().unwrap());
    let (status, xml) = response(
        chars(
            State(a.clone()),
            ConnectInfo("127.0.0.1:1234".parse().unwrap()),
            headers,
            Form(HashMap::new()),
        )
        .await,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(xml.contains("unapproved request origin"));
    assert_ne!(xml, INVALID);
    assert!(a.requests.lock().unwrap().is_empty());
    for _ in 0..20 {
        assert_eq!(
            list(&a, "bad identity", "", &[]).await,
            (StatusCode::OK, INVALID.into())
        );
    }
    let (status, xml) = list(&a, "bad identity", "", &[]).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_ne!(xml, INVALID);
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn delete_response_preserves_character_inventory_and_fame_wallet() {
    let (dir, a) = account_actions::tests::fixture();
    let account = a.store.list("delete-owner".into()).await.unwrap().id;
    let mut dead = a
        .store
        .create(account, a.content.get(WIZARD).unwrap())
        .await
        .unwrap();
    dead.hp = 0;
    dead.alive = false;
    dead.fame = 123;
    a.store.death(dead, "Pirate".into()).await.unwrap();
    let ch = a
        .store
        .create(account, a.content.get(WIZARD).unwrap())
        .await
        .unwrap();
    let before = serde_json::to_value(a.store.list("delete-owner".into()).await.unwrap()).unwrap();
    let inventory = serde_json::to_value(a.store.load(account, ch.id).await.unwrap()).unwrap();
    assert_eq!(before["fame"], 123);
    let result = response(
        delete_character(
            State(a.clone()),
            ConnectInfo("127.0.0.1:1234".parse().unwrap()),
            HeaderMap::new(),
        )
        .await,
    )
    .await;
    assert_eq!(
        result,
        (
            StatusCode::OK,
            "<Error>Character deletion is not implemented; no character was deleted.</Error>"
                .into()
        )
    );
    assert_eq!(
        serde_json::to_value(a.store.list("delete-owner".into()).await.unwrap()).unwrap(),
        before
    );
    assert_eq!(
        serde_json::to_value(a.store.load(account, ch.id).await.unwrap()).unwrap(),
        inventory
    );
    std::fs::remove_dir_all(dir).unwrap();
}

fn use_item(object: i32, slot: u8) -> Packet {
    let mut out = Writer::new();
    out.i32(1).i32(object).u8(slot).i16(0xa22).f32(0.).f32(0.);
    Packet {
        id: 30,
        body: out.0,
    }
}

async fn loaded(a: &App, guid: &str) -> Session {
    let account = a.store.list(guid.into()).await.unwrap().id;
    let ch = a
        .store
        .create(account, a.content.get(WIZARD).unwrap())
        .await
        .unwrap();
    let player = a.world.call(move |w| w.enter(ch, None)).await.unwrap();
    Session {
        account: Some(a.claim_session(account).unwrap()),
        player: Some(player),
        ..Session::default()
    }
}

#[tokio::test]
async fn rejected_container_and_foreign_useitem_reply_then_owned_potion_commits() {
    let (dir, mut a) = account_actions::tests::fixture();
    a.world = a.world.in_world(-2);
    a.world.clock.set_paused(true);
    let mut session = loaded(&a, "potion-owner").await;
    let mut foreign = loaded(&a, "potion-foreign").await;
    let player = session.player.unwrap();
    let other = foreign.player.unwrap();
    let (bag, before, foreign_before) = a
        .world
        .call(move |w| {
            let p = w.players.get_mut(&player).unwrap();
            p.character.hp = 20;
            let before = p.character.clone();
            assert_eq!(before.items[4], 0xa22);
            let pos = p.pos;
            let bag = w.spawn_entity(0x500, pos, vec![0xa22, -1, -1, -1, -1, -1, -1, -1])?;
            Ok((bag, before, w.players[&other].character.clone()))
        })
        .await
        .unwrap();
    let (tx, mut rx) = mpsc::channel(8);
    let (lost, _) = tokio::sync::watch::channel(false);
    for (object, slot) in [(bag, 0), (other, 4)] {
        handle(
            &mut a,
            use_item(object, slot),
            &mut session,
            &tx,
            &lost,
            std::time::Instant::now(),
        )
        .await
        .unwrap();
        let chat = rx.try_recv().unwrap();
        let expected = crate::world::chat_error("item ownership mismatch");
        assert_eq!((chat.id, chat.body), (expected.id, expected.body));
        let result = rx.try_recv().unwrap();
        assert_eq!((result.id, result.body), (4, 1i32.to_be_bytes().to_vec()));
        assert!(rx.try_recv().is_err());
        let (current, other_current, items, reserved) = a
            .world
            .call(move |w| {
                Ok((
                    w.players[&player].character.clone(),
                    w.players[&other].character.clone(),
                    w.entities[&bag].items.clone(),
                    w.entities[&bag].reserved,
                ))
            })
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(current).unwrap(),
            serde_json::to_value(&before).unwrap()
        );
        assert_eq!(
            serde_json::to_value(other_current).unwrap(),
            serde_json::to_value(&foreign_before).unwrap()
        );
        assert_eq!(items, vec![0xa22, -1, -1, -1, -1, -1, -1, -1]);
        assert!(!reserved);
        let saved = a.store.load(before.account, before.id).await.unwrap();
        assert_eq!(
            (saved.items, saved.revision),
            (before.items.clone(), before.revision)
        );
    }
    handle(
        &mut a,
        use_item(player, 4),
        &mut session,
        &tx,
        &lost,
        std::time::Instant::now(),
    )
    .await
    .unwrap();
    let after = a
        .world
        .call(move |w| Ok(w.players[&player].character.clone()))
        .await
        .unwrap();
    assert_eq!(after.items[4], -1);
    assert!(after.hp > before.hp);
    assert_eq!(after.revision, before.revision + 1);
    let saved = a.store.load(before.account, before.id).await.unwrap();
    assert_eq!((saved.items, saved.revision), (after.items, after.revision));
    for s in [&mut session, &mut foreign] {
        depart(&a, s.player.take().unwrap()).await.unwrap();
        s.account.take().unwrap().release();
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn rejected_useitem_chat_and_invresult_queue_failures_stay_typed_fatal() {
    let (dir, mut a) = account_actions::tests::fixture();
    a.world = a.world.in_world(-2);
    a.world.clock.set_paused(true);
    let mut session = loaded(&a, "queue-potion").await;
    for (closed, prefilled) in [(false, true), (false, false), (true, false)] {
        let (tx, rx) = mpsc::channel(1);
        let mut rx = if closed {
            drop(rx);
            None
        } else {
            Some(rx)
        };
        if prefilled {
            tx.try_send(Packet {
                id: 255,
                body: vec![],
            })
            .unwrap();
        }
        let (lost, _) = tokio::sync::watch::channel(false);
        let error = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            handle(
                &mut a,
                use_item(i32::MAX, 4),
                &mut session,
                &tx,
                &lost,
                std::time::Instant::now(),
            ),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert_eq!(
            error.downcast_ref::<ReplyQueueError>(),
            Some(&if closed {
                ReplyQueueError::Closed
            } else {
                ReplyQueueError::Full
            })
        );
        if let Some(rx) = rx.as_mut() {
            assert_eq!(
                rx.try_recv().unwrap().id,
                if prefilled { 255 } else { wire::TEXT }
            );
            assert!(rx.try_recv().is_err());
        }
    }
    depart(&a, session.player.take().unwrap()).await.unwrap();
    session.account.take().unwrap().release();
    std::fs::remove_dir_all(dir).unwrap();
}
