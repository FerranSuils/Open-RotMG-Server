<h1><img src=".github/readme-icon.png" width="44" height="44" align="absmiddle" alt=""> Open RotMG / Server</h1>

Rust backend for the Realm of the Mad God 2012 (123.5.1) client.

See https://openrotmg.org/restoration/ to see what's completed.


## Run locally

Install Rust/Cargo and Python 3, then run. The client downloads automatically:

```sh
./run.sh 8080
```

Open the URL printed when the server is ready.

## Gameplay scope

This branch combines inland populations, quests, Snake Pit/Sprite World/Undead Lair, permanent stat potions, and Realm closure → Castle/paired Guardians → Oryx → Incantation-unlocked Wine Cellar → Oryx 2. Maps, AI and drop policies are reconstruction candidates, not authenticated 123.5.1 behavior; see `data/README.md`.

Native checks cover earned equipment and storage recovery, permanent death and replacement, and prepared-character Stheno/Oryx victories. A normal Stheno roll was looted and saved as an Incantation; its dungeon admission was staged. The full earned journey, native Guardian/Oryx 2 victories and earned Cellar unlock/reward return remain unverified. The draft PR does not claim those outcomes or a finished gameplay video.

Keep receive-time funding, mirrored native shot allocation, private rewards and commit-before-publication transfers/unlocks intact. Canonical inventory is `characters.items`/`revision`, not the separately checkpointed fields in character stats JSON. Run `cargo test --locked` and strict all-target Clippy before changing these seams.

---

Join Discord: https://discord.gg/EwZWNzzYwP
