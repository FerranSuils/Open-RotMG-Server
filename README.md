# OpenRotMG server

Rust development backend for the recovered **123.5.1** client. One shared tutorial fixture: Wizard movement, staff/spell combat, potions, loot/equipment, permanent death and SQLite reconnect persistence.

Clone `server` and `client` beside each other. Install Rust stable/Cargo and Python 3, then run `../client/run.sh` from this directory (`../client/run.bat` on Windows). Open http://127.0.0.1:18923 . The launcher prepares pinned Ruffle and starts this backend; no separate proxy or database service.

Tests: `cargo test`. For scripted self-play, start with `../client/run.sh --dev-control`, then `python3 tools/play.py` in a fresh server world. Its authenticated `/dev/script` queues normal-rule actions; `/dev/observe` and `/dev/map` expose state. The token is in ignored `.local/control.token`, never logs. Control is disabled by default and the server binds loopback only.

The fixture layout, audio provenance and emulator-derived behavior are **not verified historical fidelity**. The supported normal client entry IDs `-4..=0` explicitly select this development fixture; Nexus/realm/Kitchen destinations, leveling and fame awards are not implemented. Original SWF bytes stay unchanged; the server serves a generated public-key-patched copy. Never commit `.local/`, keys or databases.

`src/wire.rs`: selected-client packets. `world.rs`: single-owner simulation. `content.rs`: runtime XML/map data. `store.rs`: transactional account/character records. Missing content/contracts: https://github.com/OpenRotMG/server/issues . Source notices: `CREDITS`. Original Rust code is AGPL-3.0; third-party rights remain applicable.
