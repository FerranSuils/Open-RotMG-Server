mod content;
mod crypto;
mod server;
mod store;
mod wire;
mod world;
use anyhow::{Context, Result, ensure};
use rand::RngCore;
use std::{path::PathBuf, sync::Arc};
#[tokio::main]
async fn main() -> Result<()> {
    let filter = match std::env::var("RUST_LOG") {
        Ok(v) => tracing_subscriber::EnvFilter::try_new(v)?,
        Err(std::env::VarError::NotPresent) => tracing_subscriber::EnvFilter::new("openrotmg=info"),
        Err(e) => return Err(e.into()),
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let client = root
        .parent()
        .context("workspace parent missing")?
        .join("client");
    let state = root.join(".local");
    std::fs::create_dir_all(&state)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700))?;
    }
    for file in ["index.html", "vendor/ruffle.js"] {
        ensure!(
            client.join(file).is_file(),
            "missing client asset {file}; run python3 ../client/run.py --prepare-only"
        );
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.iter().all(|s| s == "--dev-control"),
        "usage: cargo run -- [--dev-control]"
    );
    let content = Arc::new(content::Content::load(&root.join("data"))?);
    let map = Arc::new(content::Map::load(
        &root.join("data/tutorial.wmap"),
        &content,
    )?);
    let keys = Arc::new(crypto::Keys::load(&state)?);
    let swf = Arc::new(keys.client(&client.join("client.swf"))?);
    let store = store::Store::open(&state.join("game.db"))?;
    let world = world::Handle::spawn(world::World::new(content.clone(), map)?);
    let control = if args.iter().any(|s| s == "--dev-control") {
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
        store,
        world,
        keys,
        content,
        swf,
        origin: "http://127.0.0.1:18923".into(),
        control,
        owners: Arc::default(),
        bots: Arc::default(),
        quarantined: Arc::default(),
        shutdown: tokio::sync::watch::channel(false).0,
    };
    server::serve(app, "127.0.0.1:18923".parse()?, client).await
}
