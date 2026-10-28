//! Pets (pixel-art/inject.py): every species comes in several cosmetic skins, follows its owner
//! and has one modest ability. Chosen with /pet and saved as the character's pet_kind.
mod table;
pub use table::PETS;

/// What a species does for its owner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ability {
    /// Adds to one effective stat (index into [hp, mp, att, def, spd, dex, vit, wis]).
    Stat(usize, i32),
    /// Restores this much HP every PULSE_MS.
    Heal(i32),
    /// Restores this much MP every PULSE_MS.
    Mana(i32),
    /// Breathes a Fire Wand bolt at the nearest enemy every PULSE_MS.
    Fireball,
    /// Extra experience from kills, in percent.
    Experience(i32),
}
pub const PULSE_MS: u64 = 3000;
const SPECIES: [(&str, Ability, &str); 8] = [
    ("Cat", Ability::Stat(5, 4), "+4 DEX"),
    ("Puppy", Ability::Heal(20), "heals you 20 HP every 3 s"),
    ("Slime", Ability::Stat(0, 40), "+40 max HP"),
    (
        "Dragon",
        Ability::Fireball,
        "breathes fire at the nearest enemy every 3 s",
    ),
    ("Owl", Ability::Mana(15), "restores 15 MP every 3 s"),
    ("Bunny", Ability::Stat(4, 4), "+4 SPD"),
    ("Fox", Ability::Experience(10), "+10% experience"),
    ("Turtle", Ability::Stat(3, 4), "+4 DEF"),
];

/// The species ability of a pet and its description; skins share it.
pub fn ability(kind: Option<u16>) -> Option<(Ability, &'static str)> {
    let species = name(kind?)?.rsplit(' ').next()?;
    SPECIES
        .iter()
        .find(|(s, ..)| *s == species)
        .map(|(_, a, text)| (*a, *text))
}
/// Kill experience after the owner's pet bonus.
pub fn experience(kind: Option<u16>, amount: i32) -> i32 {
    match ability(kind) {
        Some((Ability::Experience(percent), _)) => amount + amount * percent / 100,
        _ => amount,
    }
}

pub fn is_pet(kind: u16) -> bool {
    PETS.iter().any(|(k, _)| *k == kind)
}
pub fn name(kind: u16) -> Option<&'static str> {
    PETS.iter().find(|(k, _)| *k == kind).map(|(_, n)| *n)
}
/// Looks a pet up by list number (1-based) or by name, ignoring case.
pub fn find(query: &str) -> Option<u16> {
    let query = query.trim();
    if let Ok(n) = query.parse::<usize>() {
        return PETS.get(n.checked_sub(1)?).map(|(k, _)| *k);
    }
    PETS.iter()
        .find(|(_, name)| name.eq_ignore_ascii_case(query))
        .map(|(k, _)| *k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pets_are_found_by_number_or_name_and_are_harmless_characters() {
        assert_eq!(find("1"), Some(PETS[0].0));
        assert_eq!(
            find("shadow turtle"),
            PETS.iter().find(|p| p.1 == "Shadow Turtle").map(|p| p.0)
        );
        assert_eq!(find("0"), None);
        assert_eq!(find("dragonfly"), None);
        assert!(
            PETS.iter().all(|(k, _)| ability(Some(*k)).is_some()),
            "every species has an ability"
        );
        assert_eq!(ability(find("gilded fox")), ability(find("classic fox")));
        assert_eq!(experience(find("shadow fox"), 50), 55);
        assert_eq!(experience(find("shadow cat"), 50), 50);
        assert_eq!(experience(None, 50), 50);
        let content = crate::content::Content::load(std::path::Path::new("data")).unwrap();
        for (kind, name) in PETS {
            let o = content.get(kind).unwrap();
            assert!(
                o.class == "Character" && !o.enemy && o.projectiles.is_empty(),
                "{name}"
            );
            content.permanent_companion(kind).unwrap();
        }
    }
}
