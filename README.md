# OpenRotMG server

C# server scaffold for Realm of the Mad God **123.5.1**. Not playable yet.

- `server/`: account and HTTP services.
- `wServer/`: game protocol, world simulation and NPC behaviors.
- `common/`: shared data and persistence.
- `RotMG-Dungeon-Generator/`: dungeon-generation dependency.

**Start here:** get `server.sln` building, then adapt `wServer/networking/` to 123.5.1. The inherited code targets .NET Framework 4.6 and a later game protocol; its build is not yet verified.

AGPL-3.0. Upstream attribution is in `CREDITS`. Removed private keys must be replaced with a new keypair before login can work.
