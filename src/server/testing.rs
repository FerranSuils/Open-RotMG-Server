use super::*;
#[cfg(test)]
mod tests;
use crate::store::Character;
use rand::{SeedableRng, rngs::StdRng};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::Path,
    time::{Duration, Instant},
};

type NativePings = BTreeMap<i32, (i32, tokio::sync::oneshot::Sender<()>)>;

#[derive(Clone)]
pub struct Testing {
    root: PathBuf,
    gate: Arc<tokio::sync::Mutex<()>>,
    cancelled: Arc<Mutex<BTreeSet<String>>>,
    native_serial: Arc<std::sync::atomic::AtomicI32>,
    native_pings: Arc<Mutex<NativePings>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spawn {
    pub guid: String,
    pub class: u16,
    pub level: i32,
    pub seed: u64,
    pub equipment: [i32; 4],
    pub backpack: [i32; 8],
    pub world: i32,
    pub position: Pos,
    pub controller: Controller,
    pub stats: Option<[i32; 8]>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Controller {
    Direct,
    Native,
    GameBot,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    id: String,
    command: Command,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Capabilities,
    Catalog {
        search: String,
        limit: usize,
    },
    WorldInfo {
        world: i32,
    },
    Spawn {
        spec: Spawn,
    },
    World {
        template: String,
    },
    Observe {
        actor: i32,
    },
    Inspect {
        actor: i32,
    },
    Act {
        actor: i32,
        action: Action,
        duration_ms: u64,
        #[serde(rename = "pauseAfter", default = "yes")]
        pause_after: bool,
    },
    Run {
        actor: i32,
        goal: Goal,
        max_game_ms: u64,
        #[serde(rename = "pauseAfter", default = "yes")]
        pause_after: bool,
    },
    Cheat {
        actor: i32,
        change: Cheat,
    },
    Pause {
        native_paused: Vec<i32>,
    },
    Resume,
    NativeSync {
        actors: Vec<i32>,
    },
    Result {
        request: String,
    },
    Trace {
        request: String,
        offset: usize,
        limit: usize,
    },
    Cancel {
        request: String,
    },
}
fn yes() -> bool {
    true
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Goal {
    Time { ms: u64 },
    Level { level: i32 },
    Kills { count: u64 },
    PirateCaves { count: u64 },
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Cheat {
    Stats { values: [i32; 8] },
    Item { slot: usize, item: i32 },
    Teleport { position: Pos },
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let temp = path.with_extension("tmp");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    file.write_all(&serde_json::to_vec(value)?)?;
    file.sync_all()?;
    fs::rename(&temp, path)?;
    fs::File::open(path.parent().context("artifact directory missing")?)?.sync_all()?;
    Ok(())
}
fn valid_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= 80
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)),
        "request ID must be 1–80 ASCII letters, numbers, '-' or '_'"
    );
    Ok(())
}
impl Testing {
    pub fn open(state: &Path) -> Result<Option<Self>> {
        if !state.join("testing.json").exists() {
            return Ok(None);
        }
        let marker: Value = serde_json::from_slice(&fs::read(state.join("testing.json"))?)?;
        ensure!(
            marker == json!({"schema":1,"provenance":"prepared"}),
            "invalid test-state marker"
        );
        let root = state.join("testing");
        fs::create_dir_all(&root)?;
        Ok(Some(Self {
            root,
            gate: Arc::default(),
            cancelled: Arc::default(),
            native_serial: Arc::new(std::sync::atomic::AtomicI32::new(-1)),
            native_pings: Arc::default(),
        }))
    }
    pub(super) fn acknowledge_native(&self, nonce: i32, player: i32) {
        let mut pending = self
            .native_pings
            .lock()
            .expect("native acknowledgement registry poisoned");
        if pending
            .get(&nonce)
            .is_some_and(|(expected, _)| *expected == player)
            && let Some((_, sender)) = pending.remove(&nonce)
            && sender.send(()).is_err()
        {
            tracing::warn!(
                nonce,
                player,
                "native acknowledgement arrived after its waiter closed"
            );
        }
    }
    fn path(&self, id: &str, suffix: &str) -> PathBuf {
        self.root.join(format!("{id}.{suffix}.json"))
    }
    fn registration(&self, actor: i32) -> Result<Spawn> {
        ensure!(actor > 0, "actor must be a positive durable character ID");
        Ok(serde_json::from_slice(
            &fs::read(self.path(&format!("actor-{actor}"), "spawn"))
                .context("actor was not spawned in this test session")?,
        )?)
    }
    pub fn native_entry(&self, account: i32) -> Result<Option<(i32, Pos)>> {
        let path = self.path(&format!("account-{account}"), "native");
        if !path.exists() {
            return Ok(None);
        }
        let entry: (i32, Pos) = serde_json::from_slice(&fs::read(path)?)?;
        Ok(Some(entry))
    }
    pub fn native_entered(&self, account: i32) -> Result<()> {
        let path = self.path(&format!("account-{account}"), "native");
        if path.exists() {
            fs::rename(&path, path.with_extension("consumed"))?;
        }
        Ok(())
    }
    fn record(&self, id: &str, event: Value) -> Result<()> {
        let path = self.root.join(format!("{id}.jsonl"));
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        serde_json::to_writer(&mut file, &event)?;
        file.write_all(b"\n")?;
        file.sync_data()?;
        Ok(())
    }
}
async fn resolve(a: &App, actor: i32) -> Result<i32> {
    a.world
        .directory(move |d| {
            let mut found = d
                .worlds
                .values()
                .flat_map(|w| w.players.iter())
                .filter(|(_, p)| p.character.id == actor)
                .map(|(id, _)| *id);
            let id = found
                .next()
                .context("selected character is not connected; no replacement was selected")?;
            ensure!(found.next().is_none(), "character has multiple sessions");
            Ok(id)
        })
        .await
}
async fn snapshot(a: &App, actor: i32, detail: bool) -> Result<Value> {
    a.testing
        .as_ref()
        .context("test session missing")?
        .registration(actor)?;
    let id = resolve(a, actor).await?;
    let content = a.content.clone();
    a.world.directory(move |d| {
        let view = d.player_view(id)?;
        let player = &d.worlds[&view.session.world].players[&id];
        let ch = &player.character;
        let controller = match player.controller { crate::world::Controller::Human => Controller::Native, crate::world::Controller::Qa => Controller::Direct, crate::world::Controller::Autonomous => Controller::GameBot };
        if detail { return Ok(json!({"view":view,"diagnostic":d.worlds[&view.session.world].observe(id)?,"privileged":true,"paused":d.clock.paused(),"pause_scope":"server","provenance":"prepared"})); }
        let item = |kind: i32| -> Result<Value> { if kind == -1 { Ok(Value::Null) } else { Ok(json!({"id":kind,"name":content.get(u16::try_from(kind)?)?.name})) } };
        let mut relevant = Vec::new();
        for object in &view.objects {
            let kind = content.get(object.kind)?;
            if kind.enemy || matches!(kind.class.as_str(), "Portal" | "Container" | "Player") { relevant.push(object); }
        }
        relevant.sort_by(|a,b|a.position.distance(view.position).total_cmp(&b.position.distance(view.position)));
        let nearby = relevant.iter().take(8).map(|o| Ok(json!({"id":o.id,"kind":o.kind,"name":if o.name.is_empty(){&content.get(o.kind)?.name}else{&o.name},"hp":o.hp,"position":o.position}))).collect::<Result<Vec<_>>>()?;
        let mut character = serde_json::to_value(&view.character)?;
        character["stats"] = json!({"max_hp":view.character.stats[0],"max_mp":view.character.stats[1],"attack":view.character.stats[2],"defense":view.character.stats[3],"speed":view.character.stats[4],"dexterity":view.character.stats[5],"vitality":view.character.stats[6],"wisdom":view.character.stats[7]});
        Ok(json!({"actor":actor,"controller":controller,"session":view.session,"world":view.map.name,"tick":view.tick,"game_ms":view.time_ms,"position":view.position,"character":character,"equipment":ch.items[..4].iter().map(|i|item(*i)).collect::<Result<Vec<_>>>()?,"lifecycle":view.lifecycle,"verification":{"shots":ch.shots,"kills":ch.kills,"pirate_caves":ch.pirate_caves},"nearby":nearby,"omitted_relevant_objects":relevant.len().saturating_sub(8),"paused":d.clock.paused(),"pause_scope":"server","knowledge_scope":"client_known_and_server_visible","provenance":"prepared"}))
    }).await
}
async fn native_sync(a: &App, t: &Testing, actors: Vec<i32>) -> Result<Value> {
    ensure!(
        !actors.is_empty()
            && actors.len() <= 8
            && actors.iter().collect::<BTreeSet<_>>().len() == actors.len(),
        "native synchronization requires 1–8 unique actors"
    );
    if a.world.clock.paused() {
        return Ok(json!({"already_paused":true}));
    }
    let mut acknowledgements = Vec::new();
    for actor in actors {
        let player = resolve(a, actor).await?;
        let nonce = t
            .native_serial
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        ensure!(
            nonce > i32::MIN && nonce < 0,
            "native synchronization serial budget exhausted"
        );
        let (sender, receiver) = tokio::sync::oneshot::channel();
        t.native_pings
            .lock()
            .expect("native acknowledgement registry poisoned")
            .insert(nonce, (player, sender));
        let result: Result<()> = async {
            a.world
                .directory(move |d| {
                    let view = d.player_view(player)?;
                    let p = &d.worlds[&view.session.world].players[&player];
                    ensure!(
                        p.controller == crate::world::Controller::Human,
                        "native synchronization requires the actual native controller"
                    );
                    let mut body = Writer::new();
                    body.i32(nonce);
                    crate::world::send(p, wire::PING, body.0);
                    Ok(())
                })
                .await?;
            tokio::time::timeout(Duration::from_secs(5), receiver)
                .await
                .context("native input acknowledgement timed out")?
                .context("native acknowledgement waiter closed")
        }
        .await;
        t.native_pings
            .lock()
            .expect("native acknowledgement registry poisoned")
            .remove(&nonce);
        result?;
        acknowledgements.push(json!({"actor":actor,"player":player,"nonce":nonce}));
    }
    Ok(json!({"acknowledged":acknowledgements}))
}
async fn pause(a: &App, native: &[i32]) -> Result<()> {
    let native = native.to_vec();
    a.world.directory(move |d| {
        d.clock.set_paused(true);
        for p in d.worlds.values().flat_map(|w|w.players.values()).filter(|p|p.controller == crate::world::Controller::Human) {
            ensure!(native.contains(&p.character.id), "native character {} needs coordinated Ruffle pause through the managed native client",p.character.id);
        }
        Ok(())
    }).await?;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if a.world
                .directory(|d| {
                    for w in d.worlds.values() {
                        if !w.settled()? {
                            return Ok(false);
                        }
                    }
                    Ok(true)
                })
                .await?
            {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .context("simulation stopped but persistence did not settle; inspect trace before resuming")?
}
async fn finish_action(
    a: &App,
    actor: i32,
    result: Result<Value>,
    pause_after: bool,
    nested: bool,
) -> Result<Value> {
    if !pause_after {
        return result;
    }
    match (result, pause(a, &[]).await) {
        (Err(primary), Err(cleanup)) => {
            bail!("{primary:#}; pause cleanup also failed: {cleanup:#}")
        }
        (Err(primary), _) => Err(primary),
        (_, Err(cleanup)) => Err(cleanup),
        (Ok(mut value), Ok(())) => {
            let state = snapshot(a, actor, false).await?;
            if nested {
                value["state"] = state;
            } else {
                value = state;
            }
            Ok(value)
        }
    }
}
async fn ensure_server_pause(a: &App) -> Result<()> {
    a.world
        .directory(|d| {
            ensure!(
                !d.worlds
                    .values()
                    .flat_map(|w| w.players.values())
                    .any(|p| p.controller == crate::world::Controller::Human),
                "native participants require the managed native wrapper for pauseAfter"
            );
            Ok(())
        })
        .await
}
async fn spawn(a: &App, t: &Testing, spec: Spawn) -> Result<Value> {
    ensure!(
        a.world.clock.paused(),
        "spawn requires a paused prepared test session"
    );
    ensure!((1..=20).contains(&spec.level), "level must be 1–20");
    ensure!(
        !spec.guid.is_empty() && !spec.guid.contains('@') && spec.guid.len() <= 128,
        "use an explicit disposable guest GUID, not a registered account"
    );
    let class = a.content.get(spec.class)?;
    ensure!(
        a.content.classes.contains_key(&spec.class),
        "unknown playable class"
    );
    let mut preview = Character {
        schema: 1,
        class: spec.class,
        base: a.content.classes[&spec.class].base,
        id: 1,
        account: 1,
        name: String::new(),
        level: 1,
        exp: 0,
        fame: 0,
        hp: 1,
        mp: 1,
        items: spec.equipment.into_iter().chain(spec.backpack).collect(),
        revision: 0,
        alive: true,
        created_at: 0,
        shots: 0,
        hits: 0,
        kills: 0,
        quests: 0,
        tiles: 0,
        pirate_caves: 0,
    };
    preview.gain_experience(
        &a.content,
        crate::store::level_start(spec.level)?,
        &mut StdRng::seed_from_u64(spec.seed),
    )?;
    if let Some(stats) = spec.stats {
        preview.base = stats;
    }
    preview.effective(&a.content)?;
    for kind in preview.items.iter().filter(|k| **k != -1) {
        a.content.get(u16::try_from(*kind)?)?;
    }
    let (world, pos) = (spec.world, spec.position);
    a.world
        .directory(move |d| {
            let w = d
                .worlds
                .get(&world)
                .context("world missing; explicitly create the required world first")?;
            ensure!(
                pos.x.is_finite() && pos.y.is_finite() && w.walkable(pos),
                "spawn position is blocked or outside the world"
            );
            Ok(())
        })
        .await?;
    if spec.controller == Controller::GameBot {
        crate::bots::validate_test_class(spec.class)?;
    }
    let account = a.store.list(spec.guid.clone()).await?;
    ensure!(
        account.characters.is_empty(),
        "GUID already has a living character; no implicit replacement or reset"
    );
    let mut ch = a.store.create(account.id, class).await?;
    ch.base = preview.base;
    ch.level = preview.level;
    ch.exp = preview.exp;
    ch.fame = preview.fame;
    ch.items = preview.items;
    let stats = ch.effective(&a.content)?;
    ch.hp = stats[0];
    ch.mp = stats[1];
    ch.revision = a
        .store
        .exchange(
            vec![InventoryWrite {
                id: ch.id,
                account: ch.account,
                revision: ch.revision,
                items: ch.items.clone(),
            }],
            None,
        )
        .await?[0];
    a.store.progress(ch.clone()).await?;
    let actor = ch.id;
    write_json(&t.path(&format!("actor-{actor}"), "spawn"), &spec)?;
    match spec.controller {
        Controller::Native => write_json(
            &t.path(&format!("account-{}", ch.account), "native"),
            &(world, pos),
        )?,
        Controller::Direct => {
            let owner = a.claim(ch.account)?;
            let id = a
                .world
                .in_world(world)
                .call(move |w| w.enter_at(ch, None, Some(pos)))
                .await?;
            a.bots
                .lock()
                .expect("ownership registry poisoned")
                .insert(id, owner);
        }
        Controller::GameBot => crate::bots::start_test(a, ch, world, pos).await?,
    }
    Ok(
        json!({"actor":actor,"account":account.id,"world":world,"controller":spec.controller,"provenance":"prepared","connected":spec.controller!=Controller::Native}),
    )
}
async fn execute(a: &App, t: &Testing, id: &str, command: Command) -> Result<Value> {
    match command {
        Command::Capabilities => Ok(json!({"schema":1,"provenance":"prepared","pause_scope":"isolated server and all registered native clients","controllers":["direct","native","game_bot"],"goals":["time","level","kills","pirate_caves"],"max_game_ms":600000,"trace_page_limit":40})),
        Command::Catalog{search,limit} => {ensure!(!search.is_empty() && search.len()<=80 && (1..=20).contains(&limit),"catalog needs search and limit 1–20");let matches=a.content.objects.values().filter(|o|o.name.to_lowercase().contains(&search.to_lowercase())).collect::<Vec<_>>();Ok(json!({"items":matches.iter().take(limit).map(|o|json!({"id":o.kind,"name":o.name,"class":o.class,"slot_type":o.slot_type,"unsupported_equipment":o.unsupported_equipment})).collect::<Vec<_>>(),"omitted":matches.len().saturating_sub(limit)}))},
        Command::WorldInfo{world} => a.world.in_world(world).call(|w|Ok(json!({"name":w.name,"width":w.map.width,"height":w.map.height,"spawn_points":w.map.spawns}))).await,
        Command::Spawn{spec} => spawn(a,t,spec).await,
        Command::World{template} => { let world=a.world.directory(move |d|d.instance(&template,None)).await?; Ok(json!({"world":world,"provenance":"prepared"})) },
        Command::Observe{actor} => {t.registration(actor)?;snapshot(a,actor,false).await},
        Command::Inspect{actor} => {t.registration(actor)?;snapshot(a,actor,true).await},
        Command::Resume => {a.world.clock.set_paused(false);Ok(json!({"paused":false}))},
        Command::NativeSync{actors} => native_sync(a,t,actors).await,
        Command::Pause{native_paused} => {pause(a,&native_paused).await?;Ok(json!({"paused":true,"native_paused":native_paused}))},
        Command::Act{actor,action,duration_ms,pause_after} => {
            let spec=t.registration(actor)?;
            ensure!(spec.controller==Controller::Direct,"direct actions require a direct actor; use native input or the existing game-bot controller, not simultaneous controllers");
            ensure!(duration_ms<=10000,"single action duration exceeds 10 seconds; use a bounded run");
            if pause_after {ensure_server_pause(a).await?;}
            let before=snapshot(a,actor,false).await?;t.record(id,json!({"event":"before","state":before}))?;
            a.world.clock.set_paused(false);
            let result: Result<Value> = async {
                let player=resolve(a,actor).await?;
                if let Some(plan)=a.world.for_player(player).call(move |w|w.action(player,action)).await? {persist(a,plan).await?;}
                tokio::time::sleep(Duration::from_millis(duration_ms)).await;
                a.world.for_player(player).call(move |w|w.action(player,Action::Stop)).await?;
                snapshot(a,actor,false).await
            }.await;
            finish_action(a,actor,result,pause_after,false).await
        },
        Command::Run{actor,goal,max_game_ms,pause_after} => {
            let spec=t.registration(actor)?;
            ensure!(spec.controller==Controller::GameBot,"run goals require the actual gameplay-bot controller");
            ensure!((50..=600000).contains(&max_game_ms),"max_game_ms must be 50–600000");
            match goal {Goal::Time{ms}=>ensure!(ms>0&&ms<=max_game_ms,"duration must fit deadline"),Goal::Level{level}=>ensure!((1..=20).contains(&level),"invalid target level"),Goal::Kills{count}|Goal::PirateCaves{count}=>ensure!(count>0,"count must be positive")}
            if pause_after {ensure_server_pause(a).await?;}
            let before=snapshot(a,actor,false).await?;t.record(id,json!({"event":"before","state":before}))?;
            let start_tick=a.world.directory(|d|Ok(d.tick)).await?;
            a.world.clock.set_paused(false);
            let wall=Instant::now();
            let result: Result<Value> = async {loop {
                ensure!(!t.cancelled.lock().expect("cancellation registry poisoned").contains(id),"run cancelled");
                ensure!(wall.elapsed()<Duration::from_millis(max_game_ms+30000),"wall deadline exceeded");
                let current=snapshot(a,actor,false).await?;
                ensure!(current["lifecycle"]["alive"]==true && current["lifecycle"]["death_pending"]==false,"selected actor died; no replacement was selected");
                ensure!(current["lifecycle"]["quarantined"]==false,"actor quarantined");
                let elapsed=(a.world.directory(|d|Ok(d.tick)).await?-start_tick)*50;
                let reached=match goal {
                    Goal::Time{ms}=>elapsed>=ms,
                    Goal::Level{level}=>current["character"]["level"].as_i64().context("level missing")?>=i64::from(level),
                    Goal::Kills{count}=>current["verification"]["kills"].as_u64().context("kills missing")?>=before["verification"]["kills"].as_u64().context("initial kills missing")?+count,
                    Goal::PirateCaves{count}=>current["verification"]["pirate_caves"].as_u64().context("cave count missing")?>=before["verification"]["pirate_caves"].as_u64().context("initial cave count missing")?+count,
                };
                if reached {return Ok(json!({"goal_reached":true,"game_ms":elapsed,"state":current}));}
                ensure!(elapsed<max_game_ms,"goal not reached before game deadline");
                tokio::time::sleep(Duration::from_millis(50)).await;
            }}.await;
            finish_action(a,actor,result,pause_after,true).await
        },
        Command::Cheat{actor,change} => {
            t.registration(actor)?;
            ensure!(a.world.clock.paused(),"cheats require an explicitly paused test session");
            let player=resolve(a,actor).await?;
            let mut ch=a.world.for_player(player).call(move |w| {let p=w.players.get(&player).context("player missing")?;ensure!(p.character.alive&&!p.pending&&!p.progress_started&&!p.death_pending&&!p.departing,"actor has pending gameplay or persistence");Ok(p.character.clone())}).await?;
            match change {
                Cheat::Teleport{position}=>{a.world.for_player(player).call(move |w| {ensure!(position.x.is_finite()&&position.y.is_finite()&&w.walkable(position),"teleport position invalid");let p=w.players.get_mut(&player).context("player missing")?;ensure!(p.controller!=crate::world::Controller::Human,"native teleport requires a verified client correction, not a server-only edit");p.pos=position;p.bot_goal=None;w.updates(player,true)}).await?;},
                change=>{
                    match change {
                        Cheat::Stats{values}=>{ch.base=values;let stats=ch.effective(&a.content)?;ch.hp=ch.hp.min(stats[0]);ch.mp=ch.mp.min(stats[1]);},
                        Cheat::Item{slot,item}=>{ensure!(slot<12,"invalid item slot");if item!=-1{a.content.get(u16::try_from(item)?)?;}ch.items[slot]=item;ch.effective(&a.content)?;ch.revision=a.store.exchange(vec![InventoryWrite{id:ch.id,account:ch.account,revision:ch.revision,items:ch.items.clone()}],None).await?[0];},
                        Cheat::Teleport{..}=>unreachable!(),
                    }
                    let stats = ch.effective(&a.content)?;
                    ch.hp = ch.hp.min(stats[0]); ch.mp = ch.mp.min(stats[1]);
                    a.store.progress(ch.clone()).await?;
                    a.world.for_player(player).call(move |w|{w.players.get_mut(&player).context("player missing")?.character=ch;w.updates(player,true)}).await?;
                }
            }
            snapshot(a,actor,false).await
        },
        Command::Result{request}=>{valid_id(&request)?;let saved:Value=serde_json::from_slice(&fs::read(t.path(&request,"receipt"))?)?;if saved["status"]=="running" {Ok(json!({"status":"incomplete","error":"request has no confirmed result; inspect trace, do not replay effects"}))}else{Ok(saved.get("receipt").context("stored receipt missing")?.clone())}}, 
        Command::Trace{request,offset,limit}=>{valid_id(&request)?;ensure!((1..=40).contains(&limit),"trace limit must be 1–40");let raw=fs::read_to_string(t.root.join(format!("{request}.jsonl")))?;let lines=raw.lines().collect::<Vec<_>>();Ok(json!({"events":lines.iter().skip(offset).take(limit).map(|s|serde_json::from_str::<Value>(s)).collect::<std::result::Result<Vec<_>,_>>()?,"total":lines.len(),"next_offset":(offset+limit).min(lines.len())}))},
        Command::Cancel{request}=>{valid_id(&request)?;t.cancelled.lock().expect("cancellation registry poisoned").insert(request);Ok(json!({"cancel_requested":true}))},
    }
}
pub async fn command(State(a): State<App>, headers: HeaderMap, Json(raw): Json<Value>) -> Response {
    let result:Result<Value>=async {
        control(&a,&headers)?;
        let t=a.testing.as_ref().context("testing interface requires an exclusively created --test-state directory")?;
        let request:Request=serde_json::from_value(raw.clone())?;valid_id(&request.id)?;
        if matches!(request.command,Command::Observe{..}|Command::Inspect{..}|Command::Catalog{..}|Command::WorldInfo{..}|Command::Result{..}|Command::Trace{..}|Command::Capabilities|Command::Cancel{..}) {return execute(&a,t,&request.id,request.command).await;}
        let _guard=t.gate.lock().await;
        let path=t.path(&request.id,"receipt");
        if path.exists() {
            let previous:Value=serde_json::from_slice(&fs::read(path)?)?;
            ensure!(previous["request"]==raw,"request ID reused with different input");
            ensure!(previous["status"] != "running", "previous request has no confirmed result; inspect its trace rather than repeating its effects");
            return Ok(previous.get("receipt").context("stored receipt is malformed")?.clone());
        }
        write_json(&path,&json!({"request":raw,"status":"running"}))?;
        t.record(&request.id,json!({"event":"accepted","request":raw}))?;
        let wall=Instant::now();let start=a.world.directory(|d|Ok(d.tick)).await?;
        let result=execute(&a,t,&request.id,request.command).await;
        let mut receipt=json!({"id":request.id,"status":if result.is_ok(){"succeeded"}else{"failed"},"wall_ms":wall.elapsed().as_millis(),"game_ms":(a.world.directory(|d|Ok(d.tick)).await?-start)*50,"paused":a.world.clock.paused(),"pause_scope":"server","provenance":"prepared","trace":t.root.join(format!("{}.jsonl",request.id))});
        match result {Ok(v)=>receipt["result"]=v,Err(e)=>receipt["error"]=format!("{e:#}").into()}
        t.record(&request.id,json!({"event":"result","receipt":receipt}))?;
        let durable=json!({"request":raw,"receipt":receipt});
        write_json(&path,&durable)?;
        Ok(receipt)
    }.await;
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"status":"failed","error":format!("{e:#}")})),
        )
            .into_response(),
    }
}
