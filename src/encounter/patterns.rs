//! Bullet-hell pattern engine for custom bosses (current and future). A boss is a cycle of named
//! phases; each phase mixes space-control patterns that ignore the player (spirals, rings, waves,
//! crosses, flowers) with tracking attacks that read the player's position (aimed fans and AoE
//! strikes that paint a warning circle where the player stands). Below half health the boss
//! enrages and every pattern fires faster.
//!
//! Projectile indices are the five bullets every custom boss carries (pixel-art/inject.py):
//! 0 bolt (fast), 1 ring bullet, 2 shuriken (curving), 3 big orb (slow), 4 crescent (very fast).
use super::attacks::{Attack, Schedule};

mod table;
pub use table::BOSS_KITS;

pub const BOLT: u8 = 0;
pub const RING: u8 = 1;
pub const SHURIKEN: u8 = 2;
pub const ORB: u8 = 3;
pub const CRESCENT: u8 = 4;

#[derive(Clone, Copy, Debug)]
pub enum Pattern {
    /// `arms` bullets rotating `spin` degrees per second, fired every `every` ms.
    Spiral {
        projectile: u8,
        arms: usize,
        spin: f32,
        every: u64,
    },
    /// Full ring that turns `turn` degrees between volleys.
    Ring {
        projectile: u8,
        count: usize,
        turn: f32,
        every: u64,
    },
    /// `rings` rings `gap` ms apart: an expanding wave.
    Wave {
        projectile: u8,
        count: usize,
        rings: usize,
        gap: u64,
        every: u64,
    },
    /// `arms` rays of `width` bullets each (crossed bursts), rotating `spin` degrees per second.
    Cross {
        projectile: u8,
        arms: usize,
        width: usize,
        spin: f32,
        every: u64,
    },
    /// Two interleaved rings half a step apart: petals.
    Flower {
        projectile: u8,
        petals: usize,
        every: u64,
    },
    /// Fan aimed at the player when it fires; `lead` predicts the player's movement.
    Aimed {
        projectile: u8,
        count: usize,
        gap: f32,
        every: u64,
        lead: bool,
    },
    /// Warning circle at the player's position that explodes 1.5 s later.
    Strike {
        radius: f32,
        damage: i32,
        every: u64,
    },
    /// Ring of `count` slots with `gap` slots missing: the gap is the only way through, and it
    /// drifts `drift` degrees per second so the player has to follow it.
    Gate {
        projectile: u8,
        count: usize,
        gap: usize,
        drift: f32,
        every: u64,
    },
    /// `arms` bullets whose angle swings `swing` degrees back and forth every `period` ms.
    Pendulum {
        projectile: u8,
        arms: usize,
        swing: f32,
        period: u64,
        every: u64,
    },
    /// Machine-gun burst: `shots` single bullets `gap` ms apart at the player; `lead` aims
    /// where the player is going.
    Stream {
        projectile: u8,
        shots: usize,
        gap: u64,
        every: u64,
        lead: bool,
    },
    /// Meteor shower: `count` warning circles scattered up to `scatter` tiles around the player.
    Rain {
        radius: f32,
        damage: i32,
        count: usize,
        scatter: f32,
        every: u64,
    },
}
/// What happens to the boss itself while a phase runs (src/world/bosses.rs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    None,
    /// Immune to damage for the whole phase: dodge until it ends.
    Shield,
    /// Calls minions (an object id from objects.xml) around itself, up to `count` alive.
    Summon {
        minion: &'static str,
        count: usize,
    },
    /// Teleports next to the nearest player when the phase begins.
    Blink,
    /// Recovers `percent` of its maximum health when the phase begins.
    Mend {
        percent: i32,
    },
}
pub struct Phase {
    pub name: &'static str,
    pub ms: u64,
    pub effect: Effect,
    pub patterns: &'static [Pattern],
}

const AIM_RANGE: f32 = 16.;

fn round50(ms: u64) -> u64 {
    (ms / 50).max(1) * 50
}
fn attacks(pattern: Pattern, age: u64, rate: f32) -> Vec<Attack> {
    let every = |ms: u64| round50((ms as f32 * rate) as u64);
    let seconds = age as f32 / 1000.;
    match pattern {
        Pattern::Spiral {
            projectile,
            arms,
            spin,
            every: ms,
        } => {
            let ring =
                Attack::ring(projectile, arms, every(ms), 0.).fixed((spin * seconds).to_radians());
            vec![ring]
        }
        Pattern::Ring {
            projectile,
            count,
            turn,
            every: ms,
        } => {
            let volleys = age / every(ms);
            vec![
                Attack::ring(projectile, count, every(ms), 0.)
                    .fixed((turn * volleys as f32).to_radians()),
            ]
        }
        Pattern::Wave {
            projectile,
            count,
            rings,
            gap,
            every: ms,
        } => (0..rings)
            .map(|i| {
                let half = 180. / count as f32;
                Attack::ring(projectile, count, every(ms), 0.)
                    .fixed(if i % 2 == 0 { 0. } else { half.to_radians() })
                    .delay(round50(gap) * i as u64)
            })
            .collect(),
        Pattern::Cross {
            projectile,
            arms,
            width,
            spin,
            every: ms,
        } => (0..arms)
            .map(|arm| {
                let angle = spin * seconds + 360. * arm as f32 / arms as f32;
                Attack::fan(projectile, width, 6., every(ms), 0.).fixed(angle.to_radians())
            })
            .collect(),
        Pattern::Flower {
            projectile,
            petals,
            every: ms,
        } => vec![
            Attack::ring(projectile, petals, every(ms), 0.).fixed(0.),
            Attack::ring(projectile, petals, every(ms), 0.)
                .fixed((180. / petals as f32).to_radians())
                .delay(round50(every(ms) / 2)),
        ],
        Pattern::Aimed {
            projectile,
            count,
            gap,
            every: ms,
            lead,
        } => {
            let fan = Attack::fan(projectile, count, gap, every(ms), AIM_RANGE);
            vec![if lead { fan.chance(0.7) } else { fan }]
        }
        Pattern::Strike {
            radius,
            damage,
            every: ms,
        } => vec![Attack::grenade(radius, damage, every(ms), AIM_RANGE)],
        Pattern::Gate {
            projectile,
            count,
            gap,
            drift,
            every: ms,
        } => {
            // The fan covers every slot but the gap, centred opposite it.
            let gate = drift * seconds;
            vec![
                Attack::fan(projectile, count - gap, 360. / count as f32, every(ms), 0.)
                    .fixed((gate + 180.).to_radians()),
            ]
        }
        Pattern::Pendulum {
            projectile,
            arms,
            swing,
            period,
            every: ms,
        } => {
            let turn = age as f32 / period as f32 * std::f32::consts::TAU;
            vec![
                Attack::ring(projectile, arms, every(ms), 0.)
                    .fixed((swing * turn.sin()).to_radians()),
            ]
        }
        Pattern::Stream {
            projectile,
            shots,
            gap,
            every: ms,
            lead,
        } => (0..shots)
            .map(|i| {
                let shot = Attack::fan(projectile, 1, 0., every(ms), AIM_RANGE)
                    .delay(round50(gap) * i as u64);
                if lead { shot.chance(1.) } else { shot }
            })
            .collect(),
        Pattern::Rain {
            radius,
            damage,
            count,
            scatter,
            every: ms,
        } => (0..count)
            .map(|i| {
                Attack::grenade(radius, damage, every(ms), AIM_RANGE)
                    .scatter(scatter)
                    .delay(150 * i as u64)
            })
            .collect(),
    }
}
/// The effect of the phase the boss is in.
pub fn effect(kind: u16, elapsed: u64) -> Effect {
    entry(kind)
        .zip(phase(kind, elapsed))
        .map_or(Effect::None, |((phases, _), (index, ..))| {
            phases[index].effect
        })
}
pub fn kit(key: &str) -> Option<&'static [Phase]> {
    Some(match key {
        // Lich King: memories that spiral and flood the crypt.
        "rey_liche" => &[
            Phase {
                name: "Torrent of Memories",
                ms: 10_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Spiral {
                        projectile: SHURIKEN,
                        arms: 6,
                        spin: 70.,
                        every: 150,
                    },
                    Pattern::Aimed {
                        projectile: BOLT,
                        count: 3,
                        gap: 12.,
                        every: 1400,
                        lead: true,
                    },
                ],
            },
            Phase {
                name: "Requiem of Bones",
                ms: 9_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Wave {
                        projectile: RING,
                        count: 24,
                        rings: 3,
                        gap: 250,
                        every: 2200,
                    },
                    Pattern::Strike {
                        radius: 2.5,
                        damage: 110,
                        every: 1600,
                    },
                ],
            },
            Phase {
                name: "Pale Crossing",
                ms: 9_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Cross {
                        projectile: CRESCENT,
                        arms: 4,
                        width: 3,
                        spin: -45.,
                        every: 200,
                    },
                    Pattern::Aimed {
                        projectile: ORB,
                        count: 1,
                        gap: 0.,
                        every: 1800,
                        lead: false,
                    },
                ],
            },
        ],
        // Fire Dragon: breath, embers and volcanic strikes.
        "dragon_fuego" => &[
            Phase {
                name: "Dragon's Breath",
                ms: 9_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Aimed {
                        projectile: CRESCENT,
                        count: 7,
                        gap: 9.,
                        every: 600,
                        lead: false,
                    },
                    Pattern::Ring {
                        projectile: RING,
                        count: 18,
                        turn: 10.,
                        every: 1500,
                    },
                ],
            },
            Phase {
                name: "Ember Storm",
                ms: 10_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Spiral {
                        projectile: BOLT,
                        arms: 4,
                        spin: 120.,
                        every: 100,
                    },
                    Pattern::Strike {
                        radius: 3.,
                        damage: 150,
                        every: 1300,
                    },
                ],
            },
            Phase {
                name: "Molten Heart",
                ms: 9_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Wave {
                        projectile: ORB,
                        count: 12,
                        rings: 2,
                        gap: 600,
                        every: 2500,
                    },
                    Pattern::Aimed {
                        projectile: BOLT,
                        count: 3,
                        gap: 15.,
                        every: 900,
                        lead: true,
                    },
                ],
            },
        ],
        // Naga Queen: petals of a youthful spring and lonely beauty.
        "reina_naga" => &[
            Phase {
                name: "Youthful Spring",
                ms: 10_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Flower {
                        projectile: SHURIKEN,
                        petals: 16,
                        every: 900,
                    },
                    Pattern::Aimed {
                        projectile: BOLT,
                        count: 2,
                        gap: 20.,
                        every: 1100,
                        lead: true,
                    },
                ],
            },
            Phase {
                name: "Beauty is Solitary",
                ms: 10_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Spiral {
                        projectile: CRESCENT,
                        arms: 3,
                        spin: -90.,
                        every: 120,
                    },
                    Pattern::Strike {
                        radius: 2.5,
                        damage: 120,
                        every: 1500,
                    },
                ],
            },
            Phase {
                name: "Serpent's Embrace",
                ms: 9_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Ring {
                        projectile: RING,
                        count: 20,
                        turn: 9.,
                        every: 700,
                    },
                    Pattern::Aimed {
                        projectile: ORB,
                        count: 3,
                        gap: 25.,
                        every: 2000,
                        lead: false,
                    },
                ],
            },
        ],
        // Frost Giant: glacier walls and blizzards.
        "gigante_hielo" => &[
            Phase {
                name: "Glacial Wall",
                ms: 10_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Wave {
                        projectile: RING,
                        count: 28,
                        rings: 4,
                        gap: 200,
                        every: 2600,
                    },
                    Pattern::Aimed {
                        projectile: CRESCENT,
                        count: 5,
                        gap: 8.,
                        every: 1100,
                        lead: true,
                    },
                ],
            },
            Phase {
                name: "Blizzard",
                ms: 10_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Cross {
                        projectile: SHURIKEN,
                        arms: 6,
                        width: 2,
                        spin: 60.,
                        every: 220,
                    },
                    Pattern::Strike {
                        radius: 3.,
                        damage: 160,
                        every: 1400,
                    },
                ],
            },
            Phase {
                name: "Avalanche",
                ms: 8_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Aimed {
                        projectile: ORB,
                        count: 5,
                        gap: 18.,
                        every: 900,
                        lead: false,
                    },
                    Pattern::Ring {
                        projectile: BOLT,
                        count: 12,
                        turn: 15.,
                        every: 600,
                    },
                ],
            },
        ],
        // Horned Demon: hellfire rings and pursuing flames.
        "demonio_rojo" => &[
            Phase {
                name: "Hellfire Halo",
                ms: 9_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Ring {
                        projectile: RING,
                        count: 24,
                        turn: 7.5,
                        every: 500,
                    },
                    Pattern::Aimed {
                        projectile: BOLT,
                        count: 3,
                        gap: 10.,
                        every: 1000,
                        lead: true,
                    },
                ],
            },
            Phase {
                name: "Infernal Pact",
                ms: 9_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Spiral {
                        projectile: CRESCENT,
                        arms: 5,
                        spin: 100.,
                        every: 140,
                    },
                    Pattern::Strike {
                        radius: 2.5,
                        damage: 100,
                        every: 1500,
                    },
                ],
            },
        ],
        // Cube Deity: electrified grid, like Static Circuit.
        "dios_cubo" => &[
            Phase {
                name: "Static Circuit",
                ms: 10_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Cross {
                        projectile: CRESCENT,
                        arms: 4,
                        width: 4,
                        spin: 0.,
                        every: 350,
                    },
                    Pattern::Strike {
                        radius: 2.5,
                        damage: 100,
                        every: 1100,
                    },
                    Pattern::Aimed {
                        projectile: BOLT,
                        count: 1,
                        gap: 0.,
                        every: 500,
                        lead: true,
                    },
                ],
            },
            Phase {
                name: "Overcharge",
                ms: 8_000,
                effect: Effect::None,
                patterns: &[Pattern::Spiral {
                    projectile: BOLT,
                    arms: 8,
                    spin: 45.,
                    every: 200,
                }],
            },
        ],
        // Great Sphinx: riddles of sand and gold.
        "esfinge" => &[
            Phase {
                name: "Riddle of Sands",
                ms: 9_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Flower {
                        projectile: RING,
                        petals: 12,
                        every: 1100,
                    },
                    Pattern::Aimed {
                        projectile: CRESCENT,
                        count: 3,
                        gap: 14.,
                        every: 1200,
                        lead: false,
                    },
                ],
            },
            Phase {
                name: "Golden Judgement",
                ms: 9_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Strike {
                        radius: 3.,
                        damage: 90,
                        every: 1200,
                    },
                    Pattern::Ring {
                        projectile: BOLT,
                        count: 10,
                        turn: 18.,
                        every: 700,
                    },
                ],
            },
        ],
        // Stone Golem: slow, heavy boulders and quakes.
        "golem" => &[
            Phase {
                name: "Rockslide",
                ms: 9_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Aimed {
                        projectile: ORB,
                        count: 3,
                        gap: 20.,
                        every: 1300,
                        lead: false,
                    },
                    Pattern::Wave {
                        projectile: BOLT,
                        count: 16,
                        rings: 2,
                        gap: 400,
                        every: 2400,
                    },
                ],
            },
            Phase {
                name: "Earthquake",
                ms: 8_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Strike {
                        radius: 3.5,
                        damage: 100,
                        every: 1200,
                    },
                    Pattern::Ring {
                        projectile: RING,
                        count: 14,
                        turn: 12.,
                        every: 900,
                    },
                ],
            },
        ],
        // Three-Headed Hydra: three heads, three streams.
        "hidra" => &[
            Phase {
                name: "Venom Triad",
                ms: 9_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Cross {
                        projectile: SHURIKEN,
                        arms: 3,
                        width: 2,
                        spin: 80.,
                        every: 250,
                    },
                    Pattern::Aimed {
                        projectile: BOLT,
                        count: 3,
                        gap: 20.,
                        every: 1100,
                        lead: false,
                    },
                ],
            },
            Phase {
                name: "Toxic Bloom",
                ms: 8_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Flower {
                        projectile: RING,
                        petals: 10,
                        every: 1000,
                    },
                    Pattern::Strike {
                        radius: 2.5,
                        damage: 70,
                        every: 1700,
                    },
                ],
            },
        ],
        // Bone King: royal decree of bone and death.
        "rey_esqueleto" => &[
            Phase {
                name: "Royal Decree",
                ms: 9_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Ring {
                        projectile: RING,
                        count: 16,
                        turn: 11.25,
                        every: 800,
                    },
                    Pattern::Aimed {
                        projectile: CRESCENT,
                        count: 3,
                        gap: 12.,
                        every: 1000,
                        lead: true,
                    },
                ],
            },
            Phase {
                name: "March of the Dead",
                ms: 8_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Spiral {
                        projectile: BOLT,
                        arms: 3,
                        spin: 110.,
                        every: 160,
                    },
                    Pattern::Strike {
                        radius: 2.5,
                        damage: 70,
                        every: 1600,
                    },
                ],
            },
        ],
        // Void Wyrm: a rotating gate in a wall of stars, a shielded singularity, then starfall.
        "wyrm_vacio" => &[
            Phase {
                name: "Event Horizon",
                ms: 10_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Gate {
                        projectile: RING,
                        count: 36,
                        gap: 5,
                        drift: 40.,
                        every: 700,
                    },
                    Pattern::Stream {
                        projectile: BOLT,
                        shots: 5,
                        gap: 120,
                        every: 2000,
                        lead: true,
                    },
                ],
            },
            Phase {
                name: "Singularity",
                ms: 8_000,
                effect: Effect::Shield,
                patterns: &[
                    Pattern::Pendulum {
                        projectile: SHURIKEN,
                        arms: 8,
                        swing: 60.,
                        period: 4000,
                        every: 150,
                    },
                    Pattern::Aimed {
                        projectile: ORB,
                        count: 1,
                        gap: 0.,
                        every: 1600,
                        lead: false,
                    },
                ],
            },
            Phase {
                name: "Starfall",
                ms: 9_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Rain {
                        radius: 2.5,
                        damage: 120,
                        count: 5,
                        scatter: 5.,
                        every: 2200,
                    },
                    Pattern::Spiral {
                        projectile: CRESCENT,
                        arms: 3,
                        spin: 90.,
                        every: 200,
                    },
                ],
            },
        ],
        // Broodmother: hatches spiders, weaves a web with one way out and rains venom.
        "reina_arana" => &[
            Phase {
                name: "Hatching",
                ms: 9_000,
                effect: Effect::Summon {
                    minion: "Black Spider",
                    count: 4,
                },
                patterns: &[
                    Pattern::Flower {
                        projectile: SHURIKEN,
                        petals: 10,
                        every: 1100,
                    },
                    Pattern::Aimed {
                        projectile: BOLT,
                        count: 3,
                        gap: 15.,
                        every: 1200,
                        lead: false,
                    },
                ],
            },
            Phase {
                name: "Web of Lies",
                ms: 9_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Gate {
                        projectile: RING,
                        count: 30,
                        gap: 6,
                        drift: -30.,
                        every: 900,
                    },
                    Pattern::Stream {
                        projectile: CRESCENT,
                        shots: 4,
                        gap: 150,
                        every: 1800,
                        lead: false,
                    },
                ],
            },
            Phase {
                name: "Venom Rain",
                ms: 8_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Rain {
                        radius: 2.,
                        damage: 70,
                        count: 4,
                        scatter: 4.,
                        every: 1800,
                    },
                    Pattern::Ring {
                        projectile: BOLT,
                        count: 12,
                        turn: 15.,
                        every: 900,
                    },
                ],
            },
        ],
        // Vampire Lord: flies at you, feasts to heal unless you burst him, calls his bats.
        "senor_vampiro" => &[
            Phase {
                name: "Night Flight",
                ms: 8_000,
                effect: Effect::Blink,
                patterns: &[
                    Pattern::Stream {
                        projectile: CRESCENT,
                        shots: 6,
                        gap: 100,
                        every: 1800,
                        lead: true,
                    },
                    Pattern::Ring {
                        projectile: RING,
                        count: 16,
                        turn: 11.25,
                        every: 900,
                    },
                ],
            },
            Phase {
                name: "Crimson Feast",
                ms: 8_000,
                effect: Effect::Mend { percent: 10 },
                patterns: &[
                    Pattern::Pendulum {
                        projectile: BOLT,
                        arms: 6,
                        swing: 45.,
                        period: 3000,
                        every: 180,
                    },
                    Pattern::Aimed {
                        projectile: ORB,
                        count: 3,
                        gap: 20.,
                        every: 1500,
                        lead: false,
                    },
                ],
            },
            Phase {
                name: "Swarm of Bats",
                ms: 8_000,
                effect: Effect::Summon {
                    minion: "Purple Bat",
                    count: 5,
                },
                patterns: &[
                    Pattern::Flower {
                        projectile: SHURIKEN,
                        petals: 14,
                        every: 1000,
                    },
                    Pattern::Strike {
                        radius: 2.5,
                        damage: 110,
                        every: 1500,
                    },
                ],
            },
        ],
        // Iron Colossus: lock-on bursts, a bullet curtain with one gap, a shielded overdrive.
        "coloso_hierro" => &[
            Phase {
                name: "Targeting Protocol",
                ms: 9_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Stream {
                        projectile: BOLT,
                        shots: 8,
                        gap: 90,
                        every: 1600,
                        lead: true,
                    },
                    Pattern::Strike {
                        radius: 3.,
                        damage: 130,
                        every: 1800,
                    },
                ],
            },
            Phase {
                name: "Bullet Curtain",
                ms: 10_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Gate {
                        projectile: CRESCENT,
                        count: 40,
                        gap: 6,
                        drift: 25.,
                        every: 450,
                    },
                    Pattern::Aimed {
                        projectile: ORB,
                        count: 1,
                        gap: 0.,
                        every: 2000,
                        lead: false,
                    },
                ],
            },
            Phase {
                name: "Overdrive",
                ms: 8_000,
                effect: Effect::Shield,
                patterns: &[Pattern::Pendulum {
                    projectile: RING,
                    arms: 10,
                    swing: 90.,
                    period: 5000,
                    every: 120,
                }],
            },
        ],
        // Storm Gargoyle: dives at you, turns to stone under a lightning rain, then thunders.
        "gargola_tormenta" => &[
            Phase {
                name: "Lightning Dive",
                ms: 8_000,
                effect: Effect::Blink,
                patterns: &[
                    Pattern::Cross {
                        projectile: CRESCENT,
                        arms: 4,
                        width: 3,
                        spin: 30.,
                        every: 300,
                    },
                    Pattern::Aimed {
                        projectile: BOLT,
                        count: 3,
                        gap: 12.,
                        every: 900,
                        lead: true,
                    },
                ],
            },
            Phase {
                name: "Stone Form",
                ms: 7_000,
                effect: Effect::Shield,
                patterns: &[
                    Pattern::Rain {
                        radius: 2.5,
                        damage: 90,
                        count: 4,
                        scatter: 5.,
                        every: 1600,
                    },
                    Pattern::Ring {
                        projectile: RING,
                        count: 12,
                        turn: 15.,
                        every: 1200,
                    },
                ],
            },
            Phase {
                name: "Thunderhead",
                ms: 9_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Wave {
                        projectile: BOLT,
                        count: 20,
                        rings: 3,
                        gap: 200,
                        every: 2200,
                    },
                    Pattern::Stream {
                        projectile: SHURIKEN,
                        shots: 4,
                        gap: 120,
                        every: 1500,
                        lead: false,
                    },
                ],
            },
        ],
        // Mad Archmage: an arcane labyrinth, blinking volleys, a meteor shower and a mana barrier.
        "archimago" => &[
            Phase {
                name: "Arcane Labyrinth",
                ms: 10_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Gate {
                        projectile: RING,
                        count: 32,
                        gap: 5,
                        drift: 60.,
                        every: 600,
                    },
                    Pattern::Aimed {
                        projectile: BOLT,
                        count: 2,
                        gap: 10.,
                        every: 1300,
                        lead: true,
                    },
                ],
            },
            Phase {
                name: "Blink Volley",
                ms: 8_000,
                effect: Effect::Blink,
                patterns: &[
                    Pattern::Stream {
                        projectile: CRESCENT,
                        shots: 5,
                        gap: 100,
                        every: 1400,
                        lead: true,
                    },
                    Pattern::Flower {
                        projectile: ORB,
                        petals: 8,
                        every: 1600,
                    },
                ],
            },
            Phase {
                name: "Meteor Shower",
                ms: 9_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Rain {
                        radius: 3.,
                        damage: 140,
                        count: 6,
                        scatter: 6.,
                        every: 2600,
                    },
                    Pattern::Pendulum {
                        projectile: SHURIKEN,
                        arms: 4,
                        swing: 120.,
                        period: 2500,
                        every: 200,
                    },
                ],
            },
            Phase {
                name: "Mana Barrier",
                ms: 6_000,
                effect: Effect::Shield,
                patterns: &[Pattern::Spiral {
                    projectile: BOLT,
                    arms: 6,
                    spin: -80.,
                    every: 160,
                }],
            },
        ],
        // The Sandman: drifting gates through a dream, then a shielded storm of wisps.
        "arenero" => &[
            Phase {
                name: "Drifting Sleep",
                ms: 9_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Gate {
                        projectile: ORB,
                        count: 18,
                        gap: 4,
                        drift: 25.,
                        every: 1800,
                    },
                    Pattern::Aimed {
                        projectile: BOLT,
                        count: 2,
                        gap: 14.,
                        every: 1200,
                        lead: true,
                    },
                ],
            },
            Phase {
                name: "Sleepwalkers",
                ms: 8_000,
                effect: Effect::Summon {
                    minion: "Sleepwalker",
                    count: 4,
                },
                patterns: &[
                    Pattern::Pendulum {
                        projectile: RING,
                        arms: 5,
                        swing: 60.,
                        period: 2800,
                        every: 220,
                    },
                ],
            },
            Phase {
                name: "Nightmare Storm",
                ms: 7_000,
                effect: Effect::Shield,
                patterns: &[
                    Pattern::Spiral {
                        projectile: SHURIKEN,
                        arms: 5,
                        spin: 90.,
                        every: 160,
                    },
                    Pattern::Rain {
                        radius: 2.,
                        damage: 90,
                        count: 3,
                        scatter: 5.,
                        every: 2000,
                    },
                ],
            },
        ],
        // Head Chef: knife streams, boiling strikes and a kitchen full of cooks.
        "chef" => &[
            Phase {
                name: "Knife Work",
                ms: 8_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Stream {
                        projectile: CRESCENT,
                        shots: 5,
                        gap: 120,
                        every: 1600,
                        lead: true,
                    },
                    Pattern::Cross {
                        projectile: BOLT,
                        arms: 4,
                        width: 2,
                        spin: 40.,
                        every: 700,
                    },
                ],
            },
            Phase {
                name: "Boiling Over",
                ms: 8_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Strike {
                        radius: 2.5,
                        damage: 100,
                        every: 1300,
                    },
                    Pattern::Wave {
                        projectile: RING,
                        count: 12,
                        rings: 3,
                        gap: 250,
                        every: 2600,
                    },
                ],
            },
            Phase {
                name: "Order Up",
                ms: 8_000,
                effect: Effect::Summon {
                    minion: "Angry Cook",
                    count: 4,
                },
                patterns: &[
                    Pattern::Flower {
                        projectile: ORB,
                        petals: 10,
                        every: 1500,
                    },
                ],
            },
        ],
        // Gigacorn: rainbow rings, a charging blink and a sugar rush of gumdrops.
        "gigacorn" => &[
            Phase {
                name: "Rainbow Burst",
                ms: 8_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Ring {
                        projectile: RING,
                        count: 20,
                        turn: 9.,
                        every: 800,
                    },
                    Pattern::Aimed {
                        projectile: BOLT,
                        count: 3,
                        gap: 18.,
                        every: 1300,
                        lead: false,
                    },
                ],
            },
            Phase {
                name: "Horn Charge",
                ms: 7_000,
                effect: Effect::Blink,
                patterns: &[
                    Pattern::Stream {
                        projectile: CRESCENT,
                        shots: 7,
                        gap: 90,
                        every: 1500,
                        lead: true,
                    },
                    Pattern::Cross {
                        projectile: SHURIKEN,
                        arms: 6,
                        width: 1,
                        spin: 60.,
                        every: 500,
                    },
                ],
            },
            Phase {
                name: "Sugar Rush",
                ms: 8_000,
                effect: Effect::Summon {
                    minion: "Gumdrop",
                    count: 6,
                },
                patterns: &[
                    Pattern::Gate {
                        projectile: ORB,
                        count: 16,
                        gap: 3,
                        drift: 35.,
                        every: 2000,
                    },
                ],
            },
        ],
        // Ghost of Skuld: wailing waves, a grave-raising mend and a shielded haunting.
        "skuld" => &[
            Phase {
                name: "Wail",
                ms: 8_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Wave {
                        projectile: RING,
                        count: 14,
                        rings: 4,
                        gap: 220,
                        every: 2800,
                    },
                    Pattern::Aimed {
                        projectile: BOLT,
                        count: 4,
                        gap: 10.,
                        every: 1100,
                        lead: true,
                    },
                ],
            },
            Phase {
                name: "Rise, My Dead",
                ms: 8_000,
                effect: Effect::Summon {
                    minion: "Grave Zombie",
                    count: 5,
                },
                patterns: &[
                    Pattern::Pendulum {
                        projectile: SHURIKEN,
                        arms: 4,
                        swing: 50.,
                        period: 2600,
                        every: 200,
                    },
                ],
            },
            Phase {
                name: "Haunting",
                ms: 7_000,
                effect: Effect::Shield,
                patterns: &[
                    Pattern::Spiral {
                        projectile: ORB,
                        arms: 4,
                        spin: 50.,
                        every: 220,
                    },
                    Pattern::Strike {
                        radius: 2.5,
                        damage: 120,
                        every: 1400,
                    },
                ],
            },
        ],
        // Greedy Treasurer: coin crosses, a cave-in rain and a mend behind a bat swarm.
        "tesorero" => &[
            Phase {
                name: "Coin Toss",
                ms: 8_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Cross {
                        projectile: RING,
                        arms: 5,
                        width: 2,
                        spin: 35.,
                        every: 600,
                    },
                    Pattern::Stream {
                        projectile: BOLT,
                        shots: 4,
                        gap: 150,
                        every: 1700,
                        lead: true,
                    },
                ],
            },
            Phase {
                name: "Cave-In",
                ms: 8_000,
                effect: Effect::None,
                patterns: &[
                    Pattern::Rain {
                        radius: 2.2,
                        damage: 110,
                        count: 4,
                        scatter: 6.,
                        every: 1800,
                    },
                    Pattern::Flower {
                        projectile: SHURIKEN,
                        petals: 12,
                        every: 1200,
                    },
                ],
            },
            Phase {
                name: "Hoard",
                ms: 8_000,
                effect: Effect::Mend { percent: 8 },
                patterns: &[
                    Pattern::Gate {
                        projectile: ORB,
                        count: 20,
                        gap: 4,
                        drift: 30.,
                        every: 1900,
                    },
                ],
            },
        ],
        _ => return None,
    })
}
fn entry(kind: u16) -> Option<(&'static [Phase], i32)> {
    let &(_, key, hp) = BOSS_KITS.iter().find(|(k, ..)| *k == kind)?;
    Some((kit(key)?, hp))
}
/// Current phase (index into the kit) and time spent in it.
pub fn phase(kind: u16, elapsed: u64) -> Option<(usize, &'static str, u64)> {
    let (phases, _) = entry(kind)?;
    let cycle: u64 = phases.iter().map(|p| p.ms).sum();
    let mut t = elapsed % cycle;
    for (i, p) in phases.iter().enumerate() {
        if t < p.ms {
            return Some((i, p.name, t));
        }
        t -= p.ms;
    }
    unreachable!("phase cycle covers every instant")
}
pub fn enraged(kind: u16, hp: i32) -> bool {
    entry(kind).is_some_and(|(_, max)| hp * 2 < max)
}
pub fn schedule(kind: u16, hp: i32, elapsed: u64) -> Option<Schedule> {
    let (phases, _) = entry(kind)?;
    let (index, _, age) = phase(kind, elapsed)?;
    let enraged = enraged(kind, hp);
    let rate = if enraged { 0.75 } else { 1. };
    Some(Schedule {
        state: (elapsed / phases.iter().map(|p| p.ms).sum::<u64>()) << 8
            | (index as u64) << 1
            | u64::from(enraged),
        age,
        attacks: phases[index]
            .patterns
            .iter()
            .flat_map(|p| attacks(*p, age, rate))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_boss_has_a_valid_cycle_of_named_phases() {
        for (kind, key, hp) in BOSS_KITS {
            let phases = kit(key).unwrap_or_else(|| panic!("{key} has no bullet-hell kit"));
            assert!(phases.len() >= 2 && phases.iter().all(|p| p.ms >= 5000 && !p.name.is_empty()));
            let mut names = std::collections::BTreeSet::new();
            for t in (0..120_000).step_by(250) {
                let s = schedule(kind, hp, t).unwrap();
                assert!(!s.attacks.is_empty(), "{key} silent at {t}");
                names.insert(phase(kind, t).unwrap().1);
                let enraged = schedule(kind, hp / 3, t).unwrap();
                assert_ne!(s.state, enraged.state, "enrage changes the channel state");
            }
            assert_eq!(names.len(), phases.len(), "{key}: every phase is reached");
        }
    }

    #[test]
    fn space_control_ignores_the_player_while_tracking_attacks_aim_at_them() {
        use super::super::attacks::{Action, Aim};
        let s = schedule(
            BOSS_KITS.iter().find(|k| k.1 == "rey_liche").unwrap().0,
            20000,
            0,
        )
        .unwrap();
        assert!(
            matches!(s.attacks[0].aim, Aim::Fixed(_)),
            "spiral is geometric"
        );
        assert!(
            matches!(s.attacks[1].aim, Aim::Target),
            "aimed bolts track the player"
        );
        let (a, b) = (
            attacks(kit("rey_liche").unwrap()[0].patterns[0], 0, 1.),
            attacks(kit("rey_liche").unwrap()[0].patterns[0], 1000, 1.),
        );
        let angle = |v: &[Attack]| match v[0].aim {
            Aim::Fixed(x) => x,
            Aim::Target => unreachable!(),
        };
        assert!(
            (angle(&b) - angle(&a) - 70f32.to_radians()).abs() < 1e-4,
            "spiral rotates with time"
        );
        let strike = attacks(
            Pattern::Strike {
                radius: 2.,
                damage: 50,
                every: 1000,
            },
            0,
            1.,
        );
        assert!(
            matches!(strike[0].action, Action::Grenade { .. })
                && matches!(strike[0].aim, Aim::Target)
        );
    }

    #[test]
    fn gates_leave_one_lane_streams_stagger_and_rain_scatters() {
        use super::super::attacks::{Action, Aim};
        let gate = attacks(
            Pattern::Gate {
                projectile: RING,
                count: 36,
                gap: 5,
                drift: 40.,
                every: 700,
            },
            1000,
            1.,
        );
        let shots = gate[0].shots(RING, 0.);
        assert_eq!(shots.len(), 31, "five slots stay open");
        let Aim::Fixed(centre) = gate[0].aim else {
            panic!("gates are geometric")
        };
        // the lane is opposite the fan centre and turns with time
        let lane = centre - std::f32::consts::PI;
        assert!((lane - 40f32.to_radians()).abs() < 1e-4);
        let near = shots
            .iter()
            .map(|(_, a)| {
                use std::f32::consts::{PI, TAU};
                ((a - lane + PI).rem_euclid(TAU) - PI).abs()
            })
            .fold(f32::MAX, f32::min);
        assert!(near > 2. * 10f32.to_radians(), "no bullet inside the lane");

        let stream = attacks(
            Pattern::Stream {
                projectile: BOLT,
                shots: 4,
                gap: 100,
                every: 1500,
                lead: true,
            },
            0,
            1.,
        );
        assert_eq!(
            stream.iter().map(|a| a.delay).collect::<Vec<_>>(),
            [0, 100, 200, 300]
        );
        assert!(stream.iter().all(|a| matches!(a.aim, Aim::Target)));

        let rain = attacks(
            Pattern::Rain {
                radius: 2.,
                damage: 50,
                count: 3,
                scatter: 4.,
                every: 2000,
            },
            0,
            1.,
        );
        assert_eq!(rain.len(), 3);
        assert!(
            rain.iter()
                .all(|a| matches!(a.action, Action::Grenade { scatter, .. } if scatter == 4.))
        );
    }
}
