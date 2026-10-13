use crate::content::Content;
use anyhow::{Result, ensure};
use rand::{Rng, seq::SliceRandom};
#[derive(Clone, Copy)]
pub(crate) enum Pick {
    Item(u16),
    Tier(u8, &'static [i32]),
}
pub(crate) const WEAPONS: &[i32] = &[1, 2, 3, 8, 17];
pub(crate) const ABILITIES: &[i32] = &[4, 5, 11, 12, 13, 15, 16, 18, 19, 20, 21, 22, 23];
pub(crate) const ARMOR: &[i32] = &[6, 7, 14];
pub(crate) const RINGS: &[i32] = &[9];
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
        0x172 | 0x173 | 0x175..=0x179 | 0x653 | 0xd07..=0xd0a | 0xda2 | 0xda3 | 0x918 | 0x919 => {
            &[]
        }
        0xdaa..=0xdaf => &[(0.25, Item(0xa22)), (0.25, Item(0xa23))],
        0x605..=0x607
        | 0x60a
        | 0x60c
        | 0x615
        | 0x616
        | 0x61c..=0x61e
        | 0x624..=0x626
        | 0x63a..=0x63c
        | 0x63f
        | 0x642..=0x644
        | 0x64a..=0x64d
        | 0xd91..=0xd94
        | 0xd96
        | 0xd97
        | 0xd9b..=0xd9f
        | 0xda4..=0xda6
        | 0xdb0..=0xdb2 => &[(0.05, Item(0xa22)), (0.03, Item(0xa23))],
        0x609 | 0x617 | 0x60d => &[
            (0.2, Tier(2, WEAPONS)),
            (0.2, Tier(2, ARMOR)),
            (0.2, Tier(1, RINGS)),
            (0.2, Tier(1, ABILITIES)),
            (0.03, Item(0xa22)),
            (0.02, Item(0xa23)),
        ],
        0x61f | 0x632 | 0x633 | 0x63e => &[
            (0.1, Tier(3, WEAPONS)),
            (0.05, Tier(4, WEAPONS)),
            (0.1, Tier(3, ARMOR)),
            (0.05, Tier(4, ARMOR)),
            (0.05, Tier(1, RINGS)),
            (0.05, Tier(1, ABILITIES)),
        ],
        0x640 | 0x645 | 0x646 | 0x64f | 0x650 => &[
            (0.16, Tier(5, WEAPONS)),
            (0.08, Tier(6, WEAPONS)),
            (0.16, Tier(4, ARMOR)),
            (0.08, Tier(5, ARMOR)),
            (0.04, Tier(6, ARMOR)),
            (0.05, Tier(3, RINGS)),
            (0.03, Tier(3, ABILITIES)),
        ],
        0x651..=0x659 | 0xd05 => &[
            (0.02, Tier(6, WEAPONS)),
            (0.01, Tier(7, WEAPONS)),
            (0.005, Tier(8, WEAPONS)),
            (0.02, Tier(6, ARMOR)),
            (0.01, Tier(7, ARMOR)),
            (0.005, Tier(8, ARMOR)),
            (0.015, Tier(3, RINGS)),
            (0.02, Tier(4, ABILITIES)),
        ],
        0x223..=0x229 => &[
            (0.1, Item(0xa3f)),
            (0.05, Tier(3, RINGS)),
            (0.05, Tier(6, WEAPONS)),
        ],
        0xd00..=0xd04 => &[(0.02, Tier(5, WEAPONS)), (0.05, Item(0xa23))],
        0xd95 | 0xd98..=0xd9a => &[
            (0.2, Tier(5, ARMOR)),
            (0.2, Tier(6, WEAPONS)),
            (0.1, Tier(7, WEAPONS)),
            (0.1, Tier(6, ARMOR)),
            (0.1, Tier(3, ABILITIES)),
            (0.1, Tier(3, RINGS)),
        ],
        0x917 => &[
            (1., Item(0xa21)),
            (0.05, Item(crate::endgame::INCANTATION)),
            (0.05, Item(0xc04)),
            (0.05, Item(0xa3e)),
            (0.05, Item(0xa40)),
            (0.05, Item(0xa41)),
            (0.2, Tier(9, WEAPONS)),
            (0.3, Tier(8, ARMOR)),
        ],
        0xd06 => &[
            (0.3, Item(0xa4c)),
            (0.01, Item(0xa4b)),
            (0.01, Item(0xc03)),
            (0.01, Item(0xa5a)),
            (0.15, Tier(7, ARMOR)),
            (0.124, Tier(4, ABILITIES)),
            (0.11, Tier(3, RINGS)),
        ],
        0xd90 => &[
            (1., Item(0xa35)),
            (0.005, Item(crate::endgame::INCANTATION)),
            (0.009, Item(0xc02)),
            (0.3, Tier(7, WEAPONS)),
            (0.2, Tier(8, WEAPONS)),
            (0.2, Tier(4, ABILITIES)),
            (0.3, Tier(3, RINGS)),
        ],
        kind if crate::endgame::ROSTER.contains(&kind) => crate::endgame::loot(kind),
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
pub fn rewards(content: &Content, kind: u16, rng: &mut impl Rng) -> Vec<u16> {
    if matches!(
        kind,
        0x917 | 0xd06 | 0xd90 | 0xd78 | 0xd79 | crate::endgame::ORYX | crate::endgame::ORYX2
    ) {
        table(kind)
            .iter()
            .filter_map(|(chance, pick)| {
                rng.gen_bool(*chance).then(|| {
                    *candidates(content, *pick)
                        .choose(rng)
                        .expect("validated loot candidates")
                })
            })
            .collect()
    } else {
        roll(content, kind, rng).into_iter().collect()
    }
}
pub fn private(kind: u16) -> bool {
    matches!(kind, 0x683..=0x686 | 0x927 | 0x640 | 0x645 | 0x646 | 0x64f | 0x650..=0x659
        | 0xd05 | 0x917 | 0xd06 | 0xd90 | 0xd95 | 0xd98..=0xd9a
        | 0x668 | 0x67d | 0xd78 | 0xd79 | 0xd7f | crate::endgame::ORYX | crate::endgame::ORYX2)
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
