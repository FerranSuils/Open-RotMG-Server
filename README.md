<h1><img src="readme-icon.png" width="44" height="44" align="absmiddle" alt=""> Open RotMG / Server</h1>

Rust development backend for the recovered **123.5.1** client. One shared tutorial fixture: Wizard movement, staff/spell combat, potions, loot/equipment, permanent death and SQLite reconnect persistence.

Clone `server` and `client` beside each other. Install Rust stable/Cargo and Python 3, then run `../client/run.sh` from this directory (`../client/run.bat` on Windows). Open http://127.0.0.1:18923 . The launcher prepares pinned Ruffle and starts this backend; no separate proxy or database service.

For hosting, build with `cargo build --release --locked` and run `openrotmg --config /etc/openrotmg/config.json`. The examples in `deploy/` keep TLS/static website serving at Caddy and the backend on loopback. Set explicit absolute data/client/state paths, the exact HTTPS origin and an initial player limit; public configuration rejects development control. Keep state private and back up the persistent disk, including SQLite WAL, before upgrading.

`GET /health` reports monotonic process `uptime_seconds`, native human `players`, and separate `qa_actors`. `bots` is explicitly zero: gameplay bots are not implemented; future populations need their own identity, not the QA control flag.
