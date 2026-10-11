# OpenRotMG server

Rust development backend for the recovered **123.5.1** client. One shared tutorial fixture: Wizard movement, staff/spell combat, potions, loot/equipment, permanent death and SQLite reconnect persistence.

Clone `server` and `client` beside each other. Install Rust stable/Cargo and Python 3, then run `../client/run.sh` from this directory (`../client/run.bat` on Windows). Open http://127.0.0.1:18923 . The launcher prepares pinned Ruffle and starts this backend; no separate proxy or database service.
