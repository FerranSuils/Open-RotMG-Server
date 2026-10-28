//! Fame augments in the style of Swarm: each fame milestone offers three random upgrades and the
//! player picks one with /augment. Upgrades stack in levels up to a per-augment maximum and die
//! with the character.
use crate::store::Character;
use anyhow::{Result, ensure};
use rand::{Rng, seq::SliceRandom};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    /// Adds `per_level` to effective stat `index` (HP, MP, ATT, DEF, SPD, DEX, VIT, WIS).
    Stat { index: usize, per_level: i32 },
    /// Arcane burst damaging every enemy around the player.
    Orb,
    /// Lightning to the nearest enemy that jumps to more enemies per level.
    Chain,
    /// Frost ring that slows nearby enemies.
    Frost,
    /// Healing pulse for the player and nearby allies.
    Blessing,
}
pub struct Augment {
    pub name: &'static str,
    pub effect: Effect,
    pub max: u8,
    pub text: &'static str,
}
const fn stat(name: &'static str, index: usize, per_level: i32, text: &'static str) -> Augment {
    Augment {
        name,
        effect: Effect::Stat { index, per_level },
        max: 5,
        text,
    }
}
pub const AUGMENTS: [Augment; 12] = [
    stat("Vitality", 0, 20, "+20 max HP"),
    stat("Essence", 1, 20, "+20 max MP"),
    stat("Fury", 2, 2, "+2 attack"),
    stat("Bulwark", 3, 2, "+2 defense"),
    stat("Celerity", 4, 2, "+2 speed"),
    stat("Dexterity", 5, 2, "+2 dexterity"),
    stat("Vigor", 6, 3, "+3 vitality"),
    stat("Wisdom", 7, 3, "+3 wisdom"),
    Augment {
        name: "Arcane Orb",
        effect: Effect::Orb,
        max: 5,
        text: "burst that damages nearby enemies every 1.5 s",
    },
    Augment {
        name: "Chain Lightning",
        effect: Effect::Chain,
        max: 5,
        text: "bolt to the nearest enemy that jumps to one more per level",
    },
    Augment {
        name: "Frost Nova",
        effect: Effect::Frost,
        max: 3,
        text: "slows nearby enemies every 4 s",
    },
    Augment {
        name: "Blessing",
        effect: Effect::Blessing,
        max: 3,
        text: "heals you and nearby allies 5% HP every 4 s",
    },
];
pub const OFFER: usize = 3;

/// Fame needed for the n-th pick (1-based): 20, 50, 90, 140, 200, ... growing each time.
pub fn milestone(n: u32) -> i64 {
    5 * i64::from(n) * (i64::from(n) + 3)
}
fn total_levels() -> u32 {
    AUGMENTS.iter().map(|a| u32::from(a.max)).sum()
}
/// Picks earned by `fame`, capped once every augment could be maxed.
pub fn picks(fame: i32) -> u32 {
    (1..=total_levels())
        .take_while(|n| milestone(*n) <= i64::from(fame))
        .count() as u32
}
pub fn level(ch: &Character, id: usize) -> u8 {
    ch.augments.get(id).copied().unwrap_or(0)
}
pub fn spent(ch: &Character) -> u32 {
    ch.augments.iter().map(|l| u32::from(*l)).sum()
}
pub fn pending(ch: &Character) -> u32 {
    picks(ch.fame).saturating_sub(spent(ch))
}
pub fn validate(ch: &Character) -> Result<()> {
    ensure!(
        ch.augments.len() <= AUGMENTS.len()
            && ch.augments.iter().zip(&AUGMENTS).all(|(l, a)| *l <= a.max)
            && spent(ch) <= picks(ch.fame),
        "invalid augment levels"
    );
    ensure!(
        ch.augment_offer.len() <= OFFER
            && ch.augment_offer.iter().all(|id| {
                usize::from(*id) < AUGMENTS.len()
                    && level(ch, usize::from(*id)) < AUGMENTS[usize::from(*id)].max
            })
            && ch
                .augment_offer
                .iter()
                .enumerate()
                .all(|(i, id)| !ch.augment_offer[..i].contains(id)),
        "invalid augment offer"
    );
    Ok(())
}
pub fn stat_bonus(ch: &Character) -> [i32; 8] {
    let mut bonus = [0; 8];
    for (id, augment) in AUGMENTS.iter().enumerate() {
        if let Effect::Stat { index, per_level } = augment.effect {
            bonus[index] += per_level * i32::from(level(ch, id));
        }
    }
    bonus
}
/// Applies a chosen offer slot, then deals a fresh offer while picks remain. Returns the augment
/// that gained a level, if any.
pub fn advance(
    ch: &mut Character,
    choice: Option<usize>,
    rng: &mut impl Rng,
) -> Result<Option<usize>> {
    let mut gained = None;
    if let Some(slot) = choice {
        ensure!(
            slot < ch.augment_offer.len(),
            "augment choice outside the current offer"
        );
        ensure!(pending(ch) > 0, "no augment pick is available");
        let id = usize::from(ch.augment_offer[slot]);
        if ch.augments.len() <= id {
            ch.augments.resize(id + 1, 0);
        }
        ch.augments[id] += 1;
        ch.augment_offer.clear();
        gained = Some(id);
    }
    if pending(ch) > 0 && ch.augment_offer.is_empty() {
        let mut open: Vec<u8> = (0..AUGMENTS.len())
            .filter(|id| level(ch, *id) < AUGMENTS[*id].max)
            .map(|id| id as u8)
            .collect();
        open.shuffle(rng);
        open.truncate(OFFER);
        ch.augment_offer = open;
    }
    validate(ch)?;
    Ok(gained)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn character(fame: i32) -> Character {
        let mut ch: Character = serde_json::from_value(serde_json::json!({
            "schema": 1, "class": 782, "base": [100, 100, 12, 0, 10, 15, 12, 12], "id": 1, "account": 1,
            "name": "", "level": 1, "exp": 0, "fame": 0, "hp": 100, "mp": 100, "items": vec![-1; 12],
            "revision": 0, "alive": true
        }))
        .unwrap();
        ch.fame = fame;
        ch
    }

    #[test]
    fn milestones_grow_and_picks_are_capped() {
        assert_eq!(picks(19), 0);
        assert_eq!(picks(20), 1);
        assert_eq!(picks(50), 2);
        assert_eq!(picks(90), 3);
        assert!(milestone(10) - milestone(9) > milestone(2) - milestone(1));
        assert_eq!(picks(i32::MAX), total_levels());
    }

    #[test]
    fn offers_are_dealt_chosen_and_never_exceed_earned_picks() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let mut ch = character(10);
        assert_eq!(advance(&mut ch, None, &mut rng).unwrap(), None);
        assert!(
            ch.augment_offer.is_empty(),
            "no offer before the first milestone"
        );
        ch.fame = 55;
        advance(&mut ch, None, &mut rng).unwrap();
        assert_eq!(ch.augment_offer.len(), OFFER);
        let first = ch.augment_offer.clone();
        advance(&mut ch, None, &mut rng).unwrap();
        assert_eq!(ch.augment_offer, first, "an open offer is never rerolled");
        let id = advance(&mut ch, Some(1), &mut rng).unwrap().unwrap();
        assert_eq!(id, usize::from(first[1]));
        assert_eq!(level(&ch, id), 1);
        assert_eq!(
            ch.augment_offer.len(),
            OFFER,
            "second pick is offered at once"
        );
        advance(&mut ch, Some(0), &mut rng).unwrap();
        assert!(ch.augment_offer.is_empty(), "both earned picks are spent");
        assert!(advance(&mut ch, Some(0), &mut rng).is_err());
        assert_eq!(spent(&ch), 2);
    }

    #[test]
    fn stat_augments_raise_effective_stats_and_maxed_augments_leave_offers() {
        let mut ch = character(i32::MAX);
        ch.augments = vec![2, 0, 5];
        assert_eq!(stat_bonus(&ch), [40, 0, 10, 0, 0, 0, 0, 0]);
        ch.augments = AUGMENTS.iter().map(|a| a.max).collect();
        ch.augments[11] -= 1;
        let mut rng = rand::rngs::StdRng::seed_from_u64(1);
        advance(&mut ch, None, &mut rng).unwrap();
        assert_eq!(
            ch.augment_offer,
            vec![11],
            "only unmaxed augments are offered"
        );
        ch.augments[0] = 6;
        assert!(validate(&ch).is_err());
    }
}
