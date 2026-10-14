use crate::content::Content;
use anyhow::{Result, ensure};
use rand::{Rng, seq::SliceRandom};
#[derive(Clone, Copy)]
enum Pick {
    Item(u16),
    Tier(u8, &'static [i32]),
}
const WEAPONS: &[i32] = &[1, 2, 3, 8, 17];
const ABILITIES: &[i32] = &[4, 5, 11, 12, 13, 15, 16, 18, 19, 20, 21, 22, 23];
const ARMOR: &[i32] = &[6, 7, 14];
const RINGS: &[i32] = &[9];
fn table(kind: u16) -> &'static [(f64, Pick)] {
    use Pick::{Item, Tier};
    match kind {
        0x600 => &[(0.2, Tier(1, WEAPONS)), (0.03, Item(0xa22))],
        0x60e..=0x610 => &[(0.03, Item(0xa22)), (0.02, Item(0xa23))],
        0x687..=0x689 => &[(0.01, Item(0xa22))],
        0x68a..=0x68e => &[(0.1, Tier(1, ABILITIES))],
        0x68f => &[(0.1, Tier(1, WEAPONS))],
        0x683..=0x686 => &[
            (0.3, Tier(1, ARMOR)),
            (0.3, Tier(1, RINGS)),
            (0.2, Tier(2, WEAPONS)),
            (0.1, Tier(3, WEAPONS)),
            (0.01, Item(0xa50)),
        ],
        0x927 => &[
            (0.01, Item(0xa50)),
            (0.15, Tier(1, ARMOR)),
            (0.5, Tier(1, RINGS)),
            (0.2, Tier(2, ARMOR)),
            (0.2, Tier(2, WEAPONS)),
            (0.25, Tier(3, ARMOR)),
            (0.3, Tier(3, WEAPONS)),
            (0.3, Tier(4, ARMOR)),
            (0.4, Tier(4, WEAPONS)),
        ],
        _ => panic!("loot requested without encounter profile"),
    }
}
fn candidates(content: &Content, pick: Pick) -> Vec<u16> {
    match pick {
        Pick::Item(kind) => vec![kind],
        Pick::Tier(tier, slots) => content
            .objects
            .values()
            .filter(|o| o.tier == Some(tier) && slots.contains(&o.slot_type))
            .map(|o| o.kind)
            .collect(),
    }
}
pub fn collectibles(content: &Content) -> std::collections::BTreeSet<u16> {
    [0x600, 0x60e, 0x687, 0x68a, 0x68f, 0x683, 0x927]
        .into_iter()
        .flat_map(|kind| {
            table(kind)
                .iter()
                .flat_map(|(_, pick)| candidates(content, *pick))
        })
        .collect()
}
pub fn validate(content: &Content, kind: u16) -> Result<()> {
    for (_, pick) in table(kind) {
        let items = candidates(content, *pick);
        ensure!(
            !items.is_empty(),
            "encounter {kind:#x} has an empty selected-XML loot tier"
        );
        for item in items {
            let o = content.get(item)?;
            ensure!(
                o.class == "Equipment" && o.bag_type <= 4,
                "invalid encounter loot {item:#x}"
            );
        }
    }
    Ok(())
}
pub fn roll(content: &Content, kind: u16, rng: &mut impl Rng) -> Option<u16> {
    for (chance, pick) in table(kind) {
        if rng.gen_bool(*chance) {
            return Some(
                *candidates(content, *pick)
                    .choose(rng)
                    .expect("validated nonempty loot candidates"),
            );
        }
    }
    None
}
pub fn private(kind: u16) -> bool {
    matches!(kind, 0x683..=0x686 | 0x927)
}
#[cfg(test)]
mod tests {
    use super::*;
    use rand::{SeedableRng, rngs::StdRng};
    #[test]
    fn every_candidate_loot_tier_resolves_and_rolls_selected_items() {
        let content = Content::load(std::path::Path::new("data")).unwrap();
        let mut rng = StdRng::seed_from_u64(17);
        for kind in [
            0x600, 0x60e, 0x60f, 0x610, 0x683, 0x684, 0x685, 0x686, 0x687, 0x688, 0x689, 0x68a,
            0x68b, 0x68c, 0x68d, 0x68e, 0x68f, 0x927,
        ] {
            validate(&content, kind).unwrap();
            let mut dropped = false;
            for _ in 0..1000 {
                if let Some(item) = roll(&content, kind, &mut rng) {
                    dropped = true;
                    assert_eq!(content.get(item).unwrap().class, "Equipment");
                }
            }
            assert!(dropped);
        }
        assert!(!private(0x600));
        assert!(private(0x927));
    }
}
