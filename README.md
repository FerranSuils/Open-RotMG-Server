# OpenRotMG server

Server-side reconstruction of Realm of the Mad God build **123.5.1**.

This is the initial NR-CORE source import, not a working 123.5.1 server. The inherited engine targets 27.7.X13 and .NET Framework 4.6. Its protocol, mechanics, content and persistence still need target-era adaptation. No server build or gameplay has been verified.

## Starting point

The app/account service is in `server/`, the world server in `wServer/`, and shared code in `common/`. The dungeon-generator dependency is vendored at its recorded revision so a checkout does not depend on a disappearing submodule.

`UPSTREAM.json` records the source revision. `UPSTREAM.md` preserves the upstream setup notes and credits; these are historical instructions, not a tested OpenRotMG runbook. The inherited Dockerfile is also unvalidated.

Embedded upstream private keys have been removed. Their placeholders intentionally cannot initialize RSA. Configure a new server-controlled keypair and matching client public key before implementing login. Never reuse an archived key or import a historical database dump.

The next implementation milestone is a buildable scaffold followed by the 123.5.1 account/protocol/browser vertical slice. Preserve the client-visible mechanics and add tests before substituting later-era assumptions.

## Related repositories

Client and Ruffle browser host: https://github.com/OpenRotMG/client

Historical evidence and source snapshots: https://github.com/OpenRotMG/research

## License

Our server changes are AGPL-3.0-or-later. Preserve the inherited NR-CORE notices and its AGPL license. The dungeon generator retains its own license, credits and additional permission. Archival publication does not establish historical gameplay accuracy.
