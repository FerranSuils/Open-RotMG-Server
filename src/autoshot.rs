//! Autoshot: the server fires the player's weapon at an enemy it picks, chosen with /autoshot.
//! This module is the pure part: settings, the chat grammar and the target choice. The shooting
//! itself lives in world/autoshot.rs.
use serde::{Deserialize, Serialize};

/// Which enemy gets picked among those in weapon range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// The closest enemy.
    #[default]
    Nearest,
    /// The strongest enemy among the close ones (see `Settings::radius`).
    Strongest,
    /// The weakest enemy among the close ones.
    Weakest,
}

/// What "strong" means.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stat {
    /// How much health the enemy has.
    #[default]
    Health,
    /// How hard the enemy hits.
    Damage,
}

pub const DEFAULT_RADIUS: u8 = 6;
pub const MAX_RADIUS: u8 = 15;

/// Saved with the character; whether autoshot is switched on is not (it starts off every login).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub mode: Mode,
    pub stat: Stat,
    /// Tiles that count as "close" for the strongest and weakest modes.
    pub radius: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::Nearest,
            stat: Stat::Health,
            radius: DEFAULT_RADIUS,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub id: i32,
    pub distance: f32,
    /// Maximum health of the enemy type.
    pub health: i32,
    /// Its hardest projectile hit.
    pub damage: i32,
}

/// Picks one of the candidates, which must already be in weapon range and have a clear shot.
///
/// Nearest takes the closest. Strongest and weakest only look at enemies within `radius` tiles
/// (the "close" ones), rank them by the chosen stat and break ties by distance; when nobody is
/// that close they fall back to the closest enemy in range.
pub fn choose(settings: &Settings, candidates: &[Candidate]) -> Option<i32> {
    let closest = |list: &mut dyn Iterator<Item = &Candidate>| {
        list.min_by(|a, b| a.distance.total_cmp(&b.distance))
            .map(|c| c.id)
    };
    if settings.mode == Mode::Nearest {
        return closest(&mut candidates.iter());
    }
    let radius = f32::from(settings.radius);
    let stat = |c: &Candidate| match settings.stat {
        Stat::Health => c.health,
        Stat::Damage => c.damage,
    };
    let close: Vec<&Candidate> = candidates.iter().filter(|c| c.distance <= radius).collect();
    if close.is_empty() {
        return closest(&mut candidates.iter());
    }
    close
        .into_iter()
        .min_by(|a, b| {
            let (x, y) = (stat(a), stat(b));
            let by_stat = if settings.mode == Mode::Strongest {
                y.cmp(&x)
            } else {
                x.cmp(&y)
            };
            by_stat.then(a.distance.total_cmp(&b.distance))
        })
        .map(|c| c.id)
}

pub const USAGE: &str = "Usage: /autoshot [on|off] [nearest|strongest|weakest] [health|damage] [radius 1-15]. \
Bare /autoshot toggles it; /autoshot status shows the settings.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parsed {
    pub enabled: bool,
    pub settings: Settings,
}

/// Reads what the player typed after /autoshot. Words combine freely: `on strongest damage radius 8`.
pub fn parse(current: Settings, enabled: bool, words: &str) -> Result<Parsed, String> {
    let mut out = Parsed {
        enabled,
        settings: current,
    };
    let words: Vec<String> = words.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        out.enabled = !enabled;
        return Ok(out);
    }
    let mut it = words.iter();
    while let Some(word) = it.next() {
        match word.as_str() {
            "on" | "start" => out.enabled = true,
            "off" | "stop" => out.enabled = false,
            "toggle" => out.enabled = !out.enabled,
            "status" | "show" | "help" | "?" => {}
            "nearest" | "near" | "closest" => out.settings.mode = Mode::Nearest,
            "strongest" | "strong" | "stronger" => out.settings.mode = Mode::Strongest,
            "weakest" | "weak" | "weaker" => out.settings.mode = Mode::Weakest,
            "health" | "hp" | "life" => out.settings.stat = Stat::Health,
            "damage" | "dmg" | "attack" => out.settings.stat = Stat::Damage,
            "radius" | "close" => {
                let n = it
                    .next()
                    .and_then(|n| n.parse::<u8>().ok())
                    .filter(|n| (1..=MAX_RADIUS).contains(n))
                    .ok_or_else(|| {
                        format!("The radius is a number of tiles from 1 to {MAX_RADIUS}.")
                    })?;
                out.settings.radius = n;
            }
            other => return Err(format!("Unknown autoshot option \"{other}\". {USAGE}")),
        }
    }
    Ok(out)
}

/// One chat line that says what autoshot will do.
pub fn describe(enabled: bool, s: &Settings) -> String {
    let stat = match s.stat {
        Stat::Health => "health",
        Stat::Damage => "damage",
    };
    let rule = match s.mode {
        Mode::Nearest => "the nearest enemy".to_string(),
        Mode::Strongest => format!(
            "the strongest enemy by {stat} within {} tiles (the nearest if none is that close)",
            s.radius
        ),
        Mode::Weakest => format!(
            "the weakest enemy by {stat} within {} tiles (the nearest if none is that close)",
            s.radius
        ),
    };
    format!(
        "Autoshot {}: aims at {rule}. Manual shooting still takes over.",
        if enabled { "ON" } else { "OFF" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(id: i32, distance: f32, health: i32, damage: i32) -> Candidate {
        Candidate {
            id,
            distance,
            health,
            damage,
        }
    }
    fn set(mode: Mode, stat: Stat, radius: u8) -> Settings {
        Settings { mode, stat, radius }
    }

    #[test]
    fn nearest_takes_the_closest_whatever_its_strength() {
        let list = [c(1, 5., 9000, 100), c(2, 2., 10, 1), c(3, 7., 50, 50)];
        assert_eq!(choose(&Settings::default(), &list), Some(2));
        assert_eq!(choose(&Settings::default(), &[]), None);
    }

    #[test]
    fn strongest_and_weakest_rank_the_close_enemies_by_the_chosen_stat() {
        let list = [
            c(1, 5., 9000, 5),
            c(2, 2., 300, 90),
            c(3, 7., 50_000, 400),
            c(4, 3., 10, 10),
        ];
        // within 6 tiles: ids 1, 2, 4. Id 3 is out of the circle however strong it is.
        assert_eq!(
            choose(&set(Mode::Strongest, Stat::Health, 6), &list),
            Some(1)
        );
        assert_eq!(
            choose(&set(Mode::Strongest, Stat::Damage, 6), &list),
            Some(2)
        );
        assert_eq!(choose(&set(Mode::Weakest, Stat::Health, 6), &list), Some(4));
        assert_eq!(choose(&set(Mode::Weakest, Stat::Damage, 6), &list), Some(1));
        // a wider circle reaches the big one
        assert_eq!(
            choose(&set(Mode::Strongest, Stat::Health, 8), &list),
            Some(3)
        );
        // a tiny circle degenerates into "nearest"
        assert_eq!(
            choose(&set(Mode::Strongest, Stat::Health, 2), &list),
            Some(2)
        );
    }

    #[test]
    fn ties_go_to_the_closer_one_and_an_empty_circle_falls_back_to_the_nearest() {
        let list = [c(1, 4., 100, 10), c(2, 3., 100, 10)];
        assert_eq!(
            choose(&set(Mode::Strongest, Stat::Health, 6), &list),
            Some(2)
        );
        assert_eq!(choose(&set(Mode::Weakest, Stat::Damage, 6), &list), Some(2));
        let far = [c(7, 9., 5, 5), c(8, 12., 9000, 900)];
        assert_eq!(
            choose(&set(Mode::Strongest, Stat::Health, 3), &far),
            Some(7)
        );
    }

    #[test]
    fn the_chat_grammar_combines_words_and_rejects_nonsense() {
        let base = Settings::default();
        let p = parse(base, false, "").unwrap();
        assert!(p.enabled && p.settings == base, "bare /autoshot toggles");
        assert!(!parse(base, true, "").unwrap().enabled);
        let p = parse(base, false, "ON Strongest damage radius 9").unwrap();
        assert!(p.enabled);
        assert_eq!(p.settings, set(Mode::Strongest, Stat::Damage, 9));
        let p = parse(p.settings, p.enabled, "weak hp off").unwrap();
        assert!(!p.enabled);
        assert_eq!(p.settings, set(Mode::Weakest, Stat::Health, 9));
        assert!(parse(base, false, "radius").is_err());
        assert!(parse(base, false, "radius 0").is_err());
        assert!(parse(base, false, "radius 16").is_err());
        assert!(parse(base, false, "radius lots").is_err());
        assert!(
            parse(base, false, "fastest")
                .unwrap_err()
                .contains("fastest")
        );
        let p = parse(base, true, "status").unwrap();
        assert!(p.enabled && p.settings == base, "status changes nothing");
    }

    #[test]
    fn the_description_names_the_rule() {
        let text = describe(true, &set(Mode::Strongest, Stat::Damage, 8));
        assert!(
            text.contains("ON")
                && text.contains("strongest")
                && text.contains("damage")
                && text.contains('8')
        );
        assert!(describe(false, &Settings::default()).contains("OFF"));
    }
}
