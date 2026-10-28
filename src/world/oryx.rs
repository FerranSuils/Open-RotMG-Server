//! Oryx's voice in the Realm: taunts about his living guardians every twenty seconds, a line when
//! one falls, and a replacement event raised away from the players.
//! Reference (GPL-3.0; data/REALM-SRC-LICENSE): realm-src wServer/realm/Oryx.cs taunt table.
use super::*;
use crate::quest;

struct Taunt {
    /// Every kind that counts as one living guardian of this name.
    kinds: &'static [u16],
    spawn: &'static [&'static str],
    many: &'static [&'static str],
    last: &'static [&'static str],
    killed: &'static [&'static str],
}

const TAUNTS: &[Taunt] = &[
    Taunt { kinds: &[0x91b, 0x91c], spawn: &[], killed: &[],
        many: &["I am invincible while my {COUNT} Liches still stand!", "My {COUNT} Liches will feast on your essence!"],
        last: &["My final Lich shall consume your souls!", "My final Lich will protect me forever!"] },
    Taunt { kinds: &[0x91f, 0x920], spawn: &[], killed: &[],
        many: &["Mortal scum! My {COUNT} Ent Ancients will defend me forever!", "My forest of {COUNT} Ent Ancients is all the protection I need!"],
        last: &["My final Ent Ancient will destroy you all!", "My final Ent Ancient shall crush you!"] },
    Taunt { kinds: &[0x678], spawn: &[], killed: &[],
        many: &["My {COUNT} Oasis Giants will feast on your flesh!", "You have no hope against my {COUNT} Oasis Giants!"],
        last: &["A powerful Oasis Giant still fights for me!", "You will never defeat me while an Oasis Giant remains!"] },
    Taunt { kinds: &[0x675, 0x6c1, 0x6c2], spawn: &[], killed: &[],
        many: &["Maggots! My {COUNT} Phoenix Lords will burn you to ash!", "My {COUNT} Phoenix Lords will serve me forever!"],
        last: &["My final Phoenix Lord will never fall!", "My last Phoenix Lord will blacken your bones!"] },
    Taunt { kinds: &[0x928, 0x92d], spawn: &[], killed: &[],
        many: &["My {COUNT} Ghost Kings give me more than enough protection!", "Pathetic humans! My {COUNT} Ghost Kings shall destroy you utterly!"],
        last: &["A mighty Ghost King remains to guard me!", "My final Ghost King is untouchable!"] },
    Taunt { kinds: &[0x67d], spawn: &[], killed: &[],
        many: &["Cretins! I have {COUNT} Cyclops Gods to guard me!", "My {COUNT} powerful Cyclops Gods will smash you!"],
        last: &["My last Cyclops God will smash you to pieces!", "My final Cyclops God shall crush your puny skulls!"] },
    Taunt { kinds: &[0x668], spawn: &[], killed: &[],
        many: &["Fools! There is no escape from my {COUNT} Red Demons!", "My legion of {COUNT} Red Demons live only to serve me!"],
        last: &["My final Red Demon is unassailable!", "A Red Demon still guards me!"] },
    Taunt { kinds: &[0xd56], spawn: &["Your futile efforts are no match for a Skull Shrine!"],
        many: &["Insects! {COUNT} Skull Shrines still protect me", "You hairless apes will never overcome my {COUNT} Skull Shrines!", "You frail humans will never defeat my {COUNT} Skull Shrines!", "Miserable worms like you cannot stand against my {COUNT} Skull Shrines!", "Imbeciles! My {COUNT} Skull Shrines make me invincible!"],
        last: &["Pathetic fools! A Skull Shrine guards me!", "Miserable scum! My Skull Shrine is invincible!"],
        killed: &["You defaced a Skull Shrine! Minions, to arms!", "{PLAYER} razed one of my Skull Shrines -- I WILL HAVE MY REVENGE!", "{PLAYER}, you will rue the day you dared to defile my Skull Shrine!", "{PLAYER}, you contemptible pig! Ruining my Skull Shrine will be the last mistake you ever make!", "{PLAYER}, you insignificant cur! The penalty for destroying a Skull Shrine is death!"] },
    Taunt { kinds: &[0xd59], spawn: &["Your meager abilities cannot possibly challenge a Cube God!"],
        many: &["Filthy vermin! My {COUNT} Cube Gods will exterminate you!", "Loathsome slugs! My {COUNT} Cube Gods will defeat you!", "You piteous cretins! {COUNT} Cube Gods still guard me!", "Your pathetic rabble will never survive against my {COUNT} Cube Gods!", "You feeble creatures have no hope against my {COUNT} Cube Gods!"],
        last: &["Worthless mortals! A mighty Cube God defends me!", "Wretched mongrels! An unconquerable Cube God is my bulwark!"],
        killed: &["You have dispatched my Cube God, but you will never escape my Realm!", "{PLAYER}, you pathetic swine! How dare you assault my Cube God?", "{PLAYER}, you wretched dog! You killed my Cube God!", "{PLAYER}, you may have destroyed my Cube God but you will never defeat me!", "I have many more Cube Gods, {PLAYER}!"] },
    Taunt { kinds: &[0xd5f], spawn: &["Behold my Pentaract, and despair!"],
        many: &["Wretched creatures! {COUNT} Pentaracts remain!", "You detestable humans will never defeat my {COUNT} Pentaracts!", "My {COUNT} Pentaracts will protect me forever!", "Your weak efforts will never overcome my {COUNT} Pentaracts!", "Defiance is useless! My {COUNT} Pentaracts will crush you!"],
        last: &["I am invincible while my Pentaract stands!", "Ignorant fools! A Pentaract guards me still!"],
        killed: &["That was but one of many Pentaracts!", "You have razed my Pentaract, but you will die here in my Realm!", "{PLAYER}, you lowly scum! You'll regret that you ever touched my Pentaract!", "{PLAYER}, you flea-ridden animal! You destroyed my Pentaract!", "{PLAYER}, by destroying my Pentaract you have sealed your own doom!"] },
    Taunt { kinds: &[0xd54], spawn: &["At last, a Grand Sphinx will teach you to respect!"],
        many: &["You dull-spirited apes! You shall pose no challenge for {COUNT} Grand Sphinxes!", "Regret your choices, blasphemers! My {COUNT} Grand Sphinxes will teach you respect!", "My {COUNT} Grand Sphinxes protect my Chamber with their lives!"],
        last: &["A Grand Sphinx is more than a match for this rabble.", "You festering rat-catchers! A Grand Sphinx will make you doubt your purpose!", "Gaze upon the beauty of the Grand Sphinx and feel your last hopes drain away."],
        killed: &["The death of my Grand Sphinx shall be avenged!", "My Grand Sphinx, she was so beautiful. I will kill you myself, {PLAYER}!", "{PLAYER}, you up-jumped goat herder! You shall pay for defeating my Grand Sphinx!", "{PLAYER}, you foul ruffian! Do not think I forget your defiling of my Grand Sphinx!"] },
    Taunt { kinds: &[0xd50], spawn: &["Cower in fear of my Lord of the Lost Lands!", "My Lord of the Lost Lands will make short work of you!"],
        many: &["Cower before your destroyer! You stand no chance against {COUNT} Lords of the Lost Lands!", "Feel the awesome might of my {COUNT} Lords of the Lost Lands!", "Together, my {COUNT} Lords of the Lost Lands will squash you like a bug!", "Do not run! My {COUNT} Lords of the Lost Lands only wish to greet you!"],
        last: &["Give up now! You stand no chance against a Lord of the Lost Lands!", "Pathetic fools! My Lord of the Lost Lands will crush you all!"],
        killed: &["What trickery is this?! My Lord of the Lost Lands was invincible!", "You win this time, {PLAYER}, but mark my words: You will fall before the day is done.", "{PLAYER}, I will never forget you exploited my Lord of the Lost Lands' weakness!", "You got lucky this time {PLAYER}, but you stand no chance against me!"] },
    Taunt { kinds: &[crate::dungeon::trench::HERMIT], spawn: &["My Hermit God's thousand tentacles shall drag you to a watery grave!"],
        many: &["You will make a tasty snack for my Hermit Gods!", "I will enjoy watching my {COUNT} Hermit Gods fight over your corpse!"],
        last: &["You will be pulled to the bottom of the sea by my mighty Hermit God.", "Flee from my Hermit God, unless you desire a watery grave!", "My Hermit God will pull you beneath the waves!"],
        killed: &["This is preposterous! There is no way you could have defeated my Hermit God!", "You were lucky this time, {PLAYER}! You will rue this day that you killed my Hermit God!", "My Hermit God was more than you'll ever be, {PLAYER}. I will kill you myself!"] },
];

fn taunt_for(kind: u16) -> Option<&'static Taunt> {
    TAUNTS.iter().find(|t| t.kinds.contains(&kind))
}

impl World {
    fn oryx_says(&self, text: &str) {
        self.broadcast_text("Oryx the Mad God", text);
    }
    fn guardians(&self, taunt: &Taunt) -> usize {
        self.entities
            .values()
            .filter(|e| e.hp > 0 && taunt.kinds.contains(&e.kind))
            .count()
    }
    /// Every twenty seconds Oryx boasts about one kind of guardian still standing.
    pub(super) fn oryx_step(&mut self) {
        if self.name != "Realm" || self.now % 20_000 != 0 || self.players.is_empty() {
            return;
        }
        let standing: Vec<_> = TAUNTS.iter().map(|t| (t, self.guardians(t))).filter(|(_, n)| *n > 0).collect();
        let Some((taunt, count)) = standing.choose(&mut self.rng).copied() else {
            return;
        };
        let lines = if count == 1 { taunt.last } else { taunt.many };
        if let Some(line) = lines.choose(&mut self.rng) {
            self.oryx_says(&line.replace("{COUNT}", &count.to_string()));
        }
    }
    /// A fallen guardian earns a curse on its killer and a fresh event somewhere far from everyone.
    pub(super) fn oryx_guardian_fell(&mut self, defeated: &Entity) {
        let Some(taunt) = taunt_for(defeated.kind) else {
            return;
        };
        let killer = defeated
            .damage
            .iter()
            .max_by_key(|(_, n)| **n)
            .and_then(|(ch, _)| self.players.values().find(|p| p.character.id == *ch))
            .map(|p| p.character.name.clone());
        let lines: Vec<_> = taunt
            .killed
            .iter()
            .filter(|l| killer.is_some() || !l.contains("{PLAYER}"))
            .collect();
        if let Some(line) = lines.choose(&mut self.rng) {
            self.oryx_says(&line.replace("{PLAYER}", killer.as_deref().unwrap_or("")));
        }
        if self.name == "Realm"
            && let Err(error) = self.raise_event()
        {
            tracing::warn!(%error, "Oryx could not raise a replacement event");
        }
    }
    fn raise_event(&mut self) -> Result<()> {
        let living = self
            .entities
            .values()
            .filter(|e| e.hp > 0 && quest::EVENTS.contains(&e.kind) && e.quest == Some(e.id))
            .count();
        ensure!(living < 4, "the Realm already holds {living} events");
        let kind = *quest::EVENTS.choose(&mut self.rng).expect("events");
        let (width, height) = (self.map.width, self.map.height);
        let players: Vec<Pos> = self.players.values().map(|p| p.pos).collect();
        let quests: Vec<Pos> = self.entities.values().filter(|e| e.quest.is_some()).map(|e| e.pos).collect();
        let mut pos = None;
        for _ in 0..500 {
            let candidate = Pos {
                x: self.rng.gen_range(20..width - 20) as f32 + 0.5,
                y: self.rng.gen_range(20..height - 20) as f32 + 0.5,
            };
            let Some(cell) = self.map.cell(candidate.x, candidate.y) else {
                continue;
            };
            if (1..=7).contains(&cell.terrain)
                && cell.region == 0
                && (-2..=2).all(|dy| (-2..=2).all(|dx| self.walkable(Pos { x: candidate.x + dx as f32, y: candidate.y + dy as f32 })))
                && players.iter().all(|p| p.distance(candidate) >= 40.)
                && quests.iter().all(|q| q.distance(candidate) >= 32.)
            {
                pos = Some(candidate);
                break;
            }
        }
        let pos = pos.context("no open ground far enough from the players")?;
        let root = self.spawn_event_root(kind, pos)?;
        tracing::info!(entity = root, kind, x = pos.x, y = pos.y, "Oryx raised a replacement event");
        if let Some(line) = taunt_for(kind).and_then(|t| t.spawn.choose(&mut self.rng)) {
            self.oryx_says(line);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fallen_event_is_replaced_far_from_players_and_announced() {
        let mut d = super::super::tests::directory();
        let realm = *d.worlds.iter().find(|(_, w)| w.name == "Realm").unwrap().0;
        let w = d.worlds.get_mut(&realm).unwrap();
        let character = super::super::tests::ch(w);
        let id = w.enter(character, None).unwrap();
        let before = w
            .entities
            .values()
            .filter(|e| quest::EVENTS.contains(&e.kind) && e.quest == Some(e.id))
            .count();
        let shrine = w.entities.values().find(|e| e.kind == 0xd56).unwrap().clone();
        w.remove_entity(shrine.id);
        w.oryx_guardian_fell(&shrine);
        let roots: Vec<_> = w
            .entities
            .values()
            .filter(|e| quest::EVENTS.contains(&e.kind) && e.quest == Some(e.id))
            .collect();
        assert_eq!(roots.len(), before, "one event replaced the fallen shrine");
        let pos = w.players[&id].pos;
        assert!(roots.iter().all(|e| e.pos.distance(pos) >= 40. || e.id != roots.last().unwrap().id));
        assert_eq!(taunt_for(0xd5f).unwrap().spawn.len(), 1);
        assert!(taunt_for(0x91c).is_some() && taunt_for(0x500).is_none());
    }
}
