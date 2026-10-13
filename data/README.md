# Recovered data

Selected `objects.xml`/`grounds.xml` and map assets remain third-party material; the backend license does not relicense them.

**Map candidates, not authenticated official 123.5.1 layouts:**
- Tutorial, Nexus, Limbo and Vault: trapped/rotmg_svr at `593e490803fe0ad168ead408cacd5287d778acfe`. The old 96×96 shore crop remains a reference; live Realms use seeded 512×512 island terrain with connected roads and selected-client grounds. This is a reconstruction, not the recovered original generator.
- Oryx Castle: used-cell geometry and roster from Bledixon/Project-B-Public `9947199c8d384cc8220657bfa7519dc92e47adb9`, `server-source/wServer/realm/worlds/OryxCastle.jm` (AGPL; see root `LICENSE`). Required Project-B credits: Bledixon, Kithio, Travoos, Lunarshade, Club559, Skilly, BLOODQWEEN, Miniguy, Arcanuo, Jade, ossimc82 and everyone who helped make the project. The newer alloy Castle was rejected because it contains unsupported Janus/custom markers.
- Oryx Chamber and Wine Cellar: used-cell geometry and rosters from Zolmex/alloy-server at `c4bd1169a1593fd775b59d25a40edfa1cbd475c4`, `OryxChamber.jm` / `WineCellar.jm`; see `ALLOY-LICENSE`. JSON absent grounds retain the author's void default `0xff`; ambiguous Castle `Grey Circle` is explicitly selected as `0x61`. These are candidate layouts, not authenticated official maps.
- Cave: lossless `Common/Resources/World/Data/PirateCave.jm` conversion from Zolmex/alloy-server at `c4bd1169a1593fd775b59d25a40edfa1cbd475c4`; full geometry, roster and connected-wall configurations retained. Original ancestry is unverified; see `ALLOY-LICENSE`.

HP/projectiles use selected XML. AI/loot derive from trapped's Shore/Cave behavior tables; XP, credit thresholds, respawn, drop rates and lifetimes remain provisional server policies, not established historical fidelity. The bounded shore encounters stay near coastal arrival; inland populations and Realm closure are not implemented. Earned base fame is XP/1000, a reference-server policy rather than verified 123.5.1 scoring; historical death bonuses are not restored. Account-private loot and Vault access are enforced server-side.

https://github.com/trapped/rotmg_svr
https://github.com/Zolmex/alloy-server
https://github.com/Bledixon/Project-B-Public
