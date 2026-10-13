use crate::{
    content::{Content, Map},
    encounter::{Motion, Profile},
};
use anyhow::{Result, ensure};
use std::{collections::BTreeSet, f32::consts::TAU};

pub const CASTLE: &str = "oryx-castle";
pub const CHAMBER: &str = "oryx-chamber";
pub const CELLAR: &str = "wine-cellar";
pub const GUARDIANS: [u16; 2] = [0xd78, 0xd79];
pub const ORYX: u16 = 0x1740;
pub const ORYX2: u16 = 0x932;
pub const LOCKED_CELLAR: u16 = 0x721;
pub const INCANTATION: u16 = 0x722;
pub const HEROES: [u16; 2] = [0x668, 0x67d];
pub const ROSTER: &[u16] = &[
    0x668, 0x669, 0x66a, 0x66b, 0x67d, 0x67e, 0x67f, 0x680, 0x681, 0x682, 0xd70, 0xd71, 0xd78,
    0xd79, 0xd7a, 0xd7c, 0xd7d, 0xd7e, 0xd7f, 0xd80, 0xd81, 0xd82, 0xd83, 0xd84, 0xd87, 0x1740,
    0x1748, 0x1749, 0x174a, 0x174b, 0x932, 0x944, 0xde0, 0xde1, 0xde2, 0xde3, 0xde4, 0xdf1, 0xdf2,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RealmStage {
    Open,
    Closing { deadline: u64 },
    Sealed { deadline: u64 },
    Transferring,
}
pub struct RealmCycle {
    pub remaining: BTreeSet<i32>,
    pub stage: RealmStage,
}
impl RealmCycle {
    pub fn new(heroes: BTreeSet<i32>) -> Result<Self> {
        ensure!(
            !heroes.is_empty(),
            "Realm closure needs a finite initial hero roster"
        );
        Ok(Self {
            remaining: heroes,
            stage: RealmStage::Open,
        })
    }
    pub fn defeated(&mut self, id: i32, now: u64) -> bool {
        if self.stage != RealmStage::Open || !self.remaining.remove(&id) {
            return false;
        }
        if self.remaining.is_empty() {
            self.stage = RealmStage::Closing {
                deadline: now + 60_000,
            };
        }
        true
    }
    pub fn advance(&mut self, now: u64) -> bool {
        match self.stage {
            RealmStage::Closing { deadline } if now >= deadline => {
                self.stage = RealmStage::Sealed {
                    deadline: deadline + 22_000,
                };
                true
            }
            RealmStage::Sealed { deadline } if now >= deadline => {
                self.stage = RealmStage::Transferring;
                true
            }
            _ => false,
        }
    }
    pub fn admits(&self) -> bool {
        self.stage == RealmStage::Open
    }
}

pub fn profile(kind: u16) -> Option<Profile> {
    let (motion, speed, interval, range) = match kind {
        0x668 => (
            Motion::Chase {
                sight: 12.,
                stop: 2.,
            },
            4.5,
            500,
            12.,
        ),
        0x669 => (Motion::Wander, 7., 250, 10.),
        0x66a | 0x66b => (Motion::Wander, 7., 1000, 10.),
        0x67d => (Motion::Wander, 3., 1000, 12.),
        0x67e..=0x682 => (
            Motion::Chase {
                sight: 10.,
                stop: 2.,
            },
            8.,
            1000,
            10.,
        ),
        0xd70 | 0xd71 | 0xd7a | 0xd7d | 0xdf1 => (Motion::Wander, 0., 0, 0.),
        0xd78 | 0xd79 => (Motion::Circle, 6., 200, 20.),
        0xd7c => (Motion::Circle, 7., 900, 12.),
        0xd7e => (
            Motion::Chase {
                sight: 10.,
                stop: 2.,
            },
            3.,
            600,
            10.,
        ),
        0xd7f => (Motion::Wander, 2., 900, 10.),
        0xd80 => (
            Motion::Chase {
                sight: 10.,
                stop: 1.,
            },
            5.,
            250,
            10.,
        ),
        0xd81 => (
            Motion::Chase {
                sight: 10.,
                stop: 0.,
            },
            4.,
            500,
            10.,
        ),
        0xd82 => (
            Motion::Chase {
                sight: 10.,
                stop: 1.,
            },
            4.,
            300,
            10.,
        ),
        0xd83 => (
            Motion::Chase {
                sight: 10.,
                stop: 3.,
            },
            4.,
            350,
            10.,
        ),
        0xd84 => (
            Motion::Chase {
                sight: 10.,
                stop: 0.,
            },
            4.,
            200,
            10.,
        ),
        0xd87 => (Motion::Wander, 0., 500, 5.),
        ORYX => (Motion::Wander, 0., 200, 25.),
        ORYX2 => (
            Motion::Chase {
                sight: 25.,
                stop: 3.,
            },
            2.,
            500,
            25.,
        ),
        0x1748 | 0x174a | 0x174b => (Motion::Circle, 3., 700, 25.),
        0x1749 => (
            Motion::Chase {
                sight: 15.,
                stop: 2.,
            },
            3.,
            1000,
            12.,
        ),
        0x944 => (Motion::Circle, 2., 2500, 15.),
        0xde0 => (
            Motion::Chase {
                sight: 12.,
                stop: 3.,
            },
            3.,
            1000,
            12.,
        ),
        0xde1 | 0xde3 => (Motion::Wander, 2., 0, 0.),
        0xde4 => (Motion::Wander, 0., 1000, 3.),
        0xde2 => (
            Motion::Chase {
                sight: 10.,
                stop: 0.,
            },
            6.,
            1000,
            4.,
        ),
        0xdf2 => (Motion::Wander, 2., 800, 10.),
        _ => return None,
    };
    Some(Profile {
        motion,
        speed,
        interval,
        range,
    })
}
fn fan(index: u8, count: usize, gap: f32, heading: f32) -> Vec<(u8, f32)> {
    (0..count)
        .map(|i| {
            (
                index,
                heading + (i as f32 - (count - 1) as f32 / 2.) * gap.to_radians(),
            )
        })
        .collect()
}
fn ring(index: u8, count: usize, heading: f32) -> Vec<(u8, f32)> {
    (0..count)
        .map(|i| (index, heading + i as f32 * TAU / count as f32))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OryxPhase {
    Opening,
    Dance,
    Artifacts,
    Gaze,
    DanceTwo,
    PrepareRage,
    Rage,
}
pub fn oryx_phase(elapsed: u64) -> OryxPhase {
    if elapsed < 20_000 {
        return OryxPhase::Opening;
    }
    match (elapsed - 20_000) % 85_000 {
        0..25_000 => OryxPhase::Dance,
        25_000..50_000 => OryxPhase::Artifacts,
        50_000..60_000 => OryxPhase::Gaze,
        _ => OryxPhase::DanceTwo,
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Wave {
    pub kind: u16,
    pub count: usize,
    pub radius: f32,
    pub lifetime: u64,
}
pub struct BossFacts {
    pub phase: OryxPhase,
    pub changed: bool,
    pub invulnerable: bool,
    pub waves: Vec<Wave>,
}
#[derive(Default)]
pub struct BossBrain {
    phase: Option<OryxPhase>,
    rage_at: Option<u64>,
    next_summon: u64,
    last_elapsed: Option<u64>,
}
impl BossBrain {
    pub fn update(&mut self, kind: u16, hp: i32, max_hp: i32, elapsed: u64) -> Result<BossFacts> {
        ensure!(
            matches!(kind, ORYX | ORYX2) && hp > 0 && max_hp > 0,
            "invalid live Oryx phase input"
        );
        ensure!(
            self.last_elapsed.is_none_or(|last| elapsed >= last),
            "Oryx phase clock regressed"
        );
        self.last_elapsed = Some(elapsed);
        if kind == ORYX2 && hp <= max_hp / 5 {
            self.rage_at.get_or_insert(elapsed);
        }
        let phase = if kind == ORYX {
            if hp <= max_hp / 5 {
                OryxPhase::Rage
            } else {
                oryx_phase(elapsed)
            }
        } else if let Some(start) = self.rage_at {
            ensure!(elapsed >= start, "Oryx 2 phase clock regressed");
            if elapsed - start < 10_000 {
                OryxPhase::PrepareRage
            } else {
                OryxPhase::Rage
            }
        } else {
            OryxPhase::Opening
        };
        let changed = self.phase != Some(phase);
        self.phase = Some(phase);
        let mut waves = vec![];
        if changed && kind == ORYX {
            match phase {
                OryxPhase::Dance | OryxPhase::DanceTwo => waves.push(Wave {
                    kind: 0x1748,
                    count: 15,
                    radius: 9.,
                    lifetime: 20_000,
                }),
                OryxPhase::Artifacts => {
                    waves.push(Wave {
                        kind: 0x174a,
                        count: 4,
                        radius: 1.,
                        lifetime: 20_000,
                    });
                    waves.push(Wave {
                        kind: 0x174b,
                        count: 4,
                        radius: 9.,
                        lifetime: 20_000,
                    });
                }
                _ => {}
            }
        }
        if elapsed >= self.next_summon && matches!(phase, OryxPhase::Opening | OryxPhase::Rage) {
            if kind == ORYX {
                waves.push(Wave {
                    kind: 0x1749,
                    count: if phase == OryxPhase::Rage { 10 } else { 5 },
                    radius: 3.,
                    lifetime: 0,
                });
                self.next_summon = elapsed + 1500;
            } else if phase == OryxPhase::Opening {
                waves.push(Wave {
                    kind: 0x944,
                    count: 5,
                    radius: 3.,
                    lifetime: 0,
                });
                self.next_summon = elapsed + 5000;
            }
        }
        Ok(BossFacts {
            phase,
            changed,
            invulnerable: matches!(
                phase,
                OryxPhase::Dance | OryxPhase::DanceTwo | OryxPhase::PrepareRage
            ),
            waves,
        })
    }
}
#[derive(Default)]
pub struct GuardianBrain {
    recovery_at: Option<u64>,
    sword: Option<i32>,
    sword_cleared_at: Option<u64>,
}
pub struct GuardianFacts {
    pub invulnerable: bool,
    pub summon_sword: bool,
    pub heal: bool,
}
impl GuardianBrain {
    pub fn update(&mut self, peer_alive: bool, now: u64) -> Result<GuardianFacts> {
        if peer_alive || self.sword_cleared_at.is_some_and(|t| now >= t + 1400) {
            return Ok(GuardianFacts {
                invulnerable: false,
                summon_sword: false,
                heal: false,
            });
        }
        let start = *self.recovery_at.get_or_insert(now);
        ensure!(now >= start, "Guardian phase clock regressed");
        let summon_sword = now >= start + 4000 && self.sword.is_none();
        Ok(GuardianFacts {
            invulnerable: true,
            summon_sword,
            heal: summon_sword,
        })
    }
    pub fn bind_sword(&mut self, id: i32) -> Result<()> {
        ensure!(
            self.sword.is_none() && self.recovery_at.is_some(),
            "Guardian sword already bound or recovery not started"
        );
        self.sword = Some(id);
        Ok(())
    }
    pub fn sword_defeated(&mut self, id: i32, now: u64) -> Result<()> {
        ensure!(
            self.sword == Some(id) && self.sword_cleared_at.is_none(),
            "foreign or replayed Guardian sword defeat"
        );
        ensure!(
            self.recovery_at.is_some_and(|start| now >= start + 4000),
            "Guardian sword defeat clock regressed"
        );
        self.sword_cleared_at = Some(now);
        Ok(())
    }
}
pub fn attacks(kind: u16, hp: i32, max_hp: i32, heading: f32, elapsed: u64) -> Vec<(u8, f32)> {
    match kind {
        0x668 => {
            let mut shots = vec![(1, heading)];
            if elapsed % 1000 < 500 {
                shots.extend(fan(0, 5, 5., heading));
            }
            shots
        }
        0x669 | 0x67e..=0x682 | 0xd84 | 0xd87 | 0xde2 | 0xdf2 => vec![(0, heading)],
        0x66a | 0x66b => fan(0, 3, 5., heading),
        0x67d if elapsed % 10_000 < 5000 => fan(10, 3, 10., heading),
        0x67d => (0..4).map(|index| (index, heading)).collect(),
        0xd70 | 0xd71 | 0xd7a | 0xd7d | 0xde1 | 0xde3 | 0xdf1 => vec![],
        0xd78 | 0xd79 => match elapsed % 20_000 {
            0..5200 => ring(0, 2, elapsed as f32 / 200. * 30f32.to_radians()),
            5200..10_000 => fan(0, 2, 180., (elapsed as f32 / 100. * 10.).to_radians()),
            10_000..14_000 => ring(1, 7, heading),
            14_000..18_000 => fan(0, 5, 10., heading),
            _ => ring(2, 10, heading),
        },
        0xd7c => ring(0, 6, heading),
        0xd7e => fan(0, 2, 15., heading),
        0xd7f => vec![(0, heading)],
        0xd80 => {
            if elapsed % 1500 < 250 {
                ring(0, 5, heading)
            } else {
                vec![(0, heading)]
            }
        }
        0xd81 => {
            if elapsed % 1000 < 500 {
                ring(0, 5, heading)
            } else {
                vec![(1, heading)]
            }
        }
        0xd82 => {
            if elapsed % 10_000 < 5000 {
                ring(1, 5, heading)
            } else {
                let mut shots = ring(2, 5, heading);
                shots.extend(fan(0, 3, 10., heading));
                shots
            }
        }
        0xd83 => {
            if elapsed % 7500 < 5000 {
                fan(0, 3, 20., heading)
            } else {
                vec![(0, heading), (1, heading), (2, heading)]
            }
        }
        ORYX if hp <= max_hp / 5 => {
            let mut shots = fan(1, 2, 10., heading);
            shots.extend(fan(16, 5, 10., heading));
            shots
        }
        ORYX => match oryx_phase(elapsed) {
            OryxPhase::Opening => {
                if elapsed % 1000 < 200 {
                    fan(5, 4, 10., heading)
                } else {
                    vec![]
                }
            }
            OryxPhase::Dance => {
                if elapsed % 700 < 200 {
                    ring(6, 8, heading)
                } else {
                    vec![]
                }
            }
            OryxPhase::Artifacts => {
                let mut shots = vec![];
                if elapsed % 500 < 200 {
                    shots.extend(ring(7, 10, heading));
                }
                if elapsed % 1500 < 200 {
                    shots.extend(ring(9, 3, heading));
                }
                if elapsed % 2000 < 200 {
                    shots.extend(ring(8, 10, heading));
                }
                shots
            }
            OryxPhase::Gaze => {
                if elapsed % 1000 < 200 {
                    fan(1, 2, 10., heading)
                } else {
                    vec![]
                }
            }
            OryxPhase::DanceTwo => ring(
                8,
                4,
                [0f32, 30., 15., 45.][(elapsed / 200 % 4) as usize].to_radians(),
            ),
            OryxPhase::PrepareRage | OryxPhase::Rage => unreachable!(),
        },
        ORYX2 => {
            let mut shots = vec![];
            if elapsed % 1500 < 500 {
                shots.extend(ring(0, 8, heading));
            }
            if elapsed % 1000 < 500 {
                for (index, count) in [(1, 3), (2, 3), (3, 2), (4, 3), (5, 2), (6, 3)] {
                    shots.extend(fan(index, count, 10., heading));
                }
            }
            shots
        }
        0x1748 => ring(0, 12, heading),
        0x174a | 0x174b => ring((elapsed / 700 % 2) as u8, 4, heading),
        0x1749 => fan((elapsed / 1000 % 2) as u8, 3, 10., heading),
        0x944 => {
            let mut shots = vec![(0, heading)];
            shots.extend(fan(1, 3, 10., heading));
            shots
        }
        0xde4 => fan(0, 5, 7., heading),
        0xde0 => (0..5).map(|i| (i, heading + i as f32 * TAU / 5.)).collect(),
        _ => panic!("endgame attack requested for unselected object {kind:#x}"),
    }
}

pub fn summons(kind: u16) -> &'static [Wave] {
    match kind {
        0x668 => &[
            Wave {
                kind: 0x669,
                count: 3,
                radius: 3.,
                lifetime: 0,
            },
            Wave {
                kind: 0x66a,
                count: 3,
                radius: 3.,
                lifetime: 0,
            },
            Wave {
                kind: 0x66b,
                count: 3,
                radius: 3.,
                lifetime: 0,
            },
        ],
        0x67d => &[
            Wave {
                kind: 0x67e,
                count: 3,
                radius: 3.,
                lifetime: 0,
            },
            Wave {
                kind: 0x67f,
                count: 3,
                radius: 3.,
                lifetime: 0,
            },
            Wave {
                kind: 0x680,
                count: 3,
                radius: 3.,
                lifetime: 0,
            },
            Wave {
                kind: 0x681,
                count: 3,
                radius: 3.,
                lifetime: 0,
            },
            Wave {
                kind: 0x682,
                count: 3,
                radius: 3.,
                lifetime: 0,
            },
        ],
        0xd7d => &[Wave {
            kind: 0xd7e,
            count: 1,
            radius: 1.,
            lifetime: 0,
        }],
        0xd7f => &[Wave {
            kind: 0xd80,
            count: 20,
            radius: 3.,
            lifetime: 0,
        }],
        0xd82 => &[Wave {
            kind: 0xd81,
            count: 4,
            radius: 3.,
            lifetime: 0,
        }],
        0x944 => &[
            Wave {
                kind: 0xdf2,
                count: 1,
                radius: 2.,
                lifetime: 0,
            },
            Wave {
                kind: 0xde0,
                count: 1,
                radius: 2.,
                lifetime: 0,
            },
            Wave {
                kind: 0xde1,
                count: 1,
                radius: 2.,
                lifetime: 0,
            },
            Wave {
                kind: 0xde3,
                count: 1,
                radius: 2.,
                lifetime: 0,
            },
        ],
        0xde1 => &[Wave {
            kind: 0xde2,
            count: 1,
            radius: 1.,
            lifetime: 10_000,
        }],
        0xde3 => &[Wave {
            kind: 0xde4,
            count: 1,
            radius: 5.,
            lifetime: 10_000,
        }],
        kind if ROSTER.contains(&kind) => &[],
        _ => panic!("summons requested for unselected endgame object {kind:#x}"),
    }
}
pub fn validate(content: &Content) -> Result<()> {
    for kind in HEROES {
        ensure!(
            content.get(kind)?.quest,
            "closure hero {kind:#x} is not a selected-client quest"
        );
    }
    for kind in ROSTER {
        let object = content.get(*kind)?;
        for wave in summons(*kind) {
            ensure!(
                wave.count > 0 && wave.count <= 20 && wave.radius.is_finite() && wave.radius > 0.,
                "invalid bounded summon wave for {kind:#x}"
            );
            ensure!(
                content.get(wave.kind)?.enemy && ROSTER.contains(&wave.kind),
                "unhandled summon {:#x}",
                wave.kind
            );
        }
        ensure!(
            profile(*kind).is_some(),
            "endgame object {kind:#x} has no selected AI"
        );
        for elapsed in (0..110_000).step_by(100) {
            for hp in [object.hp, (object.hp / 10).max(1)] {
                for (index, angle) in attacks(*kind, hp, object.hp, 0., elapsed) {
                    ensure!(
                        angle.is_finite() && object.projectiles.contains_key(&index),
                        "endgame object {kind:#x} emits unsupported projectile {index}"
                    );
                }
            }
        }
    }
    for kind in [LOCKED_CELLAR, 0x700, 0xd7b] {
        ensure!(
            content.get(kind)?.class == "Portal",
            "endgame portal contract mismatch {kind:#x}"
        );
    }
    let inc = content.get(INCANTATION)?;
    ensure!(
        inc.activate == "UnlockPortal",
        "Incantation activation contract mismatch"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::{BTreeMap, VecDeque},
        path::Path,
    };
    #[test]
    fn finite_hero_credit_never_counts_summons_replays_or_despawns() {
        assert!(RealmCycle::new(BTreeSet::new()).is_err());
        let mut realm = RealmCycle::new(BTreeSet::from([7, 9])).unwrap();
        assert!(!realm.defeated(100, 0));
        assert!(realm.defeated(7, 100));
        assert!(!realm.defeated(7, 200));
        assert!(realm.admits());
        assert!(!realm.advance(1_000_000));
        assert!(realm.defeated(9, 200));
        assert!(!realm.admits());
        assert!(!realm.advance(60_199));
        assert!(realm.advance(60_200));
        assert!(!realm.advance(82_199));
        assert!(realm.advance(82_200));
        assert_eq!(realm.stage, RealmStage::Transferring);
        assert!(!realm.advance(200_000));
        assert!(!realm.defeated(9, 200_001));
    }
    #[test]
    fn selected_xml_covers_all_phase_projectiles_and_different_boss_patterns() {
        let content = Content::load(Path::new("data")).unwrap();
        validate(&content).unwrap();
        assert_eq!(oryx_phase(19_999), OryxPhase::Opening);
        assert_eq!(oryx_phase(20_000), OryxPhase::Dance);
        assert_eq!(oryx_phase(45_000), OryxPhase::Artifacts);
        assert_eq!(oryx_phase(70_000), OryxPhase::Gaze);
        assert_eq!(oryx_phase(80_000), OryxPhase::DanceTwo);
        assert_ne!(
            attacks(ORYX, 85_000, 85_000, 0., 0),
            attacks(ORYX2, 75_000, 75_000, 0., 0)
        );
        assert!(
            attacks(ORYX, 17_000, 85_000, 0., 0)
                .iter()
                .any(|(i, _)| *i == 16)
        );
    }
    #[test]
    fn boss_waves_are_phase_bound_and_oryx_two_rage_has_a_real_protected_transition() {
        let mut brain = BossBrain::default();
        let initial = brain.update(ORYX, 85_000, 85_000, 0).unwrap();
        assert!(initial.changed && !initial.invulnerable);
        assert_eq!((initial.waves[0].kind, initial.waves[0].count), (0x1749, 5));
        let dance = brain.update(ORYX, 80_000, 85_000, 20_000).unwrap();
        assert!(dance.invulnerable && dance.changed);
        assert_eq!(
            (
                dance.waves[0].kind,
                dance.waves[0].count,
                dance.waves[0].lifetime
            ),
            (0x1748, 15, 20_000)
        );
        assert!(
            brain
                .update(ORYX, 80_000, 85_000, 20_050)
                .unwrap()
                .waves
                .is_empty()
        );
        let artifacts = brain.update(ORYX, 75_000, 85_000, 45_000).unwrap();
        assert!(!artifacts.invulnerable && artifacts.waves.len() == 2);
        assert_eq!(oryx_phase(105_000), OryxPhase::Dance);
        assert!(brain.update(ORYX, 75_000, 85_000, 44_999).is_err());
        let rage = brain.update(ORYX, 17_000, 85_000, 45_050).unwrap();
        assert_eq!(rage.phase, OryxPhase::Rage);
        assert!(!rage.invulnerable && rage.waves[0].count == 10);
        let mut brain = BossBrain::default();
        let prepare = brain.update(ORYX2, 15_000, 75_000, 500).unwrap();
        assert_eq!(prepare.phase, OryxPhase::PrepareRage);
        assert!(prepare.invulnerable && prepare.waves.is_empty());
        assert!(
            brain
                .update(ORYX2, 16_000, 75_000, 10_499)
                .unwrap()
                .invulnerable
        );
        let rage = brain.update(ORYX2, 16_000, 75_000, 10_500).unwrap();
        assert_eq!(rage.phase, OryxPhase::Rage);
        assert!(!rage.invulnerable && rage.waves.is_empty());
    }
    #[test]
    fn surviving_guardian_requires_a_real_bound_sword_defeat_and_recovery() {
        let mut brain = GuardianBrain::default();
        assert!(!brain.update(true, 100).unwrap().invulnerable);
        assert!(brain.bind_sword(50).is_err());
        assert!(brain.update(false, 200).unwrap().invulnerable);
        assert!(!brain.update(false, 4199).unwrap().summon_sword);
        let ready = brain.update(false, 4200).unwrap();
        assert!(ready.summon_sword && ready.heal);
        assert!(brain.update(false, 4250).unwrap().summon_sword);
        brain.bind_sword(50).unwrap();
        assert!(brain.bind_sword(51).is_err());
        assert!(brain.update(false, 20_000).unwrap().invulnerable);
        assert!(!brain.update(false, 20_000).unwrap().summon_sword);
        assert!(brain.sword_defeated(51, 20_000).is_err());
        brain.sword_defeated(50, 20_000).unwrap();
        assert!(brain.sword_defeated(50, 20_000).is_err());
        assert!(brain.update(false, 21_399).unwrap().invulnerable);
        assert!(!brain.update(false, 21_400).unwrap().invulnerable);
    }
    #[test]
    fn complete_maps_preserve_supported_rosters_and_connected_spawn_boss_routes() {
        let content = Content::load(Path::new("data")).unwrap();
        for (template, bosses) in [
            (CASTLE, GUARDIANS.as_slice()),
            (CHAMBER, &[ORYX][..]),
            (CELLAR, &[ORYX2][..]),
        ] {
            let map = Map::load(
                &Path::new("data").join(format!("{template}.wmap")),
                &content,
            )
            .unwrap();
            let mut counts = BTreeMap::new();
            for cell in &map.cells {
                if let Some(kind) = cell.object {
                    *counts.entry(kind).or_insert(0) += 1;
                }
            }
            for boss in bosses {
                assert_eq!(counts[boss], 1);
            }
            if template == CASTLE {
                assert_eq!(counts[&0xd83], 103);
                assert_eq!(counts[&0xd7f], 6);
            }
            if template == CELLAR {
                assert_eq!(counts[&0x944], 44);
            }
            for cell in &map.cells {
                if let Some(kind) = cell.object {
                    if content.objects[&kind].enemy {
                        assert!(
                            profile(kind).is_some(),
                            "{template} contains unhandled enemy {kind:#x}"
                        );
                    }
                }
            }
            let (x, y) = map.spawns[0];
            let start = (y as i32 * map.width + x as i32) as usize;
            let mut visited = BTreeSet::from([start]);
            let mut queue = VecDeque::from([start]);
            while let Some(i) = queue.pop_front() {
                let (x, y) = (i as i32 % map.width, i as i32 / map.width);
                for (x, y) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
                    if x < 0 || y < 0 || x >= map.width || y >= map.height {
                        continue;
                    }
                    let n = (y * map.width + x) as usize;
                    let c = &map.cells[n];
                    if content.grounds[&c.ground] > 0.
                        && c.object.is_none_or(|k| {
                            !content.objects[&k].blocks || content.objects[&k].destructible
                        })
                        && visited.insert(n)
                    {
                        queue.push_back(n);
                    }
                }
            }
            for (i, c) in map.cells.iter().enumerate() {
                if c.object.is_some_and(|k| bosses.contains(&k)) {
                    assert!(visited.contains(&i), "{template} boss is unreachable");
                }
            }
        }
    }
}
