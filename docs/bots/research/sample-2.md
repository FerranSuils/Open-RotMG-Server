# Bot research: sample 2

Research only. These are behavior observations and proposed future acceptance scenes, not authorization to implement bots or claims that the shared server supports them.

## Sources and method

Six recordings were inspected, deliberately adding ordinary play, death/rebuilding, inventory work and trade counterexamples rather than more boss highlights.

| Recording | Upload date | Version/date qualification |
| --- | --- | --- |
| Whcxy3RJKIA: Realm of the Mad God part 1 - Biceps and Triceps | 2012-02-04 | Native connection text says 121.0.0; earlier than target |
| fubGzGmVqpI: Realm Of The Mad God - Episode 1 | 2012-08-17 | Exact build not verified |
| 4J4S4iGeLiw: Rotmg Trading Video | 2012-08-18 | Native connection text says 123.4.4; earlier than target |
| -Im_yUlCuSc: Rotmg:Mooselegs: Great Trade | 2012-10-15 | Exact build not verified |
| Fo_M0vXhtcQ: Realm Of The Mad God - Let's Play, Part 1 | 2012-10-21 | Exact build not verified; inspected mainly as a tutorial-context counterexample |
| Ex_Ix3dlB4U: RotMG Old Trading(Around January 2012) | 2013-05-11 | Native connection text says 123.0.0. Creator claims approximately January 2012 but admits uncertainty; capture date unverified |

https://www.youtube.com/watch?v=Whcxy3RJKIA

https://www.youtube.com/watch?v=fubGzGmVqpI

https://www.youtube.com/watch?v=4J4S4iGeLiw

https://www.youtube.com/watch?v=-Im_yUlCuSc

https://www.youtube.com/watch?v=Fo_M0vXhtcQ

https://www.youtube.com/watch?v=Ex_Ix3dlB4U

Reviewed 80 explicit-seek overview frames, then 508 frames across eight longer source windows spanning 740 seconds, plus native-resolution confirmation frames. Deeper intervals were 0.5, 1 or 2 seconds; the longest window was 130 seconds. This is visual/on-screen-chat analysis, not full continuous playback, audio analysis or measurement of reaction latency. Contact labels identify requested source seeks, not precise human input times. Repeated persistent chat is not new speech each frame. Loading/retry screens are not evidence of deliberate player idling.

Downloaded footage, per-video metadata/hashes, sampling records, contact sheets and native frames remain private and outside this repository. Source URLs and reviewed windows make the observations reviewable without republishing the recordings. A separate September 2012 episode, 5Iz7eq_2gMs, was metadata-screened but its gameplay was not inspected; reserve it as a candidate later comparison, not evidence for these findings.

## Strongest examples

### 1. Social decisions can compete with existing friendships

**Observed:** In fubGzGmVqpI around 8:59–9:47, background public chat contains a recruitment exchange between ExElite and Kadabu, not the recorder:

- "join our guild?"
- "me?" / "yes"
- "hm i dont know"
- "go in nexus?"
- "but i have my 2 best friends here :D"
- A promise of free potions every day
- "call them too"

This conversation continues while the recorder fights. It is a specific invitation, hesitation, incentive and attempted accommodation, not a welcome speech centered on the person recording.

**Unknown:** Whether the invitation was accepted, the claimed friends were actually present, or the promised potions existed/were delivered. This is evidence of expressed social priorities, not proof of guild membership or a friendship graph.

**Implication:** The first sample's short practical chat is only part of the picture. Brief messages can sustain a longer negotiation. Bots should have reasons to hesitate, stay with companions or include them in an invitation. Promises must correspond to real capabilities/resources, not invented daily rewards.

### 2. Death can interrupt a session without ending its social thread

**Observed:** Whcxy3RJKIA, 4:40–5:44: Biceps dies at level two, the death display shows 22 total fame, and the recording proceeds through character creation to a level-one Wizard. Triceps is visible again; a Biceps-labelled grave is near the new character at approximately 5:36. The creator's description explicitly describes playing with David as a team. Earlier 1:00–3:00 footage shows the two names in the same local activity, with changing spacing rather than fixed formation.

**Unknown:** The companion's complete death/recreation history, voice coordination, whether all transitions are unedited, and any relationship beyond this session. No vault-based re-equipping was observed for Biceps.

**Implication:** Death should clear the character's real inventory/progression without automatically erasing the account's acquaintances or reasons to rejoin a group. Rebuilding can itself become a shared event. Do not require an invented rage message or immediate logout after every death.

**Counterexample:** fubGzGmVqpI reaches level seven and dies near 9:48; the recording ends after its death display. There is no observed replacement in that video. A clip ending is not evidence that the player logged off, refused to rebuild or had a particular emotional reaction.

### 3. Ordinary play includes assessment, not just movement and shooting

**Observed:** Whcxy3RJKIA, 17:00–19:00, alternates fighting/travel with native item hovers, examining loot and a staff change. The local group changes spacing while the recorder handles inventory. In fubGzGmVqpI, 3:30–5:30, travel and combat are interrupted by loot/potion inspection and passing players rather than a continuous train.

**Unknown:** Exact thought process, attention limits and which pauses were deliberate versus recording/network effects. Camera-relative positions are not world-coordinate measurements. These do not establish universal retreat percentages or a timing distribution.

**Implication:** Comparing gear, making room, recovering and catching up are useful sources of natural variation. Different activity priorities can create stragglers without adding random mistakes. Some unusable-by-current-class items may still have storage, trade or other-character value; do not equate current equip incompatibility with worthlessness.

### 4. A meaningful deal can span several exchanges and bank trips

**Observed:** Ex_Ix3dlB4U, 4:50–6:04: the recorder trades repeatedly with Pixl, receives eight Life potions, stores items in the Vault, returns for another eight, and then receives one more. Native "Trade Successful" messages and changed inventory accompany the exchanges. At approximately 5:33 the recorder says "sold cdirk to pixl for 17 pure life". Later native tooltips identify Potion of Life and stored gear.

The last two exchanges, examined alone, show nothing offered back by the recorder. The wider sequence and the recorder's message support payment installments, not an automatic classification as gifts. The price is one reported deal in an uncertain-date/older-build recording, not a historical economy constant.

**Implication:** A short screenshot can hide the real task. Future bots may need to remember an agreed deal across capacity-limited exchanges, travel and storage actions. They must not say a deal is fulfilled after only one side accepts, or invent an off-ledger wallet. Native transactions can be atomic individually without a multi-exchange bargain being guaranteed against reneging.

**Additional observed thread:** 1:05–2:10 includes a trade proposal, private chat such as "deal?" and "lemme make room", then a Vault visit. The outcome of that earlier proposed bargain was not established in this pass. Negotiation/preparation is not completed settlement.

### 5. Low-level characters can have economic roles

**Observed:** Ex_Ix3dlB4U's trader is level one, with 46 account stars, simple equipped gear and extensive stored items. 4J4S4iGeLiw, approximately 0:10–1:20, showcases cross-class equipment and native tooltips in a Vault; its description asks to trade items for a good bow. No completed native exchange is shown there.

**Unknown:** Whether a particular character is a dedicated mule, a replacement, an alt or just temporarily storing items. Stars are not a calibrated combat-skill score.

**Implication:** Experience, resources and immediate role belong to more than current character level or equipped tier. A population made only of "weak beginner" and "strong fighter" would miss this dimension. A trader's inventory should come from genuine account holdings, not free replacement gear.

## Counterexamples and annotation safeguards

- **A promising title is not an outcome.** -Im_yUlCuSc shows an offer and chat such as "good trade huh" / "plz", then cuts to credits. No completion was verified. 4J4S4iGeLiw is an inventory showcase, not a demonstrated transaction. Do not classify either as a successful exchange from the title.
- **Quiet space can be special context.** Fo_M0vXhtcQ, approximately 5:00–6:30, includes the first-time Nexus sequence and a prompt saying the next visit will be more crowded. It is not evidence that ordinary public Nexus should be empty or that a human intentionally avoids crowds.
- **Not every visible line is human conversation.** These videos include NPC/system lines and repeated commercial trading-site spam. The automation status of advertising accounts is not established. Do not use every message/avatar as human-behavior ground truth or reproduce real-money advertising as bot atmosphere.
- **Chat recipients matter.** The guild exchange concerns other players, and the merchant recording includes private messages. Hearing an invitation does not mean every nearby bot should answer it.

## Candidate future acceptance scenes

These are research-derived proposals to revisit when normal shared-game support exists, not tests already passed.

| Scene | Desired observable behavior | Shared dependency / limit |
| --- | --- | --- |
| Companion stops to examine loot | Others can continue, wait or regroup without synchronized marching | Normal movement, item views and limited remembered observations |
| Death during paired activity | A legitimate replacement can recognize/rejoin a known companion; no inherited dead inventory | Durable account/character distinction, death, creation and transfers; graves only if supported normally |
| Invitation conflicts with companions | A brief hesitation or counteroffer reflects actual contacts and changes actions if accepted | Relevant chat/group or guild support; no fictional friends or rewards |
| Deal exceeds available carrying space | Inspect capacity, use real storage and continue/cancel explicitly; acknowledge actual settlement only | Normal inventory, trade/vault transactions and remembered commitments |
| Cross-class valuable loot | Current equip restrictions coexist with a reason to retain/store/trade an item | Real content compatibility and account resources |
| Public conversation addressed elsewhere | An uninvolved character can continue its own activity instead of joining every conversation | Message provenance/recipient scope and non-human message distinctions |

## What remains unresolved

This pass adds an observed completed economic sequence, a short death-to-replacement social continuation and a concrete invitation conflict. It still does not establish long-term friendship/reputation, the distribution of beginner/veteran/trader roles, typical prices, login/logout schedules, generosity rates or response latencies. None of the videos is authenticated exact 123.5.1 footage. Emotional reactions and voice coordination were not analyzed.

The next useful step is to consolidate these few behavior scenes and their shared-game dependencies, not build a bot framework or collect interchangeable recordings indefinitely. Any later research should answer a specific remaining question and include disconfirming examples.
