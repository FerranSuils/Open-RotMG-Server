use super::*;

fn created_on(xml: &str) -> u64 {
    roxmltree::Document::parse(xml)
        .unwrap()
        .descendants()
        .find(|node| node.has_tag_name("CreatedOn"))
        .unwrap()
        .text()
        .unwrap()
        .parse()
        .unwrap()
}

#[tokio::test]
async fn fame_serializes_recorded_death_not_birth_and_preserves_snapshot() {
    let content = Content::load(Path::new("data")).unwrap();
    let s = Store::open(Path::new(":memory:"), &content).unwrap();
    let account = s.list("death-date".into()).await.unwrap().id;
    let mut ch = s
        .create(account, content.get(WIZARD).unwrap())
        .await
        .unwrap();
    ch.created_at = 86_399;
    ch.hp = 0;
    ch.alive = false;
    ch.fame = 150;
    ch.name = "A<&>".into();
    s.death(ch.clone(), "Pirate<&>".into()).await.unwrap();
    let original = serde_json::to_string(&ch).unwrap();
    for died_at in [86_400_u64, 86_401, 8_640_000_000_000] {
        s.call(move |db| {
            db.execute("UPDATE deaths SET died_at=?1", [died_at])?;
            Ok(())
        })
        .await
        .unwrap();
        let xml = s.fame(account, ch.id).await.unwrap();
        assert_eq!(created_on(&xml), died_at);
        let doc = roxmltree::Document::parse(&xml).unwrap();
        for (tag, expected) in [
            ("Name", "A<&>"),
            ("KilledBy", "Pirate<&>"),
            ("TotalFame", "150"),
        ] {
            assert_eq!(
                doc.descendants()
                    .find(|node| node.has_tag_name(tag))
                    .unwrap()
                    .text(),
                Some(expected)
            );
        }
        let snapshot: String = s
            .call(|db| Ok(db.query_row("SELECT snapshot FROM deaths", [], |row| row.get(0))?))
            .await
            .unwrap();
        assert_eq!(snapshot, original);
        assert_eq!(
            serde_json::from_str::<Character>(&snapshot)
                .unwrap()
                .created_at,
            86_399
        );
    }
    s.call(|db| {
        db.execute("UPDATE deaths SET died_at=86400", [])?;
        Ok(())
    })
    .await
    .unwrap();
    let legends = s.legends("all".into(), account, ch.id).await.unwrap();
    let doc = roxmltree::Document::parse(&legends).unwrap();
    let row = doc
        .descendants()
        .find(|node| node.has_tag_name("FameListElem"))
        .unwrap();
    assert_eq!(
        row.attribute("accountId").unwrap().parse::<i32>().unwrap(),
        account
    );
    let clicked = row.attribute("charId").unwrap().parse().unwrap();
    assert_eq!(clicked, ch.id);
    assert_eq!(created_on(&s.fame(account, clicked).await.unwrap()), 86_400);
    assert!(s.fame(account + 1, clicked).await.is_err());
    let snapshot: String = s
        .call(|db| Ok(db.query_row("SELECT snapshot FROM deaths", [], |row| row.get(0))?))
        .await
        .unwrap();
    assert_eq!(snapshot, original);
}

#[tokio::test]
async fn fame_accepts_legacy_snapshot_without_birth_but_rejects_invalid_death_time() {
    let content = Content::load(Path::new("data")).unwrap();
    let s = Store::open(Path::new(":memory:"), &content).unwrap();
    let account = s.list("legacy-death".into()).await.unwrap().id;
    let mut ch = s
        .create(account, content.get(WIZARD).unwrap())
        .await
        .unwrap();
    ch.hp = 0;
    ch.alive = false;
    s.death(ch.clone(), "Pirate".into()).await.unwrap();
    let mut legacy = serde_json::to_value(&ch).unwrap();
    legacy.as_object_mut().unwrap().remove("created_at");
    let legacy = legacy.to_string();
    let raw = legacy.clone();
    s.call(move |db| {
        db.execute("UPDATE deaths SET snapshot=?1,died_at=86400", [raw])?;
        Ok(())
    })
    .await
    .unwrap();
    assert_eq!(created_on(&s.fame(account, ch.id).await.unwrap()), 86_400);
    for died_at in [-1_i64, 0, 8_640_000_000_001, i64::MAX] {
        s.call(move |db| {
            db.execute("UPDATE deaths SET died_at=?1", [died_at])?;
            Ok(())
        })
        .await
        .unwrap();
        assert!(s.fame(account, ch.id).await.is_err(), "accepted {died_at}");
    }
    let snapshot: String = s
        .call(|db| Ok(db.query_row("SELECT snapshot FROM deaths", [], |row| row.get(0))?))
        .await
        .unwrap();
    assert_eq!(snapshot, legacy);
}
