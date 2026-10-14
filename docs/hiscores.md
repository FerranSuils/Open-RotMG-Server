# Hiscores and Discord linking

`GET /api/hiscores` supplies permanent Discord top-ten messages and the website's full rankings. **Base fame** is the best one-character record per account, alive/dead; **collection** counts unique enemy-drop pickups; **total fame** sums immutable dead-character credited fame before spending. Current death records credit base fame, not historical bonuses. Ties share rank, ordered stably by account ID. Registered native accounts qualify after connecting; linking is optional and shore bots/QA are excluded.

Collection and inventory commit atomically. Starter/map gear, trades, Vault moves and player-dropped items never earn credit. There is no invented historical backfill. The loot tables define the collectible set; the website atlas uses those original sprites and silhouettes, and rejects catalogue mismatches.

Authenticated registered `/discord` issues a private random 128-bit code, SHA-256 stored, five-minute expiry, once/minute/account. Ephemeral Discord `/link` consumes it atomically, one-to-one; conflicts never replace links. `/kick`, `/recover`, unlinking and recovery privileges remain disabled. The client/SWF is unchanged.

`POST /internal/discord/link` accepts only `{code,discord_id}` from direct loopback plus its dedicated bearer credential; Origin, forwarding and unknown fields are rejected. Caddy denies `/internal/*`. The bot cannot read game state or control gameplay. Set `DISCORD_LINK_TOKEN_FILE` for both services and the bot's explicit loopback `GAME_API_URL`; hosted units use `LoadCredential` from root-private `/etc/openrotmg/discord-link-token`.

Before production migration, install/test schema-4 backups and hide the new credential in the backup sandbox. Nonsecret eligibility/collections/Discord IDs survive; credentials/code hashes do not. Restored associations grant no authentication or password recovery. Verification uses Rust/Clippy, exact backup roundtrips, encrypted two-account packet acceptance and website/Discord tests; packet acceptance is not rendered production gameplay acceptance.
