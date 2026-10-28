//! Prestige from the live fame standings: a rank insignia beside every ranked character, an aura
//! that grows with character fame, and a floating crown on the server's best player that shines
//! brighter the longer the reign lasts. All visuals have a hard cap.
use std::collections::BTreeMap;

mod table;
pub use table::{CROWNS, RANKS};

pub struct Rank {
    pub name: &'static str,
    /// Last standings position (1-based) covered by this rank.
    pub last: u32,
    pub insignia: u16,
    pub color: u32,
}

/// Live standings shared with every world.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct Standings {
    /// Account to 1-based position by living fame.
    pub positions: BTreeMap<i32, u32>,
    pub leader: Option<i32>,
    /// Unix seconds when the current leader took first place.
    pub reign_since: u64,
}

pub fn rank(position: u32) -> usize {
    RANKS
        .iter()
        .position(|r| position <= r.last)
        .expect("the last rank is unbounded")
}

pub struct Aura {
    pub radius: f32,
    pub color: u32,
    pub every_ms: u64,
}
const AURA_FAME: [i32; 7] = [50, 200, 500, 1000, 2000, 4000, 8000];
const AURA_COLORS: [u32; 7] = [
    0x8a8f99, 0xc07a3a, 0xd8dde6, 0xf0c43c, 0x3cc86e, 0x3c78e6, 0xdc3246,
];
/// None below the first threshold; the seventh tier is the cap.
pub fn aura(fame: i32) -> Option<Aura> {
    let tier = AURA_FAME.iter().take_while(|t| fame >= **t).count();
    (tier > 0).then(|| Aura {
        radius: 0.8 + 0.25 * tier as f32,
        color: AURA_COLORS[tier - 1],
        every_ms: 1600 - 150 * tier as u64,
    })
}

const REIGN_SECS: [u64; 6] = [0, 300, 900, 1800, 3600, 7200];
/// Crown level 0..=5 from continuous seconds at first place; capped after two hours.
pub fn crown_level(reign_secs: u64) -> usize {
    REIGN_SECS.iter().take_while(|t| reign_secs >= **t).count() - 1
}
/// Golden sparkle cadence around the crown: faster each level.
pub fn sparkle_every_ms(level: usize) -> u64 {
    2000 - 300 * level as u64
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Period {
    Week,
    Month,
    Year,
}
/// Start of the current calendar period in UTC (weeks begin on Monday).
pub fn period_start(period: Period, now: u64) -> u64 {
    let days = now / 86_400;
    let start_day = match period {
        Period::Week => days - (days + 3) % 7,
        Period::Month | Period::Year => {
            let (y, m, _) = civil(days as i64);
            days_from_civil(y, if period == Period::Year { 1 } else { m }, 1) as u64
        }
    };
    start_day * 86_400
}
// Howard Hinnant's civil calendar conversions.
fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (i64::from(m) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Positions by living fame (ties keep the older account first). The crown needs some fame.
pub fn standings(scores: &[(i32, i64)], previous: &Standings, now: u64) -> Standings {
    let mut order = scores.to_vec();
    order.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let leader = order.first().filter(|(_, fame)| *fame > 0).map(|(a, _)| *a);
    Standings {
        positions: order
            .iter()
            .enumerate()
            .map(|(i, (account, _))| (*account, i as u32 + 1))
            .collect(),
        leader,
        reign_since: if leader.is_some() && leader == previous.leader {
            previous.reign_since
        } else {
            now
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_cover_every_position_with_tight_bands() {
        assert_eq!(RANKS[rank(1)].last, 1);
        assert_eq!(rank(7), rank(8));
        assert_ne!(rank(8), rank(9));
        assert_eq!(rank(u32::MAX >> 1), RANKS.len() - 1);
        for pair in RANKS.windows(2) {
            let band = pair[1].last - pair[0].last;
            assert!(pair[1].last == i32::MAX as u32 || (1..=14).contains(&band));
        }
        let mut seen = std::collections::BTreeSet::new();
        assert!(RANKS.iter().all(|r| seen.insert(r.insignia)));
    }

    #[test]
    fn aura_and_crown_grow_with_fame_and_reign_up_to_a_cap() {
        assert!(aura(49).is_none());
        let radii: Vec<f32> = [50, 200, 500, 1000, 2000, 4000, 8000, 1_000_000]
            .iter()
            .map(|f| aura(*f).unwrap().radius)
            .collect();
        assert!(radii.windows(2).take(6).all(|w| w[1] > w[0]));
        assert_eq!(radii[6], radii[7], "aura is capped");
        assert_eq!(crown_level(0), 0);
        assert_eq!(crown_level(299), 0);
        assert_eq!(crown_level(300), 1);
        assert_eq!(crown_level(7200), 5);
        assert_eq!(crown_level(u64::MAX), CROWNS.len() - 1, "crown is capped");
        assert!(sparkle_every_ms(5) < sparkle_every_ms(0));
    }

    #[test]
    fn calendar_periods_start_in_utc() {
        // Wednesday 2026-10-21 12:00 UTC.
        let now = 1_792_584_000;
        assert_eq!(civil((now / 86_400) as i64), (2026, 10, 21));
        assert_eq!(
            civil((period_start(Period::Week, now) / 86_400) as i64),
            (2026, 10, 19)
        );
        assert_eq!(
            civil((period_start(Period::Month, now) / 86_400) as i64),
            (2026, 10, 1)
        );
        assert_eq!(
            civil((period_start(Period::Year, now) / 86_400) as i64),
            (2026, 1, 1)
        );
        assert_eq!(period_start(Period::Week, now) % 86_400, 0);
    }

    #[test]
    fn standings_keep_reign_until_the_leader_changes() {
        let first = standings(&[(7, 10), (3, 50), (9, 50)], &Standings::default(), 100);
        assert_eq!(first.positions[&3], 1);
        assert_eq!(first.positions[&9], 2);
        assert_eq!(first.positions[&7], 3);
        assert_eq!((first.leader, first.reign_since), (Some(3), 100));
        let held = standings(&[(3, 60), (9, 55)], &first, 400);
        assert_eq!(held.reign_since, 100);
        let lost = standings(&[(3, 60), (9, 70)], &held, 500);
        assert_eq!((lost.leader, lost.reign_since), (Some(9), 500));
        assert_eq!(standings(&[(1, 0)], &lost, 600).leader, None);
    }
}
