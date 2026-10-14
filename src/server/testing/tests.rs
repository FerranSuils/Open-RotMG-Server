use super::*;

fn fixture() -> (PathBuf, App) {
    let (dir, mut app) = crate::server::account_actions::tests::fixture();
    write_json(
        &dir.join("testing.json"),
        &json!({"schema":1,"provenance":"prepared"}),
    )
    .unwrap();
    app.testing = Testing::open(&dir).unwrap();
    app.control = Some("test-token".into());
    app.world.clock.set_paused(true);
    (dir, app)
}
async fn request(app: &App, id: &str, command: Value) -> Value {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer test-token".parse().unwrap());
    let response = super::command(
        State(app.clone()),
        headers,
        Json(json!({"id":id,"command":command})),
    )
    .await;
    serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1_048_576)
            .await
            .unwrap(),
    )
    .unwrap()
}
fn spec(pos: Pos) -> Value {
    json!({"guid":"qa-interface-unit","class":WIZARD,"level":8,"seed":12351,"equipment":[2712,-1,-1,-1],"backpack":[2599,2594,-1,-1,-1,-1,-1,-1],"world":-1,"position":pos,"controller":"direct","stats":null})
}
#[tokio::test]
async fn explicit_spawn_idempotency_and_compact_observations() {
    let (dir, app) = fixture();
    let pos = app
        .world
        .in_world(-1)
        .call(|w| {
            let (x, y) = w.map.spawns[0];
            Ok(Pos { x, y })
        })
        .await
        .unwrap();
    let mut incomplete = spec(pos);
    incomplete.as_object_mut().unwrap().remove("equipment");
    assert_eq!(
        request(&app, "missing", json!({"op":"spawn","spec":incomplete})).await["status"],
        "failed"
    );
    let mut incompatible = spec(pos);
    incompatible["equipment"][0] = 2599.into();
    assert_eq!(
        request(&app, "bad-gear", json!({"op":"spawn","spec":incompatible})).await["status"],
        "failed"
    );
    let cmd = json!({"op":"spawn","spec":spec(pos)});
    let created = request(&app, "create", cmd.clone()).await;
    assert_eq!(created["status"], "succeeded");
    assert_eq!(request(&app, "create", cmd).await, created);
    assert_eq!(
        request(&app, "create", json!({"op":"resume"})).await["status"],
        "failed"
    );
    let actor = created["result"]["actor"].as_i64().unwrap();
    let view = request(&app, "view", json!({"op":"observe","actor":actor})).await;
    assert_eq!(view["character"]["level"], 8);
    assert_eq!(view["equipment"][0]["name"], "Firebrand Staff");
    assert!(view.get("tiles").is_none());
    assert!(serde_json::to_vec(&view).unwrap().len() < 6000);
    let trace = request(
        &app,
        "trace",
        json!({"op":"trace","request":"create","offset":0,"limit":1}),
    )
    .await;
    assert_eq!(trace["events"].as_array().unwrap().len(), 1);
    assert_eq!(
        request(
            &app,
            "trace-path",
            json!({"op":"trace","request":"../control.token","offset":0,"limit":1})
        )
        .await["status"],
        "failed"
    );
    assert_eq!(
        request(
            &app,
            "trace-limit",
            json!({"op":"trace","request":"create","offset":0,"limit":41})
        )
        .await["status"],
        "failed"
    );
    app.shutdown.send_replace(true);
    fs::remove_dir_all(dir).unwrap();
}
#[tokio::test]
async fn ordinary_equipment_action_is_committed_and_pause_is_stable() {
    let (dir, app) = fixture();
    let pos = app
        .world
        .in_world(-1)
        .call(|w| {
            let (x, y) = w.map.spawns[0];
            Ok(Pos { x, y })
        })
        .await
        .unwrap();
    let created = request(&app, "create", json!({"op":"spawn","spec":spec(pos)})).await;
    let actor = created["result"]["actor"].as_i64().unwrap();
    let before = request(&app, "before", json!({"op":"observe","actor":actor})).await;
    let player = before["session"]["player"].as_i64().unwrap();
    let action = json!({"command":"swap","from":{"object":player,"slot":4,"kind":2599},"to":{"object":player,"slot":3,"kind":-1}});
    let result = request(
        &app,
        "equip",
        json!({"op":"act","actor":actor,"action":action,"duration_ms":0}),
    )
    .await;
    assert_eq!(result["status"], "succeeded");
    assert_eq!(result["paused"], true);
    let after = request(&app, "after", json!({"op":"observe","actor":actor})).await;
    assert!(
        after["character"]["stats"]["max_hp"].as_i64().unwrap()
            > before["character"]["stats"]["max_hp"].as_i64().unwrap()
    );
    let saved = app
        .store
        .load(
            after["session"]["account"].as_i64().unwrap() as i32,
            actor as i32,
        )
        .await
        .unwrap();
    assert_eq!(saved.items[3], 2599);
    assert_eq!(
        saved.revision,
        after["character"]["inventory_revision"].as_i64().unwrap() as i32
    );
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        request(&app, "after-wait", json!({"op":"observe","actor":actor})).await,
        after
    );
    assert_eq!(
        request(
            &app,
            "not-bot",
            json!({"op":"run","actor":actor,"goal":{"kind":"level","level":9},"max_game_ms":1000})
        )
        .await["status"],
        "failed"
    );
    app.shutdown.send_replace(true);
    fs::remove_dir_all(dir).unwrap();
}
#[tokio::test]
async fn actual_gameplay_controller_can_run_a_bounded_time_goal() {
    let (dir, app) = fixture();
    let pos = app
        .world
        .in_world(-2)
        .call(|w| {
            let (x, y) = w.map.spawns[0];
            Ok(Pos { x, y })
        })
        .await
        .unwrap();
    let mut spawn = spec(pos);
    spawn["controller"] = "game_bot".into();
    spawn["world"] = (-2).into();
    let created = request(&app, "create", json!({"op":"spawn","spec":spawn})).await;
    assert_eq!(created["status"], "succeeded");
    let actor = created["result"]["actor"].as_i64().unwrap();
    let result = request(
        &app,
        "bounded",
        json!({"op":"run","actor":actor,"goal":{"kind":"time","ms":100},"max_game_ms":1000}),
    )
    .await;
    assert_eq!(result["result"]["goal_reached"], true);
    assert_eq!(result["paused"], true);
    assert_eq!(result["result"]["state"]["lifecycle"]["output"], "live");
    app.shutdown.send_replace(true);
    fs::remove_dir_all(dir).unwrap();
}
#[tokio::test]
async fn controls_do_not_enable_the_testing_interface_on_ordinary_state() {
    let (dir, mut app) = fixture();
    app.testing = None;
    let response = request(&app, "no-test-state", json!({"op":"capabilities"})).await;
    assert_eq!(response["status"], "failed");
    assert!(
        response["error"]
            .as_str()
            .unwrap()
            .contains("exclusively created --test-state")
    );
    assert!(valid_id("../control.token").is_err());
    app.shutdown.send_replace(true);
    fs::remove_dir_all(dir).unwrap();
}
