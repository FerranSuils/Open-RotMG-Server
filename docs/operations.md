# Server development and hosting

The development fixture supports Wizard movement, staff/spell combat, potions, loot/equipment, permanent death and SQLite reconnect persistence. It is not the complete 2012 game.

The launcher needs Git, Rust/Cargo and Python 3. Keep development separate from other worktrees and saves:

```sh
git worktree add -b feat/my-change ../my-change
cd ../my-change
./run.sh 8081 --state "$PWD/.local/qa" --client /absolute/path/to/client
```

`--client` (also `--local-client`) selects a prepared client checkout; omit it to download the pinned client into the selected state directory. The launcher prints its revision/dirty status and environment, retains logs in that state's `server.log`, and rejects an occupied port rather than choosing another. The binary also accepts `--state /absolute/path` with `--config`.

Add `--dev-control` for authenticated loopback gameplay checks using `python3 scripts/play.py --url http://127.0.0.1:8081 --state "$PWD/.local/qa"`. It is not needed to play normally. See the four recipes in `examples/testing/` for focused experiments; prepared state is not ordinary progression evidence. On Windows, use the sibling client's `run.bat`.

For hosting, build with `cargo build --release --locked` and run `openrotmg --config /etc/openrotmg/config.json`. The examples in `scripts/deploy/` keep TLS/static website serving at Caddy and the backend on loopback. Set explicit absolute data/client/state paths, the exact HTTPS origin and an initial player limit; public configuration rejects development control. Keep state private and back up the persistent disk, including SQLite WAL, before upgrading.

`GET /health` reports monotonic process `uptime_seconds` and distinct native human `players`, autonomous `bots` and `qa_actors`. Enable the three-Wizard population explicitly with `"shore_bots": true`; reserve at least one human slot and never enable production QA control. Bot accounts are created once with normal starter equipment, resume committed characters, and remain retired after permanent death. Private disk/SQLite backups retain their registry, identity and inventory. Credential-free exports omit identities; restoring those exports requires operator-reviewed identity recovery before bot startup.

Independent credential-free backup and offline restore instructions are in `scripts/backups/OPERATIONS.md`.
