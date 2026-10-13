use crate::{
    content::{Content, WIZARD},
    crypto::Keys,
    store::{InventoryWrite, Store, xml_escape},
    wire::{self, Codec, Packet, Reader, Writer},
    world::{Action, Handle, InventoryPlan, Pos, Slot, TradeAction, TradePlan},
};
use anyhow::{Context, Result, bail, ensure};
use axum::{
    Json, Router,
    extract::{
        ConnectInfo, DefaultBodyLimit, Form, Query, State,
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
fn admit(a: &App, headers: &HeaderMap, peer: SocketAddr) -> Result<bool> {
    if let Some(origin) = headers.get("origin") {
        ensure!(
            origin.to_str()? == a.origin,
            "unapproved request origin: {}; expected {}",
            origin.to_str()?,
            a.origin
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
            form.get("secret").is_none_or(|s| s.is_empty()),
            "external platform authentication is not implemented"
        );
        let password = form.get("password").context("password required")?.clone();
        let account = a.store.characters(guid, password).await?;
        Ok(account.xml())
    }
    .await;
    match result {
        Ok(xml) => ([("content-type", "application/xml")], xml).into_response(),
        Err(e) => error(e),
    }
}
async fn account_request(
    State(a): State<App>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    axum::extract::Path(operation): axum::extract::Path<String>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    match admit(&a, &headers, peer) {
        Ok(true) => {}
        Ok(false) => return limited(),
        Err(e) => return error(e),
    }
    let result: Result<String> = async {
        let guid = form.get("guid").context("guid required")?.clone();
        let id = match operation.as_str() {
            "register" => {
                a.store
                    .register(
                        guid,
                        form.get("newGUID").context("newGUID required")?.clone(),
                        form.get("newPassword")
                            .context("newPassword required")?
                            .clone(),
                    )
                    .await?
            }
            "verify" => {
                let password = form.get("password").context("password required")?.clone();
                ensure!(!password.is_empty(), "email and password required");
                a.store.authenticate(guid, password).await?
            }
            "setName" => {
                let id = a
                    .store
                    .authenticate(
                        guid,
                        form.get("password").context("password required")?.clone(),
                    )
                    .await?;
                let _claim = a.claim(id)?;
                a.store
                    .choose_name(id, form.get("name").context("name required")?.clone())
                    .await?;
                id
            }
            _ => bail!("account operation not implemented"),
        };
        tracing::info!(account = id, operation, "account request completed");
        Ok("<Success/>".into())
    }
    .await;
    let xml = match result {
        Ok(xml) => xml,
        Err(e) => {
            tracing::warn!(error=%e, operation, "account request rejected");
            format!("<Error>{}</Error>", xml_escape(&e.to_string()))
        }
    };
    // The original account service carries semantic failures in XML, not HTTP error pages.
    (
        [
            ("content-type", "application/xml"),
            ("cache-control", "no-store"),
        ],
        xml,
    )
        .into_response()
}
async fn legends(
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
        let span = form.get("timespan").context("timespan required")?.clone();
        let account = form
            .get("accountId")
            .context("accountId required")?
            .parse()?;
        let character = form.get("charId").context("charId required")?.parse()?;
        a.store.legends(span, account, character).await
    }
    .await;
    match result {
        Ok(xml) => (
            [
                ("content-type", "application/xml"),
                ("cache-control", "no-store"),
            ],
            xml,
        )
            .into_response(),
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
        [
            ("content-type", "application/x-shockwave-flash"),
            ("cache-control", "no-store"),
        ],
        a.swf.as_ref().clone(),
    )
        .into_response()
}
async fn health(State(a): State<App>) -> Response {
    let started = a.started;
    match a.world.directory(move |d| Ok(d.health(started))).await {
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
    if origin != Some(a.origin.as_str()) {
        tracing::warn!(?origin, expected=%a.origin, "unapproved WebSocket origin");
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
        .exchange(
            vec![InventoryWrite {
                id: plan.character,
                account: plan.account,
                revision: plan.revision,
                items: plan.items.clone(),
            }],
            plan.vault.clone(),
        )
        .await
        .and_then(|mut revisions| {
            revisions
                .pop()
                .context("inventory commit returned no revision")
        });
    a.world
        .for_player(plan.player)
        .call(move |w| w.finish_inventory(plan, result))
        .await
}
async fn persist_trade(a: &App, plan: TradePlan) -> Result<()> {
    let id = plan.plans[0].player;
    let writes = plan
        .plans
        .iter()
        .map(|p| InventoryWrite {
            id: p.character,
            account: p.account,
            revision: p.revision,
            items: p.items.clone(),
        })
        .collect();
    let result = a.store.exchange(writes, None).await;
    a.world
        .for_player(id)
        .call(move |w| w.finish_trade(plan, result))
        .await
}
fn trade_offer(r: &mut Reader<'_>) -> Result<[bool; 12]> {
    ensure!(r.i16()? == 12, "trade offer must contain twelve slots");
    let mut values = [false; 12];
    for value in &mut values {
        let raw = r.u8()?;
        ensure!(raw <= 1, "invalid trade offer boolean");
        *value = raw == 1;
    }
    Ok(values)
}
fn player_text(r: &mut Reader<'_>) -> Result<String> {
    let text = r.utf()?;
    r.done()?;
    Ok(text)
}
#[derive(Default)]
struct Session {
    account: Option<Owner>,
    player: Option<i32>,
    expected: Option<i32>,
    reconnecting: bool,
}
fn collision_report(other: bool, r: &mut Reader<'_>) -> Result<()> {
    // Reports cannot grant hits: the actor advances and resolves projectiles itself.
    r.i32()?;
    r.u8()?;
    r.i32()?;
    if other {
        r.i32()?;
    }
    r.done()
}
async fn handle(
    a: &mut App,
    packet: Packet,
    session: &mut Session,
    tx: &mpsc::Sender<Packet>,
    lost: &tokio::sync::watch::Sender<bool>,
) -> Result<()> {
    let Session {
        account,
        player,
        expected,
        reconnecting,
    } = session;
    ensure!(!*reconnecting, "session is transferring");
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
            let guid = a.keys.decrypt(&r.utf()?)?;
            let password = a.keys.decrypt(&r.utf()?)?;
            let secret = a.keys.decrypt(&r.utf()?)?;
            ensure!(
                secret.is_empty(),
                "external platform authentication is not implemented"
            );
            let keytime = r.i32()?;
            let n = usize::try_from(r.i16()?)?;
            ensure!(n == 0 || n == 16, "invalid transfer key size");
            let key = r.take(n)?.to_vec();
            ensure!(r.i32()? == 0, "map editor JSON not supported");
            for _ in 0..4 {
                r.utf()?;
            }
            r.done()?;
            let id = a.store.authenticate(guid, password).await?;
            *account = Some(a.claim(id)?);
            let (destination, character) = a
                .world
                .directory(move |d| {
                    if key.is_empty() {
                        Ok((d.initial(game)?, None))
                    } else {
                        Ok((game, Some(d.consume(id, game, keytime, key)?)))
                    }
                })
                .await?;
            *expected = character;
            a.world = a.world.in_world(destination);
            let body = a.world.call(|w| Ok(w.map_info())).await?;
            tx.send(Packet {
                id: wire::MAP_INFO,
                body,
            })
            .await?;
            tracing::info!(
                account = id,
                requested_world = game,
                actual_world = destination,
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
                ensure!(
                    expected.is_none(),
                    "transfer requires LOAD of the existing character"
                );
                ensure!(r.i16()? as u16 == WIZARD, "only Wizard is implemented");
                r.done()?;
                a.store.create(account, a.content.get(WIZARD)?).await?
            } else {
                let id = r.i32()?;
                r.done()?;
                ensure!(
                    expected.is_none_or(|bound| bound == id),
                    "transfer character mismatch"
                );
                a.store.load(account, id).await?
            };
            let output = crate::world::Output {
                packets: tx.clone(),
                lost: lost.clone(),
            };
            let vault = if a.world.call(|w| Ok(w.name == "Vault")).await? {
                Some(a.store.vault(account).await?)
            } else {
                None
            };
            *player = Some(
                a.world
                    .call(move |w| {
                        if let Some(vault) = vault {
                            w.load_vault(vault)?;
                        }
                        w.enter(ch, Some(output))
                    })
                    .await?,
            );
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
                .call(move |w| w.native_shoot(id, time, bullet, kind, pos, angle))
                .await?;
        }
        wire::PLAYER_TEXT => {
            let text = player_text(&mut r)?;
            let id = player.context("chat before character")?;
            let received = std::time::Instant::now();
            if let Err(error) = a.world.directory(move |d| d.chat(id, text, received)).await {
                tracing::warn!(player=id, error=%error, "local chat rejected");
                tx.send(crate::world::chat_error(&error.to_string()))
                    .await?;
            }
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
        66 | 51 => collision_report(packet.id == 66, &mut r)?,
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
        21 => {
            let name = r.utf()?;
            r.done()?;
            let id = player.context("trade request before character")?;
            a.world.call(move |w| w.request_trade(id, name)).await?;
        }
        1 => {
            r.done()?;
            let id = player.context("trade cancel before character")?;
            a.world
                .call(move |w| {
                    w.cancel_trade(id, "Trade cancelled.");
                    Ok(())
                })
                .await?;
        }
        37 => {
            let values = trade_offer(&mut r)?;
            r.done()?;
            let id = player.context("trade change before character")?;
            a.world.call(move |w| w.change_trade(id, values)).await?;
        }
        57 => {
            let mine = trade_offer(&mut r)?;
            let theirs = trade_offer(&mut r)?;
            r.done()?;
            let id = player.context("trade accept before character")?;
            if let Some(plan) = a
                .world
                .call(move |w| w.accept_trade(id, mine, theirs))
                .await?
            {
                persist_trade(a, plan).await?;
            }
        }
        35 => {
            let slot = slot(&mut r)?;
            r.done()?;
            let id = player.context("inventory drop before character")?;
            let plan = a.world.call(move |w| w.drop_item(id, slot)).await?;
            persist(a, plan).await?;
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
            if let Err(e) = a.store.choose_name(account, name.clone()).await {
                tracing::warn!(account, error=%e, "name choice rejected");
                let mut o = Writer::new();
                o.u8(0).utf(&e.to_string());
                tx.send(Packet { id: 20, body: o.0 }).await?;
                return Ok(());
            }
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
            let portal = if packet.id == 3 { Some(r.i32()?) } else { None };
            r.done()?;
            let id = player.context("transfer before character")?;
            let (destination, keytime, key, name) = a
                .world
                .directory(move |d| {
                    let (account, character, destination) = d.destination(id, portal)?;
                    let (keytime, key, name) = d.issue(account, character, destination)?;
                    Ok((destination, keytime, key, name))
                })
                .await?;
            depart(a, id).await?;
            *player = None;
            let mut o = Writer::new();
            o.utf(&name)
                .utf("")
                .i32(2050)
                .i32(destination)
                .i32(keytime)
                .i16(key.len() as i16);
            o.0.extend(key);
            tx.send(Packet { id: 39, body: o.0 }).await?;
            *reconnecting = true;
            account.take();
            tracing::info!(
                player = id,
                destination,
                "durable departure acknowledged; reconnect queued"
            );
        }
        _ => bail!(
            "packet {} is not implemented in this development build",
            packet.id
        ),
    }
    Ok(())
}
async fn connection(socket: WebSocket, mut a: App) {
    let (mut wsout, mut wsin) = socket.split();
    let (tx, mut rx) = mpsc::channel::<Packet>(64);
    let (lost, mut lost_rx) = tokio::sync::watch::channel(false);
    let mut shutdown = a.shutdown.subscribe();
    let deadline = tokio::time::sleep(std::time::Duration::from_secs(10));
    tokio::pin!(deadline);
    let mut codec = Codec::server();
    let mut session = Session::default();
    let mut reconnect_sent = false;
    let mut ping = tokio::time::interval(std::time::Duration::from_secs(5));
    let mut serial = 0;
    let mut packet_window = std::time::Instant::now();
    let mut packet_count = 0usize;
    'connection: loop {
        tokio::select! {
            incoming=wsin.next()=>match incoming {
                Some(Ok(Message::Binary(bytes)))=>{
                    let packets=match codec.feed(&bytes) {Ok(p)=>p,Err(e)=>{tracing::error!(error=%e,"invalid packet stream");break;}};
                    if packet_window.elapsed().as_secs()>=1 {packet_window=std::time::Instant::now();packet_count=0;}
                    packet_count=packet_count.saturating_add(packets.len());
                    if packet_count>512 {tracing::warn!(player=session.player,"input packet rate exceeded");break;}
                    for packet in packets {
                        if session.reconnecting { break; }
                        let id=packet.id;
                        if let Err(e)=handle(&mut a,packet,&mut session,&tx,&lost).await {
                            tracing::error!(account=session.account.as_ref().map(|o|o.id),player=session.player,packet=id,error=%e,"game command rejected");
                            if matches!(id,1|21|37|57) && let Some(player) = session.player {
                                let reason = e.to_string();
                                let closed = a.world.for_player(player).call(move |w| Ok(w.cancel_trade(player, &reason))).await;
                                match closed { Ok(true) => {}, Ok(false) => { let mut out = Writer::new(); out.i32(1).utf(&e.to_string()); if wsout.send(Message::Binary(codec.frame(12,out.0).into())).await.is_err() {break 'connection;} }, Err(error) => { tracing::error!(%error,"trade rejection notification failed"); break 'connection; } }
                                continue;
                            }
                            if matches!(id,30|35|65) {
                                let mut o=Writer::new();o.i32(1);
                                if wsout.send(Message::Binary(codec.frame(4,o.0).into())).await.is_err(){break 'connection;}
                                continue;
                            }
                            let f=fail(&e.to_string());
                            let _=wsout.send(Message::Binary(codec.frame(f.id,f.body).into())).await;
                            break 'connection;
                        }
                    }
                },
                Some(Ok(Message::Ping(bytes)))=>{if wsout.send(Message::Pong(bytes)).await.is_err(){break}},
                Some(Ok(Message::Close(_)))|None=>break,
                Some(Ok(_))=>{tracing::warn!("non-binary game message rejected");break},
                Some(Err(e))=>{tracing::warn!(error=%e,"WebSocket failed");break},
            },
            _=shutdown.changed()=>break,
            _=lost_rx.changed()=>{tracing::warn!(player=session.player,"closing slow game connection before its state diverges");break;},
            _=&mut deadline,if session.player.is_none() && (!session.reconnecting || reconnect_sent)=>{tracing::warn!(reconnecting=session.reconnecting,reconnect_sent,"game selection or reconnect-close deadline exceeded");break;},
            outgoing=rx.recv()=>{
                let Some(packet)=outgoing else {break};
                let reconnect=packet.id==39;
                if wsout.send(Message::Binary(codec.frame(packet.id,packet.body).into())).await.is_err(){break;}
                // Ruffle can discard unread TCP bytes when the proxy closes before its next frame.
                if reconnect {reconnect_sent=true;deadline.as_mut().reset(tokio::time::Instant::now()+std::time::Duration::from_secs(10));}
            },
            _=ping.tick()=>{if session.account.is_some(){serial+=1;let mut o=Writer::new();o.i32(serial);
                if wsout.send(Message::Binary(codec.frame(wire::PING,o.0).into())).await.is_err(){break;}}}
        }
    }
    if let Some(id) = session.player
        && let Err(e) = depart(&a, id).await
    {
        tracing::error!(player=id,error=%e,"disconnect save failed; avatar retained for diagnosis");
        if let Some(owner) = session.account.take() {
            a.quarantined
                .lock()
                .expect("quarantine registry poisoned")
                .push(owner);
        }
    }
}
async fn depart(a: &App, id: i32) -> Result<()> {
    loop {
        if let Some(ch) = a
            .world
            .for_player(id)
            .call(move |w| w.begin_departure(id))
            .await?
        {
            if ch.alive {
                a.store.progress(ch).await?;
            }
            return a
                .world
                .for_player(id)
                .call(move |w| w.finish_departure(id))
                .await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}
#[derive(Deserialize)]
struct BotCreate {
    guid: String,
    world: i32,
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
        let destination = a.world.directory(move |d| d.initial(input.world)).await?;
        let id = a
            .world
            .in_world(destination)
            .call(move |w| w.enter(ch, None))
            .await?;
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
        if let Some(plan) = a
            .world
            .for_player(id)
            .call(move |w| w.action(id, input.action))
            .await?
        {
            persist(&a, plan).await?;
        }
        a.world.for_player(id).call(move |w| w.observe(id)).await
    }
    .await;
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => error(e),
    }
}
#[derive(Deserialize)]
struct BotTrade {
    player: i32,
    action: TradeAction,
}
async fn bot_trade(
    State(a): State<App>,
    headers: HeaderMap,
    Json(input): Json<BotTrade>,
) -> Response {
    let result: Result<serde_json::Value> = async {
        control(&a, &headers)?;
        let id = input.player;
        if let Some(plan) = a
            .world
            .for_player(id)
            .call(move |w| w.script_trade(id, input.action))
            .await?
        {
            persist_trade(&a, plan).await?;
        }
        a.world.for_player(id).call(move |w| w.observe(id)).await
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
                && let Some(plan) = a.world.for_player(id).call(move |w| w.action(id, action)).await?
            {
                persist(&a, plan).await?;
            }
            frames.push(a.world.for_player(id).call(move |w| w.observe(id)).await?);
        }
        Ok(frames)
    }
    .await;
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => error(e),
    }
}
#[derive(Deserialize)]
struct MapView {
    player: Option<i32>,
}
async fn map(State(a): State<App>, headers: HeaderMap, Query(view): Query<MapView>) -> Response {
    let world = match view.player {
        Some(id) => a.world.for_player(id),
        None => a.world.in_world(-1),
    };
    let result: Result<serde_json::Value> = async {
        control(&a, &headers)?;
        world.call(move |w| w.inspect_map(view.player)).await
    }
    .await;
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
        a.world
            .for_player(input.player)
            .call(move |w| w.observe(input.player))
            .await
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
pub async fn serve(a: App, bind: SocketAddr, client: PathBuf) -> Result<()> {
    let deaths = a.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let progress = match deaths.world.collect(|w| w.pending_progress()).await {
                Ok(plans) => plans,
                Err(e) => {
                    tracing::error!(error=%e, "progress coordinator stopped");
                    break;
                }
            };
            if !progress.is_empty() {
                match deaths
                    .store
                    .progress_batch(progress.iter().map(|(_, ch)| ch.clone()).collect())
                    .await
                {
                    Ok(()) => {
                        for (id, snapshot) in progress {
                            if let Err(e) = deaths
                                .world
                                .for_player(id)
                                .call(move |w| w.finish_progress(id, snapshot))
                                .await
                            {
                                tracing::error!(player=id, error=%e, "committed progress notification failed");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(error=%e, "progress persistence failed; characters quarantined");
                        for (id, _) in progress {
                            let reason = e.to_string();
                            if let Err(e) = deaths
                                .world
                                .for_player(id)
                                .call(move |w| w.poison(id, reason))
                                .await
                            {
                                tracing::error!(player=id,error=%e,"progress quarantine notification failed");
                            }
                        }
                    }
                }
            }
            let pending = match deaths.world.collect(|w| Ok(w.pending_deaths())).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(error=%e,"death coordinator stopped");
                    break;
                }
            };
            for (id, ch, killer) in pending {
                match deaths.store.death(ch, killer).await {
                    Ok(()) => {
                        if let Err(e) = deaths
                            .world
                            .for_player(id)
                            .call(move |w| w.finish_death(id))
                            .await
                        {
                            tracing::error!(player=id,error=%e,"committed death notification failed");
                        }
                    }
                    Err(e) => {
                        let reason = e.to_string();
                        tracing::error!(player=id,error=%e,"death persistence failed; character quarantined");
                        if let Err(error) = deaths
                            .world
                            .for_player(id)
                            .call(move |w| w.poison(id, reason))
                            .await
                        {
                            tracing::error!(player=id,%error,"death quarantine notification failed");
                        }
                    }
                }
            }
        }
    });
    let listener = tokio::net::TcpListener::bind(bind).await.with_context(|| {
        format!("cannot listen on {bind}; another server may already use this port. Stop it and rerun ./run.sh")
    })?;
    let app = Router::new()
        .route("/legacy-api/char/list", post(chars))
        .route("/legacy-api/char/fame", post(fame))
        .route("/legacy-api/account/{operation}", post(account_request))
        .route("/legacy-api/fame/list", post(legends))
        .nest_service("/sfx", tower_http::services::ServeDir::new(a.sfx.clone()))
        .route("/client.swf", get(swf))
        .route("/game", get(websocket))
        .route("/health", get(health));
    let app = app
        .route("/dev/join", post(bot_create))
        .route("/dev/action", post(bot_action))
        .route("/dev/trade", post(bot_trade))
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
    tracing::info!(address=%bind,origin=%a.origin,max_players=a.max_players,behind_proxy=a.behind_proxy,"game frontdoor listening");
    if !a.behind_proxy {
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
    fn original_otherhit_has_target_but_squarehit_does_not() {
        let mut out = Writer::new();
        out.i32(1234);
        out.u8(7);
        out.i32(1001);
        let square = out.0.clone();
        out.i32(5000);
        assert_eq!(square.len(), 9);
        assert_eq!(out.0.len(), 13);
        collision_report(false, &mut Reader::new(&square)).unwrap();
        collision_report(true, &mut Reader::new(&out.0)).unwrap();
        assert!(collision_report(false, &mut Reader::new(&out.0)).is_err());
        for end in 0..out.0.len() {
            assert!(collision_report(true, &mut Reader::new(&out.0[..end])).is_err());
        }
        out.u8(0);
        assert!(collision_report(true, &mut Reader::new(&out.0)).is_err());
    }
    #[test]
    fn native_trade_offer_requires_exact_canonical_twelve_slot_shape() {
        let mut out = Writer::new();
        out.i16(12);
        for slot in 0..12 {
            out.u8(u8::from(slot == 4));
        }
        let mut r = Reader::new(&out.0);
        let offer = trade_offer(&mut r).unwrap();
        r.done().unwrap();
        assert!(offer[4]);
        assert_eq!(offer.iter().filter(|v| **v).count(), 1);
        for count in [-1, 0, 11, 13, i16::MAX] {
            let mut out = Writer::new();
            out.i16(count);
            assert!(trade_offer(&mut Reader::new(&out.0)).is_err());
        }
        out.0[6] = 2;
        assert!(trade_offer(&mut Reader::new(&out.0)).is_err());
        out.0[6] = 1;
        out.0.pop();
        assert!(trade_offer(&mut Reader::new(&out.0)).is_err());
    }
    #[test]
    fn selected_player_text_has_only_one_utf_string() {
        let mut out = Writer::new();
        out.utf("café 😀");
        assert_eq!(player_text(&mut Reader::new(&out.0)).unwrap(), "café 😀");
        for end in 0..out.0.len() {
            assert!(player_text(&mut Reader::new(&out.0[..end])).is_err());
        }
        out.utf("ForgedSender");
        assert!(player_text(&mut Reader::new(&out.0)).is_err());
        assert!(player_text(&mut Reader::new(&[0, 1, 0xff])).is_err());
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
