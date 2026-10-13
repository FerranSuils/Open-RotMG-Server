# Four testing recipes

Copy and adapt these into scratch work. They are examples, not a scenario framework; adding shared helpers or more examples requires explicit approval. Prepared state never proves earned progression. Keep formatting, focused tests and long native journeys separate.

- **Cover:** `cargo test --locked example_cover` constructs a hostile shot behind cover using real collision code.
- **Timing:** `cargo test --locked example_timing` sends SHOOT before MOVE. For sustained-fire and skipped-clock variants, see the existing native attack regressions.
- **Prepared state:** capture the guest GUID from your own disposable QA browser's character-list request. Run `ROTMG_QA_GUID=… cargo test --locked example_prepared_state -- --ignored --nocapture`. It creates a new private temporary directory, a level-eight Wizard and prepared equipment through the current Store API. Start `./run.sh PORT --state PRINTED_DIRECTORY --client CLIENT_DIRECTORY`, then LOAD that character in the same QA profile. Never use a production identity/profile. The server has no new seeding endpoint.
- **UI transition:** import `exampleNexusReturn` from `ui-transition.mjs`; pass your owned tab's CDP call function, its server log path and character ID. Start outside Nexus after the game HUD has rendered and the owned game is focused. It sends the selected client's Insert binding and requires a NEW same-character Nexus entry. Key-release failure cannot hide the original error. This demonstrates observation, not the boss portal's UI selection.

Rust recipes compile with the existing tests; the prepared-state recipe runs only when explicitly selected. No browser dependency is bundled: use your existing browser-scoped connection, never desktop input. Keep generated state, media and exploratory scripts outside Git.
