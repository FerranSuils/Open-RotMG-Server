# Bot research: decisions and continuity

The user approved an isolated offline experiment, not game integration. A Rust policy was compared against a profile-aware fixed-priority baseline across twelve constructed situations. Scoring responded to changing circumstances, but needed a switching margin to avoid churning between nearly equal goals. The common memory and validation rules, not scoring itself, preserved trade and death continuity.

## Method and boundary

```text
manually annotated situation + profile + remembered facts
                           -> intent + reason + next memory
```

The experiment contains 34 input frames, run with three profiles and three policy variants: 306 recorded decisions. Each variant receives the same observations and confirmed-event fixtures. The third variant removes the scorer's switching margin to isolate its effect.

These are synthetic inputs inspired by the observations in the two video research reports. Health, danger, distance and estimated loot value are invented test values, not measurements of recorded players. The historical seventeen-Life deal supplies the 8 + 8 + 1 installment structure, not a universal price or contract rule. No additional footage was reviewed, and the reserved recording remains uninspected.

The policy has no world, combat, movement, network connection, inventory mutation, language service or server dependency. It selects goals, not executable gameplay commands. The experiment's source, fixtures, traces and preview remain private; this report publishes the findings.

### Fair baseline

Both approaches use the same candidate eligibility, safety checks, incoming-deal ledger, character lifecycle and resource checks. Both receive these arbitrary profile weights:

| Profile | Survival | Resources | Companions |
| --- | ---: | ---: | ---: |
| Cautious | 6 | 4 | 3 |
| Resource-seeking | 3 | 8 | 3 |
| Companion-oriented | 4 | 3 | 8 |

The fixed baseline ranks eligible goals by the corresponding profile weight. It is not deliberately identical for every character. The scorer multiplies that weight by the annotated size of a need or opportunity. Recovery uses missing health plus annotated threat; loot uses estimated value; regrouping uses annotated separation. Invitation options use fixed experiment values. Neither approach is a calibrated model of human behavior.

The continuity variant keeps a valid prior goal when the challenger is less than eight utility points better. Invalid goals cannot be retained. This margin is an experiment parameter, not a reaction time or historical constant. Ties use stable candidate order.

## Situations and results

Source windows identify the inspiration, not a reconstruction of the participant's decisions. Recording details and limitations are in the two sample reports.

| Constructed situation | Inspiration | Result |
| --- | --- | --- |
| Conversation addressed elsewhere | Sample 2 recruitment, 8:59–9:47 | No invitation/request response is selected from the unrelated annotation |
| Injury, loot and a moving companion | Sample 2 ordinary play, 17:00–19:00 and 3:30–5:30 | Cautious chooses recovery, resource-seeking inspects loot, companion-oriented regroups; both approaches already produce this variation |
| Smaller injury and useful loot | Same ordinary-play windows | The scorer can inspect loot despite a cautious profile; fixed ordering continues to prefer recovery whenever eligible |
| Near-tie changes, then greater separation | Same ordinary-play windows | Raw scoring churns; a modest margin keeps the loot goal until separation becomes substantial |
| Danger interrupts loot | Ordinary-play/combat observations | Every variant selects retreat under the experiment's urgent-danger guard |
| Bag fills while companion moves away | Sample 2 inventory pauses | Loot becomes ineligible; continuity cannot keep it |
| Invitation conflicts with contacts | Sample 2 recruitment, 8:59–9:47 | Companion-oriented profile counterinvites; when the annotated offer includes contacts, it considers the invitation |
| Cross-class loot with a reason to inspect it | Sample 1 Mad Lab aftermath; sample 2 inventory assessment | Equip incompatibility does not discard manually estimated account/trade value; no classifier or valuation model is tested |
| Seventeen-unit deal across storage trips | Sample 2 completed deal, 4:50–6:04 | Outstanding balance follows 17, 9, 1, 0 only after confirmed receipts; acceptance and storage trips do not settle it |
| Unknown storage and absent counterparty | Same trade sequence and its limits | No invented capacity or hidden partner position; a pending deal can be deferred for other activity without losing the balance |
| Death and legitimate replacement | Sample 2 paired death/rejoining, 4:40–5:44 | Dead character cannot act; contact memory survives; a replacement uses its own supplied resource snapshot |
| Immediate help versus an impossible promise | Sample 1 health requests; sample 2 recruitment incentives | A two-potion offer requires two held potions; one potion produces a refusal rather than invented stock |

### 1. Variation does not require scoring

The fixed baseline already differentiates the three profiles. In the shared injury/loot/companion situation, both approaches produce recovery, loot inspection and regrouping respectively. This does not prove three convincing personalities; it demonstrates that the same input can produce different explainable intentions without looking at a scenario name.

Scoring earns its place when the magnitude changes. With less injury and a useful nearby opportunity, the cautious scorer can inspect loot instead of treating any missing health as its highest priority. Likewise, the companion-oriented scorer can inspect valuable loot when separation is small. These are defensible constructed choices, not observed human answers.

### 2. Raw scoring needs restraint, but excessive stickiness also fails

In the deliberately constructed cautious near-tie sequence, loot has a score of 120. Alternating separation annotations produce regrouping scores of 117 and 123. Raw scoring switches goals five times. An eight-point margin switches once, when the final regrouping score rises to 180. Fixed priorities never switch in this sequence, including at the final separation.

| Switching margin | Goal changes | Responds to final separation? |
| ---: | ---: | --- |
| 0 | 5 | Yes |
| 4 | 1 | Yes |
| 8 | 1 | Yes |
| 16 | 1 | Yes |
| 64 | 0 | No |

A generated grid of 1,200 five-frame loot/separation sequences, using the three profiles, produced 220 changes without a margin and 187 with the eight-point margin. No individual generated sequence had more changes with continuity. This checks stability over those inputs, not the optimality of eight, a human timing distribution or live social believability.

### 3. Commitments must be remembered without freezing the character

An initial version forced waiting whenever a deal remained unfinished and the counterparty was absent. A regression test supplied another available activity and failed: the character waited instead of inspecting loot. The corrected policy retains the balance but permits other activity when the deal cannot currently proceed. It resumes the deal when the counterparty becomes visible and capacity permits.

A completed deal also releases attention rather than trapping the character in a permanent completion state. An accepted exchange waits for a receipt; an explicit cancellation releases that wait without erasing the overall balance. Timeouts and reliable receipt/cancellation adapters are not implemented.

The ledger tracks confirmed incoming installments only. It does not model the seller's outgoing item, escrow, prices, reneging, counterparty trust, physical banking or enforcement of the bargain. Receiving an intent is not receiving an item. Both policies benefit from this same ledger, so its consistency is not a scoring win.

### 4. Resource and knowledge limits belong below personality

The policy stores contacts, character identity, outstanding balance, receipt IDs and its prior goal. It does not cache inventory. Every resource view belongs to the current character; dead snapshots containing held potions are rejected, and a retired character ID cannot be reused as a replacement in this toy lifecycle. This does not verify real-world death, character creation or persistence.

Known own storage can justify a banking intent. Unknown storage cannot silently become free capacity. Gift offers require held stock and do not create a daily reward promise or imply delivery. Another player's private inventory or enemy attack schedule is not represented in the view schema.

Unknown fields, missing fields, malformed quantities, stale observations, unconfirmed character changes, duplicate contacts and duplicate/excessive receipts produce explicit errors. A private-field injection was rejected through the executable's JSON input, not just by a unit-test helper. A malformed observation reports the scene, profile, policy and tick and emits no partial result.

Schema rejection is not a privacy proof. A bad future adapter could encode hidden information inside an allowed value. Annotated threat, value, contact visibility and recipient meaning still need a justified perception/evaluation boundary.

## Verification

- **17 Rust tests passed**, with Clippy warnings treated as errors.
- **Independent executable runs produced byte-identical traces.** This is offline deterministic replay, not a claim about the game server.
- **810 additional health/threat/held-stock combinations** checked emergency precedence and resource-backed offers across the three profiles.
- **1,200 generated context sequences** checked boundary switching, separate from the twelve named situations.
- Renaming every scenario, source and hypothesis did not change the goals. Those labels do not enter the policy.
- Malformed JSON fields and out-of-range observations exited nonzero through the actual CLI, with empty stdout and explicit errors.
- The trace preview's twelve situation selectors and all 34 frames were exercised in the browser at desktop and 390-pixel mobile widths. Every frame displayed all nine profile/policy outputs; mobile controls worked without page overflow. The preview is an inspection tool, not an in-game demonstration.

## Recommendation and remaining limits

Keep the small common lifecycle/resource/ledger boundary. Favor scoring with a modest switching margin for optional activities, while retaining fixed priorities as a comparison. Do not add a personality generator, random mistakes or free-form dialogue to explain what these few priorities already achieve.

The next useful independent experiment is **partial knowledge and the passage of time**: an unseen companion, a counterparty who never returns, a cancelled interaction, an aging item observation and an interruption that should or should not preserve a commitment. It should test deliberate deferral and abandonment without forgetting confirmed debts or inventing success. This is a recommendation, not approval to build another slice.

Before integration, replace the magical annotations with explainable inputs and evaluation based on current shared-game support. The experiment has no actual chat parser, gear/value estimator, observation adapter, skill model, route execution, group dynamics or population schedule. It tests coherent choices under supplied facts. Historical authenticity, movement/dodging, real social feel and production capacity remain unverified.
