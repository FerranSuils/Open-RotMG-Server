use crate::{
    content::{Content, WIZARD},
    crypto::Keys,
    store::{Store, xml_escape},
    wire::{self, Codec, Packet, Reader, Writer},
    world::{Action, Handle, InventoryPlan, Pos, Slot},
};
use anyhow::{Context, Result, bail, ensure};
use axum::{
    Json, Router,
    extract::{
        ConnectInfo, DefaultBodyLimit, Form, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::{Arc, Mutex},
};
pub struct Owner {
    registry: Arc<Mutex<BTreeSet<i32>>>,
    id: i32,
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.registry
            .lock()
            .expect("ownership registry poisoned")
            .remove(&self.id);
    }
}
impl App {
    fn claim(&self, id: i32) -> Result<Owner> {
        let mut owners = self.owners.lock().expect("ownership registry poisoned");
        ensure!(owners.len() < self.max_players, "development world full");
        ensure!(owners.insert(id), "account already has an active session");
        Ok(Owner {
            registry: self.owners.clone(),
            id,
        })
    }
}
use tokio::sync::mpsc;
#[derive(Clone)]
pub struct App {
    pub started: std::time::Instant,
    pub store: Store,
    pub world: Handle,
    pub keys: Arc<Keys>,
    pub content: Arc<Content>,
    pub swf: Arc<Vec<u8>>,
    pub origin: String,
    pub hosted_client: bool,
    pub sfx: PathBuf,
    pub max_players: usize,
    pub behind_proxy: bool,
    pub requests: Arc<Mutex<HashMap<IpAddr, (std::time::Instant, u32)>>>,
    pub connections: Arc<tokio::sync::Semaphore>,
    pub control: Option<String>,
    pub owners: Arc<Mutex<BTreeSet<i32>>>,
    pub bots: Arc<Mutex<BTreeMap<i32, Owner>>>,
    pub quarantined: Arc<Mutex<Vec<Owner>>>,
    pub shutdown: tokio::sync::watch::Sender<bool>,
}
fn error(e: anyhow::Error) -> Response {
    tracing::error!(error=%e,"request failed");
    (
        StatusCode::BAD_REQUEST,
        [("content-type", "application/xml")],
        format!("<Error>{}</Error>", xml_escape(&e.to_string())),
    )
        .into_response()
}
const HOSTED_ORIGIN: &str = "https://openrotmg.org";
fn allowed_origin(origin: &str, configured: &str, hosted: bool) -> bool {
    origin == configured || (hosted && origin == HOSTED_ORIGIN)
}
fn admit(a: &App, headers: &HeaderMap, peer: SocketAddr) -> Result<bool> {
    if let Some(origin) = headers.get("origin") {
        ensure!(
            allowed_origin(origin.to_str()?, &a.origin, a.hosted_client),
            "unapproved request origin: {}; expected {}{}",
            origin.to_str()?,
            a.origin,
            if a.hosted_client {
                " or https://openrotmg.org"
            } else {
                ""
            }
        );
    }
    let ip = if a.behind_proxy {
        headers
            .get("x-forwarded-for")
            .context("proxy must supply X-Forwarded-For")?
            .to_str()?
            .parse::<IpAddr>()
            .context("proxy must supply one client IP")?
    } else {
        peer.ip()
    };
    Ok(rate_allow(
        &mut a.requests.lock().expect("request registry poisoned"),
        ip,
        std::time::Instant::now(),
    ))
}
fn rate_allow(
    requests: &mut HashMap<IpAddr, (std::time::Instant, u32)>,
    ip: IpAddr,
    now: std::time::Instant,
) -> bool {
    requests.retain(|_, (start, _)| now.duration_since(*start).as_secs() < 60);
    if requests.len() >= 4096 && !requests.contains_key(&ip) {
        return false;
    }
    let (_, count) = requests.entry(ip).or_insert((now, 0));
    *count = count.saturating_add(1);
    if *count == 21 {
        tracing::warn!(%ip, "guest entry rate exceeded");
    }
    *count <= 20
}
fn limited() -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        [("content-type", "application/xml"), ("retry-after", "60")],
        "<Error>Too many guest entry requests; retry in one minute.</Error>",
    )
        .into_response()
}
async fn chars(
    State(a): State<App>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    match admit(&a, &headers, peer) {
        Ok(true) => {}
        Ok(false) => return limited(),
        Err(e) => return error(e),
    }
    let result: Result<String> = async {
        let guid = form.get("guid").context("guid is required")?.clone();
        ensure!(
            form.get("password").is_none_or(|s| s.is_empty()),
            "registered account login not implemented in this development build"
        );
        let account = a.store.list(guid).await?;
        Ok(account.xml(a.content.get(WIZARD)?))
    }
    .await;
    match result {
        Ok(xml) => ([("content-type", "application/xml")], xml).into_response(),
        Err(e) => error(e),
    }
}
async fn fame(State(a): State<App>, Form(form): Form<HashMap<String, String>>) -> Response {
    let result: Result<String> = async {
        let account = form
            .get("accountId")
            .context("accountId required")?
            .parse()?;
        let character = form.get("charId").context("charId required")?.parse()?;
        a.store.fame(account, character).await
    }
    .await;
    match result {
        Ok(xml) => ([("content-type", "application/xml")], xml).into_response(),
        Err(e) => error(e),
    }
}
async fn swf(State(a): State<App>) -> Response {
    (
        [("content-type", "application/x-shockwave-flash")],
        a.swf.as_ref().clone(),
    )
        .into_response()
}
async fn health(State(a): State<App>) -> Response {
    let started = a.started;
    match a.world.call(move |w| Ok(w.health(started))).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::SERVICE_UNAVAILABLE, error(e)).into_response(),
    }
}
async fn websocket(
    State(a): State<App>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let origin = headers.get("origin").and_then(|v| v.to_str().ok());
    if !origin.is_some_and(|v| allowed_origin(v, &a.origin, a.hosted_client)) {
        tracing::warn!(?origin, expected=%a.origin, hosted_client=a.hosted_client, "unapproved WebSocket origin");
        return (
            StatusCode::FORBIDDEN,
            "unapproved WebSocket origin; inspect the server log for received and expected origins",
        )
            .into_response();
    }
    match admit(&a, &headers, peer) {
        Ok(true) => {}
        Ok(false) => return limited(),
        Err(e) => return error(e),
    }
    let Ok(permit) = a.connections.clone().try_acquire_owned() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "game connection limit reached",
        )
            .into_response();
    };
    ws.max_message_size(1_048_576)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            connection(socket, a).await
        })
}
fn fail(description: &str) -> Packet {
    let mut o = Writer::new();
    o.i32(0).utf(description);
    Packet { id: 0, body: o.0 }
}
fn slot(r: &mut Reader<'_>) -> Result<Slot> {
    Ok(Slot {
        object: r.i32()?,
        slot: r.u8()? as usize,
        kind: r.i16()? as i32,
    })
}
async fn persist(a: &App, plan: InventoryPlan) -> Result<()> {
    let result = a
        .store
        .inventory(
            plan.character,
            plan.account,
            plan.revision,
            plan.items.clone(),
        )
        .await;
    a.world
        .call(move |w| w.finish_inventory(plan, result))
        .await
}
async fn handle(
    a: &App,
    packet: Packet,
    account: &mut Option<Owner>,
    player: &mut Option<i32>,
    tx: &mpsc::Sender<Packet>,
    lost: &tokio::sync::watch::Sender<bool>,
) -> Result<()> {
    if let Some(id) = *player
        && matches!(packet.id, 7 | 38 | 30 | 65 | 33 | 42 | 3)
        && a.world
            .call(move |w| {
                let p = w.players.get(&id).context("player missing")?;
                Ok(p.death_pending || !p.character.alive)
            })
            .await?
    {
        tracing::debug!(
            player = id,
            packet = packet.id,
            "ignored queued command from dying character"
        );
        return Ok(());
    }
    let mut r = Reader::new(&packet.body);
    match packet.id {
        wire::HELLO => {
            ensure!(account.is_none(), "duplicate HELLO");
            let version = r.utf()?;
            ensure!(version == "123.5.1", "unsupported client build {version}");
            let game = r.i32()?;
            ensure!(
                (-4..=0).contains(&game),
                "unsupported world entry {game}; only the explicit development fixture routes are available"
            );
            let guid = a.keys.decrypt(&r.utf()?)?;
            let password = a.keys.decrypt(&r.utf()?)?;
            let secret = a.keys.decrypt(&r.utf()?)?;
            ensure!(
                password.is_empty() && secret.is_empty(),
                "only local guest accounts are enabled"
            );
            let _keytime = r.i32()?;
            let n = usize::try_from(r.i16()?)?;
            r.take(n)?;
            ensure!(r.i32()? == 0, "map editor JSON not supported");
            for _ in 0..4 {
                r.utf()?;
            }
            r.done()?;
            let id = a.store.authenticate(guid).await?;
            *account = Some(a.claim(id)?);
            let body = a.world.call(|w| Ok(w.map_info())).await?;
            tx.send(Packet {
                id: wire::MAP_INFO,
                body,
            })
            .await?;
            tracing::info!(
                account = id,
                requested_world = game,
                actual_world = "development tutorial fixture",
                "guest authenticated"
            );
        }
        wire::CREATE | wire::LOAD => {
            ensure!(player.is_none(), "character already loaded");
            let account = account
                .as_ref()
                .context("HELLO required before character selection")?
                .id;
            let ch = if packet.id == wire::CREATE {
                ensure!(r.i16()? as u16 == WIZARD, "only Wizard is implemented");
                r.done()?;
                a.store.create(account, a.content.get(WIZARD)?).await?
            } else {
                let id = r.i32()?;
                r.done()?;
                a.store.load(account, id).await?
            };
            let output = crate::world::Output {
                packets: tx.clone(),
                lost: lost.clone(),
            };
            *player = Some(a.world.call(move |w| w.enter(ch, Some(output))).await?);
        }
        wire::UPDATE_ACK => {
            r.done()?;
        }
        wire::MOVE => {
            let _tick = r.i32()?;
            let time = r.i32()?;
            let pos = Pos {
                x: r.f32()?,
                y: r.f32()?,
            };
            let count = r.i16()?;
            ensure!((0..=128).contains(&count), "movement history too long");
            for _ in 0..count {
                r.i32()?;
                r.f32()?;
                r.f32()?;
            }
            r.done()?;
            let id = player.context("MOVE before character")?;
            a.world
                .call(move |w| match w.move_to(id, pos, time) {
                    Ok(()) => Ok(()),
                    Err(e) => w.correct_position(id, time, &e.to_string()),
                })
                .await?;
        }
        wire::PLAYER_SHOOT => {
            let time = r.i32()?;
            let bullet = r.u8()?;
            let kind = r.i16()? as u16;
            let pos = Pos {
                x: r.f32()?,
                y: r.f32()?,
            };
            let angle = r.f32()?;
            r.done()?;
            let id = player.context("shoot before character")?;
            a.world
                .call(move |w| w.shoot(id, time, bullet, kind, pos, angle))
                .await?;
        }
        wire::PONG => {
            r.i32()?;
            r.i32()?;
            r.done()?;
        }
        22 => {
            r.i32()?;
            r.done()?;
        }
        76 => {
            r.i32()?;
            r.u8()?;
            r.i32()?;
            r.u8()?;
            r.done()?;
        }
        24 => {
            r.u8()?;
            r.i32()?;
            r.done()?;
        }
        66 | 51 => {
            r.i32()?;
            r.u8()?;
            r.i32()?;
            r.done()?;
        }
        30 => {
            r.i32()?;
            let s = slot(&mut r)?;
            let pos = Pos {
                x: r.f32()?,
                y: r.f32()?,
            };
            r.done()?;
            let id = player.context("item use before character")?;
            if let Some(plan) = a.world.call(move |w| w.use_item(id, s, pos)).await? {
                persist(a, plan).await?;
            }
        }
        65 => {
            r.i32()?;
            r.f32()?;
            r.f32()?;
            let from = slot(&mut r)?;
            let to = slot(&mut r)?;
            r.done()?;
            let id = player.context("inventory swap before character")?;
            let plan = a.world.call(move |w| w.swap(id, from, to)).await?;
            persist(a, plan).await?;
        }
        14 => {
            r.i32()?;
            r.done()?;
        }
        48 => {
            r.done()?;
        }
        33 => {
            let name = r.utf()?;
            r.done()?;
            let account = account.as_ref().context("name before HELLO")?.id;
            a.store.rename(account, name.clone()).await?;
            if let Some(id) = player {
                let id = *id;
                a.world
                    .call(move |w| {
                        w.players
                            .get_mut(&id)
                            .context("player missing")?
                            .character
                            .name = name;
                        Ok(())
                    })
                    .await?;
            }
            let mut o = Writer::new();
            o.u8(1).utf("");
            tx.send(Packet { id: 20, body: o.0 }).await?;
        }
        42 | 3 => {
            bail!("Nexus and portal destinations are not implemented in this development build");
        }
        _ => bail!(
            "packet {} is not implemented in this development build",
            packet.id
        ),
    }
    Ok(())
}
async fn connection(socket: WebSocket, a: App) {
    let (mut wsout, mut wsin) = socket.split();
    let (tx, mut rx) = mpsc::channel::<Packet>(64);
    let (lost, mut lost_rx) = tokio::sync::watch::channel(false);
    let mut shutdown = a.shutdown.subscribe();
    let deadline = tokio::time::sleep(std::time::Duration::from_secs(10));
    tokio::pin!(deadline);
    let mut codec = Codec::server();
    let mut account = None;
    let mut player = None;
    let mut ping = tokio::time::interval(std::time::Duration::from_secs(5));
    let mut serial = 0;
    let mut packet_window = std::time::Instant::now();
    let mut packet_count = 0usize;
    loop {
        tokio::select! {incoming=wsin.next()=>match incoming{Some(Ok(Message::Binary(bytes)))=>{match codec.feed(&bytes){Ok(packets)=>{if packet_window.elapsed().as_secs()>=1{packet_window=std::time::Instant::now();packet_count=0;}packet_count=packet_count.saturating_add(packets.len());if packet_count>512{tracing::warn!(player,"input packet rate exceeded");break;}let mut failed=false;for packet in packets{let id=packet.id;if let Err(e)=handle(&a,packet,&mut account,&mut player,&tx,&lost).await{tracing::error!(account=account.as_ref().map(|o|o.id),player,packet=id,error=%e,"game command rejected");if matches!(id,30|65){let mut o=Writer::new();o.i32(1);if wsout.send(Message::Binary(codec.frame(4,o.0).into())).await.is_err(){failed=true;break;}continue;}let f=fail(&e.to_string());let _=wsout.send(Message::Binary(codec.frame(f.id,f.body).into())).await;failed=true;break;}}if failed{break;}},Err(e)=>{tracing::error!(error=%e,"invalid packet stream");break;}}},Some(Ok(Message::Ping(bytes)))=>{if wsout.send(Message::Pong(bytes)).await.is_err(){break}},Some(Ok(Message::Close(_)))|None=>break,Some(Ok(_))=>{tracing::warn!("non-binary game message rejected");break},Some(Err(e))=>{tracing::warn!(error=%e,"WebSocket failed");break}},
        _=shutdown.changed()=>break,
        _=lost_rx.changed()=>{tracing::warn!(player,"closing slow game connection before its state diverges");break;},
        _=&mut deadline,if player.is_none()=>{tracing::warn!("game selection deadline exceeded");break;},
        outgoing=rx.recv()=>{let Some(packet)=outgoing else{break};if wsout.send(Message::Binary(codec.frame(packet.id,packet.body).into())).await.is_err(){break}},
        _=ping.tick()=>{if account.is_some(){serial+=1;let mut o=Writer::new();o.i32(serial);if wsout.send(Message::Binary(codec.frame(wire::PING,o.0).into())).await.is_err(){break;}}}
        }
    }
    if let Some(id) = player
        && let Err(e) = depart(&a, id).await
    {
        tracing::error!(player=id,error=%e,"disconnect save failed; avatar retained for diagnosis");
        if let Some(owner) = account.take() {
            a.quarantined
                .lock()
                .expect("quarantine registry poisoned")
                .push(owner);
        }
    }
}
async fn depart(a: &App, id: i32) -> Result<()> {
    loop {
        if let Some(ch) = a.world.call(move |w| w.begin_departure(id)).await? {
            if ch.alive {
                a.store.progress(ch).await?;
            }
            return a.world.call(move |w| w.finish_departure(id)).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}
#[derive(Deserialize)]
struct BotCreate {
    guid: String,
}
async fn bot_create(
    State(a): State<App>,
    headers: HeaderMap,
    Json(input): Json<BotCreate>,
) -> Response {
    let result: Result<serde_json::Value> = async {
        control(&a, &headers)?;
        let acc = a.store.list(input.guid).await?;
        let owner = a.claim(acc.id)?;
        let ch = if acc.characters.is_empty() {
            a.store.create(acc.id, a.content.get(WIZARD)?).await?
        } else {
            ensure!(acc.characters.len() == 1, "unexpected character count");
            acc.characters[0].clone()
        };
        let id = a.world.call(move |w| w.enter(ch, None)).await?;
        a.bots
            .lock()
            .expect("bot ownership registry poisoned")
            .insert(id, owner);
        Ok(serde_json::json!({"player":id}))
    }
    .await;
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => error(e),
    }
}
fn control(a: &App, headers: &HeaderMap) -> Result<()> {
    ensure!(!*a.shutdown.borrow(), "server is shutting down");
    let token = a.control.as_ref().context("script control is disabled")?;
    ensure!(
        headers.get("authorization").and_then(|v| v.to_str().ok())
            == Some(format!("Bearer {token}").as_str()),
        "invalid control token"
    );
    Ok(())
}
#[derive(Deserialize)]
struct BotCommand {
    player: i32,
    action: Action,
}
async fn bot_action(
    State(a): State<App>,
    headers: HeaderMap,
    Json(input): Json<BotCommand>,
) -> Response {
    let result: Result<serde_json::Value> = async {
        control(&a, &headers)?;
        let id = input.player;
        if let Some(plan) = a.world.call(move |w| w.action(id, input.action)).await? {
            persist(&a, plan).await?;
        }
        a.world.call(move |w| w.observe(id)).await
    }
    .await;
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => error(e),
    }
}
#[derive(Deserialize)]
struct ScriptStep {
    at_ms: u64,
    action: Option<Action>,
}
#[derive(Deserialize)]
struct Script {
    player: i32,
    steps: Vec<ScriptStep>,
}
async fn script(State(a): State<App>, headers: HeaderMap, Json(input): Json<Script>) -> Response {
    let result: Result<Vec<serde_json::Value>> = async {
        control(&a, &headers)?;
        ensure!(
            !input.steps.is_empty() && input.steps.len() <= 512,
            "script needs 1–512 steps"
        );
        let mut previous = 0;
        for s in &input.steps {
            ensure!(
                s.at_ms >= previous && s.at_ms <= 120000,
                "script times must be ordered and at most 120 seconds"
            );
            previous = s.at_ms;
        }
        let start = tokio::time::Instant::now();
        let mut shutdown=a.shutdown.subscribe();
        let mut frames = Vec::new();
        for step in input.steps {
            tokio::select!{_=tokio::time::sleep_until(start+std::time::Duration::from_millis(step.at_ms))=>{},_=shutdown.changed()=>bail!("server is shutting down")};
            let id = input.player;
            if let Some(action) = step.action
                && let Some(plan) = a.world.call(move |w| w.action(id, action)).await?
            {
                persist(&a, plan).await?;
            }
            frames.push(a.world.call(move |w| w.observe(id)).await?);
        }
        Ok(frames)
    }
    .await;
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => error(e),
    }
}
async fn map(State(a): State<App>, headers: HeaderMap) -> Response {
    let result:Result<serde_json::Value>=async{control(&a,&headers)?;a.world.call(|w|{let blocked:Vec<_>=(0..w.map.height).flat_map(|y|(0..w.map.width).map(move|x|(x,y))).filter(|(x,y)|!w.walkable(Pos{x:*x as f32+0.5,y:*y as f32+0.5})).collect();Ok(serde_json::json!({"width":w.map.width,"height":w.map.height,"blocked":blocked,"entities":w.entities.values().collect::<Vec<_>>()}))}).await}.await;
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => error(e),
    }
}
#[derive(Deserialize)]
struct BotPlayer {
    player: i32,
}
async fn observe(
    State(a): State<App>,
    headers: HeaderMap,
    Json(input): Json<BotPlayer>,
) -> Response {
    let result: Result<serde_json::Value> = async {
        control(&a, &headers)?;
        a.world.call(move |w| w.observe(input.player)).await
    }
    .await;
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => error(e),
    }
}
async fn bot_leave(
    State(a): State<App>,
    headers: HeaderMap,
    Json(input): Json<BotPlayer>,
) -> Response {
    let result: Result<()> = async {
        control(&a, &headers)?;
        ensure!(
            a.bots
                .lock()
                .expect("bot ownership registry poisoned")
                .contains_key(&input.player),
            "not a script-owned player"
        );
        depart(&a, input.player).await?;
        a.bots
            .lock()
            .expect("bot ownership registry poisoned")
            .remove(&input.player);
        Ok(())
    }
    .await;
    match result {
        Ok(()) => Json(serde_json::json!({"saved":true})).into_response(),
        Err(e) => error(e),
    }
}
async fn local_client(State(a): State<App>) -> Response {
    if !a.hosted_client {
        return (
            StatusCode::NOT_FOUND,
            "Hosted development client is disabled. Start the server with ./run.sh.",
        )
            .into_response();
    }
    Json(serde_json::json!({"protocol": 1, "mode": "local-development"})).into_response()
}
pub async fn serve(a: App, bind: SocketAddr, client: PathBuf) -> Result<()> {
    let deaths = a.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let pending = match deaths.world.call(|w| Ok(w.pending_deaths())).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(error=%e,"death coordinator stopped");
                    break;
                }
            };
            for (id, ch, killer) in pending {
                match deaths.store.death(ch, killer).await {
                    Ok(()) => {
                        if let Err(e) = deaths.world.call(move |w| w.finish_death(id)).await {
                            tracing::error!(player=id,error=%e,"committed death notification failed");
                        }
                    }
                    Err(e) => {
                        tracing::error!(player=id,error=%e,"death persistence failed; character quarantined")
                    }
                }
            }
        }
    });
    let listener = tokio::net::TcpListener::bind(bind).await.with_context(|| {
        format!("cannot listen on {bind}; another server may already use this port. Stop it and rerun ./run.sh")
    })?;
    let mut origins = vec![a.origin.parse::<axum::http::HeaderValue>()?];
    if a.hosted_client {
        origins.push(HOSTED_ORIGIN.parse()?);
    }
    let cors = tower_http::cors::CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
        .allow_headers([axum::http::header::CONTENT_TYPE]);
    let app = Router::new()
        .route("/legacy-api/char/list", post(chars))
        .route("/legacy-api/char/fame", post(fame))
        .nest_service("/sfx", tower_http::services::ServeDir::new(a.sfx.clone()))
        .route("/client.swf", get(swf))
        .route("/game", get(websocket))
        .route("/health", get(health))
        .route("/local-client", get(local_client))
        .layer(cors);
    let app = app
        .route("/dev/join", post(bot_create))
        .route("/dev/action", post(bot_action))
        .route("/dev/script", post(script))
        .route("/dev/map", get(map))
        .route("/dev/observe", post(observe))
        .route("/dev/leave", post(bot_leave))
        .route_service(
            "/",
            tower_http::services::ServeFile::new(client.join("index.html")),
        )
        .route_service(
            "/index.html",
            tower_http::services::ServeFile::new(client.join("index.html")),
        )
        .nest_service(
            "/vendor",
            tower_http::services::ServeDir::new(client.join("vendor")),
        )
        .layer(DefaultBodyLimit::max(1_048_576))
        .with_state(a.clone());
    tracing::info!(address=%bind,origin=%a.origin,max_players=a.max_players,behind_proxy=a.behind_proxy,hosted_client=a.hosted_client,"game frontdoor listening");
    if a.hosted_client {
        eprintln!(
            "\nLocal server ready. Open: {HOSTED_ORIGIN}/local/#port={}\nAllow local-network access if your browser asks.\nLogs: the terminal and .local/server.log\nIf the hosted client does not work, stop this server, clone https://github.com/OpenRotMG/Open-RotMG-Client beside this repo as client, then run ./run.sh --local-client ../client.\n",
            bind.port()
        );
    } else if !a.behind_proxy {
        eprintln!(
            "\nLocal server ready. Open: http://{bind}/\nLogs: the terminal and .local/server.log\n"
        );
    }
    let shutdown = a.clone();
    let failure = Arc::new(Mutex::new(None));
    let report = failure.clone();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        let result: Result<()> = async {
            shutdown_signal().await?;
            tracing::info!("saving active characters before shutdown");
            let _ = shutdown.shutdown.send(true);
            let bots: Vec<_> = shutdown
                .bots
                .lock()
                .expect("bot registry poisoned")
                .keys()
                .copied()
                .collect();
            for id in bots {
                depart(&shutdown, id).await?;
                shutdown
                    .bots
                    .lock()
                    .expect("bot registry poisoned")
                    .remove(&id);
            }
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
            while !shutdown
                .owners
                .lock()
                .expect("ownership registry poisoned")
                .is_empty()
            {
                ensure!(
                    tokio::time::Instant::now() < deadline,
                    "shutdown save timed out; inspect quarantined characters and logs"
                );
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            Ok(())
        }
        .await;
        if let Err(e) = result {
            tracing::error!(error=%e,"shutdown persistence failed");
            *report.lock().expect("shutdown report poisoned") = Some(e.to_string());
        }
    })
    .await?;
    if let Some(e) = failure.lock().expect("shutdown report poisoned").take() {
        bail!("{e}");
    }
    Ok(())
}
async fn shutdown_signal() -> Result<()> {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {v=tokio::signal::ctrl_c()=>v?,_=term.recv()=>{}};
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hosted_origin_is_explicit_and_exact() {
        let local = "http://127.0.0.1:18923";
        assert!(allowed_origin(local, local, false));
        assert!(!allowed_origin(HOSTED_ORIGIN, local, false));
        assert!(allowed_origin(HOSTED_ORIGIN, local, true));
        for origin in [
            "https://evil.example",
            "https://openrotmg.org.evil.example",
            "http://openrotmg.org",
            "null",
        ] {
            assert!(!allowed_origin(origin, local, true));
        }
    }
    #[test]
    fn guest_rate_is_bounded_and_expires() {
        let mut requests = HashMap::new();
        let now = std::time::Instant::now();
        let ip = "192.0.2.1".parse().unwrap();
        for _ in 0..20 {
            assert!(rate_allow(&mut requests, ip, now));
        }
        assert!(!rate_allow(&mut requests, ip, now));
        assert!(rate_allow(&mut requests, "192.0.2.2".parse().unwrap(), now));
        assert!(rate_allow(
            &mut requests,
            ip,
            now + std::time::Duration::from_secs(60)
        ));
        assert_eq!(requests.len(), 1);
        for n in 1..=4096 {
            requests.insert(
                IpAddr::V6(std::net::Ipv6Addr::from(n as u128)),
                (now + std::time::Duration::from_secs(60), 1),
            );
        }
        assert!(!rate_allow(
            &mut requests,
            "192.0.2.3".parse().unwrap(),
            now + std::time::Duration::from_secs(60)
        ));
    }
}
