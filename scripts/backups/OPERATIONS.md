# Independent state backups

The private `OpenRotMG/Open-RotMG-Backups` repository stores daily `state.json.gz` exports with Git history. The VM's `openrotmg-backup.timer` runs at **02:00 UTC**, catches missed runs after boot, and retries failures at 15-minute intervals, at most three starts per hour. Google disk snapshots remain separate.

`scripts/backups/backup.py` reads each live SQLite database inside a read-only transaction. It preserves every current persisted account ID/name, character field, inventory/revision/alive flag, immutable death, account-private eight-slot Vault and Discord XP/cooldown/replay field. Version 2 includes Vaults and schema-1 character class/base/progression counters; version 3 also preserves the nonsecret `shore_bots(name, account)` registry, even when empty/disabled. Versions 1–3 export and restore through exact explicit schemas. Account GUIDs are login secrets and are never selected or exported. Passwords, password hashes/verifiers, tokens, keys and credential files are not backed up. New tables/columns or nested character fields fail closed until the allowlist is deliberately updated and tested. Unsaved in-memory state is not covered; daily backups are not zero-RPO.

The service uses a fine-grained token restricted to `OpenRotMG/Open-RotMG-Backups` contents read/write and mandatory metadata read, outside Git. Deploy keys are disabled by GitHub policy; that policy is preserved. Token renewal is required before its recorded expiry; no broad user token is copied to the VM. Its sandbox makes source databases read-only and hides game RSA/bot credentials. Success is recorded only after GitHub's branch matches the uploaded commit, in `/var/lib/openrotmg-backup/last-success.json`. Inspect `systemctl status openrotmg-backup.timer` and `journalctl -u openrotmg-backup.service`. The private repository's independent daily GitHub Actions check fails if the export is over 36 hours old; email delivery depends on GitHub notification settings.

## Restore without Google

Clone/download the **private** backup repository and the server tools using a separately available GitHub owner account. Keep this repository private: exports still contain player names, Discord IDs and save history. Private repository access follows existing organisation permissions.

```sh
./scripts/backups/run.sh restore --archive /path/to/state.json.gz --directory /path/to/new-private-directory
```

The destination must not exist. This creates `game.db` and `levels.sqlite`, validates SQLite integrity/foreign keys and preserves IDs/ownership relationships. It never overwrites production databases. **All guest and shore-bot login GUIDs are NULL/disabled**, not regenerated or reassigned: passwords/login secrets were deliberately excluded. The restored bot registry retains account ownership, not authentication; bot startup also requires operator-reviewed identity recovery. Verified guest ownership recovery remains a separate unresolved requirement. Do not claim automatic player login or bot resumption after this restore. Replace credentials/keys separately, and perform an operator-reviewed restore before starting services.

Tests: `python3 -m unittest discover -s scripts/backups -p 'test_*.py' -v`.
