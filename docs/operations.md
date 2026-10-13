# Server development and hosting

The development fixture supports Wizard movement, staff/spell combat, potions, loot/equipment, permanent death and SQLite reconnect persistence. It is not the complete 2012 game.

`./run.sh PORT --dev-control` enables authenticated loopback script control for gameplay checks using `python3 scripts/play.py`. It is not needed to play normally. On Windows, use the sibling client's `run.bat`.

For hosting, build with `cargo build --release --locked` and run `openrotmg --config /etc/openrotmg/config.json`. The examples in `scripts/deploy/` keep TLS/static website serving at Caddy and the backend on loopback. Set explicit absolute data/client/state paths, the exact HTTPS origin, player limit and `realm_seed`; public configuration rejects development control. Local launches choose a random seed and log it. Keep state private and back up the persistent disk, including SQLite WAL, before upgrading.

`GET /health` reports monotonic process `uptime_seconds`, native human `players`, and separate `qa_actors`. `bots` is explicitly zero: gameplay bots are not implemented; future populations need their own identity, not the QA control flag.

Native registration converts a guest without changing account ownership; subsequent login uses email/password rather than the guest GUID. SQLite stores Argon2id verifiers. Registered accounts can choose one free, unique 3–10-letter name through the original client; name changes after that, email verification and password recovery are not implemented.

Independent credential-free backup and offline restore instructions are in `scripts/backups/OPERATIONS.md`.
