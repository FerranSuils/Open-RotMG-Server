# OpenRotMG server

Rust backend for the recovered **123.5.1** client. Playable-loop implementation is in progress; not a running game yet.

Run `cargo test` (Rust stable). Keep the `client` repository beside this one to test the server's public-key patch against the preserved SWF. Local keys and databases must never be committed.

`src/wire.rs` implements the selected client's framed, continuously encrypted packet stream. `src/content.rs` loads client-matched definitions and the development map; `src/store.rs` provides transactional character/inventory persistence.

The recovered tutorial layout is **unverified**, including its Kitchen Portal exit. It is a development fixture, not an authenticated historical map. Source notes are in `CREDITS`. Original Rust code is AGPL-3.0; third-party notices remain applicable.
