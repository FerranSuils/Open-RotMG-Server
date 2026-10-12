# Bot research: sample 1

Follow-up findings are in `sample-2.md`; limitations below describe this initial pass, not the entire accumulated research.

## What was examined

Three historical videos, with metadata checked and local video bytes retained. Reviewed coarse overviews, then 164 sampled frames across nine selected windows spanning 328 seconds, plus native-resolution confirmation frames for quotations/build/death details. This was visual/chat analysis, not a full continuous viewing or an audio analysis.

| Source | Upload date | Reviewed windows | Version evidence |
| --- | --- | --- | --- |
| Getting to level 20 in Realm of the Mad God | 2012-03-23 | 0:20–0:44; 2:50–3:20; 4:50–5:20; 6:50–7:20; 8:10–8:40; 15:50–16:20 | Title and connection text visibly say 122.0.1; older than our target |
| RotMG - The new, MAD LAB! | 2012-09-16 | 0:00–0:40; 3:00–4:18 | No exact build label verified |
| Realm of the Mad God - Oryx gets owned! | 2012-10-03 | 0:00–0:36 | No exact build label verified |

https://www.youtube.com/watch?v=G08RmJiBdl8

https://www.youtube.com/watch?v=xyUiQewDLg8

https://www.youtube.com/watch?v=H44UToO5faY

Contact sheets have nominal two-second-bin-center labels, not exact event timestamps. Windows above are the appropriate citation granularity. Native confirmation frames use explicit source seeks. Repeated persistent chat lines are not counted as new messages. The first window includes character creation, brief Nexus passage and Vault preparation, not a sustained Nexus trading session.

Downloaded footage, extracted frames and sampling records remain private and outside this repository. Reviewable source URLs and windows are listed above. A fourth candidate returned HTTP 403 and was excluded, not counted as inspected.

## Learnings

### 1. A train is a loose crowd, not synchronized formation

**Observed:** The leveling video’s road groups change shape, spacing and visible membership. In 2:50–3:20 and 6:50–7:20, surrounding players occupy different positions and fire in different directions. The recorder trails the crowd at points rather than remaining in a fixed squad slot.

**Implication:** Shared opportunities can bring independent players together. Prefer loose attraction to activity, healing and other players over identical routes or formation marching. This does not establish friendships or a designated leader.

### 2. The game’s events create much of the social rhythm

**Observed:** The leveling sample changes into Oryx’s Castle during 8:10–8:40, accompanied by player reactions such as "woot" and gear requests. In the short Oryx video, the boss dies, Wine Cellar unlock text appears, and the visible crowd/minimap population rapidly shrinks while a smaller set stays near loot and the entrance.

**Implication:** Population should gather and disperse with real game events, not simply remain in one busy spot. The short Oryx sequence suggests many players take the next opportunity; it does not identify every individual’s exit route.

### 3. Short, useful chat is a large part of the atmosphere

**Observed:** Mad Lab begins with a nearby player saying "HP" and another message explaining "go to the blue pool". The leveling video shows "hp", "hp pot" and "ty" in chat around 4:50–5:20. The Oryx opening contains "hp", "hpa" and "ouch" while fighting and visible healing effects continue.

**Implication:** Context-sensitive requests, advice and acknowledgments deserve priority over elaborate dialogue. Casual banter also exists after the Mad Lab fight, including "jk" and "lozls". Not all chat is a task, and proximity does not prove that a particular heal was a response to a request.

### 4. Equipment needs intrude into ongoing adventures

**Observed:** Castle arrival includes "anyone got a bow they dont need?". Mad Lab’s aftermath includes "tradeing dhide for staff" and "can i buy your staff?". The Oryx aftermath contains "selling inc and gsorc" and "need good staff plz". The Mad Lab recorder inspects Roc Leather Armor with a native tooltip explicitly saying it is not usable by Wizard.

**Implication:** Bots’ requests and offers should follow their actual inventory and class needs. Trading/social behavior is not confined to Nexus. These are observed requests/offers, not verified completed trades. "free staff someone?" is ambiguous and should not automatically be labeled an offer rather than a request.

### 5. Imperfect play is not just occasional random failure

**Observed:** The video titled Getting to level 20 ends with the recorder’s level-16 Assassin dying to Oryx. The death screen shows 3,584 shots and 11% accuracy. Other players also die during the sampled road/boss scenes. Some Oryx participants keep firing after the main boss dies; players linger, inspect loot or talk rather than instantly performing the optimal next action.

**Implication:** Aim attention, excessive firing, delayed decisions and concrete competing interests can produce plausible inefficiency. Do not optimize every bot for perfect survival/DPS. The 11% value describes one character run, not a population target or the recorder’s overall skill.

### 6. Level, visible appearance and underlying experience are different evidence

**Observed:** The leveling recorder starts a level-one character on an account showing 11 stars and a prior death list. Mad Lab includes a Warrior identified by the native hover panel even though appearances during the dungeon are not a dependable simple class proxy. Its blue-pool advice also explicitly concerns a visual/status effect.

**Implication:** Do not label every low-level character a novice or classify everyone solely from an avatar’s appearance. Keep observed facts separate from inferred experience, equipment and personality. An account star count is not a calibrated skill rating.

## Written context, with limits

A March 2012 article describes trains as players following one another and Nexus as a meeting/trade hub. Its explanations of mechanics are not authoritative:

https://www.engadget.com/2012-03-25-rise-and-shiny-realm-of-the-mad-god.html

Two posts dated April 2012 discuss trains, healing, teleporting, character preservation and different reasons for farming. They support questions to investigate, not exact 123.5.1 contracts. The fame article explicitly says it was edited and includes later material such as ninja and removed resurrection amulets:

https://anticosmic.blogspot.com/2012/04/fame-farming.html

https://anticosmic.blogspot.com/2012/04/rotmg-survival-guide.html

## What this changes in the proposed build

Prioritize a loosely cooperating crowd before sophisticated individual personalities: distinct positioning/retreat decisions, group attraction, short requests that affect actions, and event-driven gathering/departure. Healing, shared XP, normal teleporting and real world/portal transitions are important shared-game dependencies, not decorative bot features. Three Wizards can prove controller plumbing, but cannot alone demonstrate the social role of healers seen here.

Useful candidate acceptance scenes are: a straggler rejoins a moving crowd without synchronized marching; a health request is handled by a capable nearby character when appropriate; an unwanted item creates a real request/offer; and a boss aftermath splits the crowd into departures, looters and lingerers. These are research-derived proposals, not implemented guarantees.

Still missing: completed trade negotiation, long-term friendship/reputation, reliable novice/veteran distributions, longitudinal login/logout patterns and measured reaction times. Two of three videos are dungeon/boss-focused, and none is authenticated exact 123.5.1 footage. Next research should specifically fill those gaps rather than collect another pile of interchangeable boss clips.
