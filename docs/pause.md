# Pause

`/pause` toggles a session-local pause. The world and network keep ticking; movement, new shots, item use, HP/MP regeneration, enemy targeting and hostile damage stop for that player. Teleporting from/to a paused player is rejected. Chat, inventory dragging, trading and leaving remain available. Reloading a character starts unpaused.

The pinned 123.5.1 `client.swf` (client commit `7e3f4b83df7f65ea2a7b57b10c2288ef02c5ae8c`, SHA-256 `07ae4fc3ca021a5bf08bbf94f057f7620be18b85e01abd763b3a198382e8fa48`) establishes:
- `ConditionEffect`: Paused index 21, mask `1 << 20`; player status stat 29.
- `Player.update`, `shoot`, ability use and inventory `attemptUse`: paused input suppression; gray player texture/minimap; `teleportTo` rejects paused senders/targets.
- Network `move`: paused tick acknowledgements use `(-1, -1)`. Keep their clocks without moving or banking movement credit.
- `Projectile.update`: no damage from a paused owner's already-fired projectile; collisions still consume that projectile. Hostile collisions skip paused players.
- Built-in help: `/pause` again resumes. No client changes are needed.

The eight-tile entry safety check and private pause/resume messages follow [the historical reference server](https://github.com/trapped/rotmg_svr/blob/593e490803fe0ad168ead408cacd5287d778acfe/wServer/realm/entities/player/Player.Chat.cs#L509-L552), not authenticated original 123.5.1 server code. The exact radius/boundary remains a reconstruction policy. Existing XP/loot, externally applied healing and cooldown policies are unchanged; no later-build `/p`, dungeon restriction or timed condition framework is introduced.

Run `cargo test --locked pause` and the full `cargo test --locked`. Native evidence must use the pinned client and isolated state/profile; see `videos.md`.
