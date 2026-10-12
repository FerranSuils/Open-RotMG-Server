<h1><img src="../../.github/readme-icon.png" width="44" height="44" align="absmiddle" alt=""> Open RotMG / Backups</h1>

Credential-free export and offline restore of persisted game saves and Discord levels.

## Run locally

Requires Python 3. From this folder:

```sh
./run.sh restore --archive /path/to/state.json.gz --directory /path/to/new-private-directory
```

The destination must not exist. Restored guest login identities are disabled; credentials are not backed up. Deployment and privacy details are in `OPERATIONS.md`.

---

Join Discord: https://discord.gg/EwZWNzzYwP
