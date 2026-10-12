<h1><img src=".github/readme-icon.png" width="44" height="44" align="absmiddle" alt=""> Open RotMG / Server</h1>

Rust backend for the Open RotMG recreation of Realm of the Mad God.

## Run locally

Install Rust/Cargo and Python 3, then run:

```sh
./run.sh
```

Open the https://openrotmg.org/local/#port=18923 URL printed when the server is ready. Allow local-network access if asked. No client clone needed; the launcher downloads a verified reference client on first run.

Errors appear in the terminal and `.local/server.log`; the browser page shows connection errors and its own log.

Prefer a local client? Stop the server, clone https://github.com/OpenRotMG/Open-RotMG-Client beside this repository as `client`, then run `./run.sh --local-client ../client` and open http://127.0.0.1:18923/.

---

Join Discord: https://discord.gg/EwZWNzzYwP
