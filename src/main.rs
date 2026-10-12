mod config;
mod content;
mod crypto;
mod server;
mod store;
mod wire;
mod world;
use anyhow::{Result, ensure};
use rand::RngCore;
use std::sync::Arc;
#[tokio::main]
async fn main() -> Result<()> {
    let started = std::time::Instant::now();
    let filter = match std::env::var("RUST_LOG") {
        Ok(v) => tracing_subscriber::EnvFilter::try_new(v)?,
        Err(std::env::VarError::NotPresent) => tracing_subscriber::EnvFilter::new("openrotmg=info"),
        Err(e) => return Err(e.into()),
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();
    let args: Vec<_> = std::env::args().skip(1).collect();
    let (config, dev_control) = config::Config::load(&args)?;
    let client = config.client;
    let state = config.state;
    std::fs::create_dir_all(&state)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700))?;
    }
    if !config.hosted_client {
        for file in ["index.html", "vendor/ruffle.js"] {
            ensure!(
                client.join(file).is_file(),
                "missing client asset {file}; run ../client/run.sh --prepare-only, or ./run.sh for the hosted client"
            );
        }
    }
    ensure!(
        client.join("client.swf").is_file(),
        "missing reference client {}; run ./run.sh to download the pinned client",
        client.join("client.swf").display()
    );
    let content = Arc::new(content::Content::load(&config.data)?);
    let map = Arc::new(content::Map::load(
        &config.data.join("tutorial.wmap"),
        &content,
    )?);
    let keys = Arc::new(crypto::Keys::load(&state)?);
    let swf = Arc::new(keys.client(&client.join("client.swf"))?);
    let store = store::Store::open(&state.join("game.db"))?;
    let mut world = world::World::new(content.clone(), map)?;
    world.max_players = config.max_players;
    let world = world::Handle::spawn(world);
    let control = if dev_control {
        let mut bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        let token = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let path = state.join("control.token");
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        use std::io::Write;
        opts.open(&path)?.write_all(token.as_bytes())?;
        tracing::info!(token_file=%path.display(),"loopback script control enabled");
        Some(token)
    } else {
        None
    };
    let app = server::App {
        started,
        store,
        world,
        keys,
        content,
        swf,
        origin: config.origin,
        hosted_client: config.hosted_client,
        sfx: config.data.join("sfx"),
        max_players: config.max_players,
        behind_proxy: config.behind_proxy,
        requests: Arc::default(),
        connections: Arc::new(tokio::sync::Semaphore::new(config.max_players * 2)),
        control,
        owners: Arc::default(),
        bots: Arc::default(),
        quarantined: Arc::default(),
        shutdown: tokio::sync::watch::channel(false).0,
    };
    server::serve(app, config.bind, client).await
}
