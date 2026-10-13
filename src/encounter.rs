use crate::content::Content;
use anyhow::{Result, ensure};

#[derive(Clone, Copy)]
pub enum Anchor {
    Player,
    Parent,
    Kind(u16),
    Home,
}
#[derive(Clone, Copy)]
pub enum Motion {
    Stationary,
    Wander,
    Chase { sight: f32, stop: f32 },
    Circle,
    Orbit { anchor: Anchor, radius: f32 },
    Protect { anchor: Anchor, distance: f32 },
}
#[derive(Clone, Copy)]
pub struct Profile {
    pub motion: Motion,
    pub speed: f32,
    pub interval: u64,
    pub range: f32,
}
pub fn profile(kind: u16) -> Option<Profile> {
    use Motion::*;
    let chase = |stop| Chase { sight: 10., stop };
    let (motion, speed, interval, range) = match kind {
        0x172 | 0x173 | 0x175..=0x179 => (Stationary, 0., 0, 0.),
        0x600 => (
            Chase {
                sight: 6.,
                stop: 0.,
            },
            8.5,
            2500,
            3.,
        ),
        0x60e => (Wander, 4., 1000, 5.),
        0x60f => (Wander, 4., 600, 5.),
        0x610 => (Wander, 4., 1800, 5.),
        0x683..=0x686 => (Circle, 6., 500, 100.),
        0x687..=0x689 => (
            Chase {
                sight: 11.,
                stop: 1.,
            },
            8.,
            500,
            100.,
        ),
        0x68a..=0x68f => (Wander, 2., 0, 0.),
        0x927 => (Wander, 2., 500, 10.),
        0x605 | 0x60a | 0x616 => (chase(4.), 8., 1000, 10.),
        0x606 | 0x615 => (chase(1.), 8., 1000, 3.),
        0x607 => (chase(6.), 6., 300, 10.),
        0x60c => (chase(4.), 6., 1000, 10.),
        0x60d => (chase(4.), 6., 10000, 10.),
        0x609 => (chase(2.), 6., 1000, 10.),
        0x617 => (chase(4.), 7.5, 400, 15.),
        0x61c | 0x61e | 0x61f | 0x63a | 0x63b | 0x63e => (chase(1.), 10., 1000, 3.),
        0x61d | 0x63c => (chase(7.), 10., 1000, 10.),
        0x63f => (Wander, 4., 1000, 10.),
        0x624 | 0xd00 => (Wander, 15., 300, 10.),
        0x625 | 0x626 | 0xd01..=0xd04 => (Wander, 15., 1000, 10.),
        0x632 => (chase(1.), 10., 1000, 10.),
        0x633 => (chase(1.), 10., 250, 10.),
        0x640 => (chase(1.), 10., 500, 10.),
        0x642 | 0x645 => (chase(8.), 4., 500, 10.),
        0x643 => (chase(1.), 6., 1000, 3.),
        0x644 => (chase(8.), 4., 200, 10.),
        0x646 => (chase(1.), 10., 500, 10.),
        0x64a..=0x64d | 0x64f => (chase(7.), 12., 500, 10.),
        0x650 => (chase(1.), 10., 500, 10.),
        0x651 | 0x654 => (chase(4.), 6., 1000, 12.),
        0x652 | 0xd05 => (Wander, 2., 1000, 12.),
        0x653 => (
            Protect {
                anchor: Anchor::Parent,
                distance: 2.,
            },
            4.,
            0,
            0.,
        ),
        0x655 => (chase(4.), 10., 1250, 12.),
        0x656 => (chase(4.), 4., 750, 12.),
        0x657 => (chase(4.), 6., 500, 12.),
        0x658 => (chase(4.), 10., 650, 12.),
        0x659 => (chase(4.), 4., 900, 12.),
        0x223 | 0x224 => (Wander, 8., 1000, 10.),
        0x225 | 0x227 => (chase(1.), 10., 1000, 10.),
        0x226 => (Wander, 5., 1000, 10.),
        0x228 => (chase(5.), 10., 1000, 10.),
        0x229 => (chase(5.), 10., 300, 10.),
        0x917 => (Stationary, 0., 500, 15.),
        0x918 => (
            Orbit {
                anchor: Anchor::Kind(0x917),
                radius: 20.,
            },
            10.,
            1000,
            20.,
        ),
        0x919 => (
            Protect {
                anchor: Anchor::Parent,
                distance: 2.,
            },
            4.,
            1000,
            20.,
        ),
        0xd06 => (Wander, 3., 400, 12.),
        0xd07..=0xd0a => (Stationary, 0., 300, 100.),
        0xd90 => (Wander, 0.5, 150, 12.),
        0xd91..=0xd93 => (chase(1.), 10., 1000, 6.),
        0xd94..=0xd98 => (chase(7.), 9., 1000, 10.),
        0xd99 | 0xd9a => (chase(1.), 8., 1000, 10.),
        0xd9b..=0xd9d => (Wander, 1., 1000, 10.),
        0xd9e | 0xd9f => (Wander, 1., 500, 10.),
        0xda2 | 0xda3 => (Stationary, 0., 1000, 3.),
        0xda4 => (chase(1.), 12., 1000, 10.),
        0xda5 | 0xda6 => (chase(7.), 10., 1000, 10.),
        0xdaa | 0xdac => (chase(7.), 9., 1000, 8.),
        0xdab | 0xdad..=0xdaf | 0xdb0 | 0xdb2 => (chase(1.), 12., 1000, 10.),
        0xdb1 => (
            Protect {
                anchor: Anchor::Parent,
                distance: 6.,
            },
            10.,
            1000,
            10.,
        ),
        _ => return None,
    };
    Some(Profile {
        motion,
        speed,
        interval,
        range,
    })
}
pub fn stage(kind: u16, hp: i32, elapsed: u64) -> u64 {
    match kind {
        0x917 => {
            if hp >= 5625 {
                0
            } else if hp >= 3750 {
                1
            } else if hp >= 1875 {
                2
            } else {
                3
            }
        }
        0xd06 => {
            elapsed / 43300 * 5
                + match elapsed % 43300 {
                    0..2000 => 0,
                    2000..2300 => 1,
                    2300..12300 => 2,
                    12300..32300 => 3,
                    _ => 4,
                }
        }
        0xd90 => elapsed / 13000,
        _ => 0,
    }
}
pub fn movement(kind: u16, hp: i32, elapsed: u64) -> (Motion, f32) {
    let p = profile(kind).expect("validated encounter motion");
    match kind {
        0x917 if (3750..5625).contains(&hp) => (Motion::Wander, 3.),
        0xd06 => match stage(kind, hp, elapsed) % 5 {
            0 | 4 => (Motion::Wander, 3.),
            1 => (
                Motion::Chase {
                    sight: 10.,
                    stop: 2.,
                },
                36.,
            ),
            2 => (
                Motion::Orbit {
                    anchor: Anchor::Player,
                    radius: 4.,
                },
                10.8,
            ),
            3 => (Motion::Stationary, 0.),
            _ => unreachable!(),
        },
        _ => (p.motion, p.speed),
    }
}
fn spread(index: u8, count: usize, gap: f32, heading: f32) -> Vec<(u8, f32)> {
    (0..count)
        .map(|i| {
            (
                index,
                heading + (i as f32 - (count - 1) as f32 / 2.) * gap.to_radians(),
            )
        })
        .collect()
}
fn ring(index: u8, count: usize, offset: f32) -> Vec<(u8, f32)> {
    (0..count)
        .map(|i| {
            (
                index,
                offset + i as f32 * std::f32::consts::TAU / count as f32,
            )
        })
        .collect()
}
#[cfg(test)]
pub fn attacks(kind: u16, hp: i32, heading: f32) -> Vec<(u8, f32)> {
    phased_attacks(kind, hp, heading, 0)
}
pub fn phased_attacks(kind: u16, hp: i32, heading: f32, elapsed: u64) -> Vec<(u8, f32)> {
    match kind {
        0x927 if hp < 640 => vec![(0, heading), (1, heading)],
        0x172 | 0x173 | 0x175..=0x179 | 0x927 | 0x68a..=0x68f | 0x653 => vec![],
        0x60e => spread(0, 2, 10., heading),
        0x610 => ring(0, 5, 0.),
        0x600
        | 0x60f
        | 0x683..=0x689
        | 0x605..=0x607
        | 0x60a
        | 0x60c
        | 0x60d
        | 0x615
        | 0x616
        | 0x61c..=0x61f
        | 0x63a..=0x63c
        | 0x63e
        | 0x63f
        | 0x642..=0x645
        | 0x64a..=0x64d
        | 0x64f
        | 0x223
        | 0x224
        | 0x226
        | 0x227
        | 0x229
        | 0x918
        | 0x919
        | 0xd91..=0xd94
        | 0xd96
        | 0xd97
        | 0xd9b..=0xd9d
        | 0xda4
        | 0xdaa..=0xdb2 => vec![(0, heading)],
        0x609 => spread(
            0,
            3,
            if (elapsed / 1000).is_multiple_of(2) {
                14.
            } else {
                10.
            },
            heading,
        ),
        0x617 => {
            let phase = (elapsed / 400) % 3;
            ring(phase as u8, 15, phase as f32 * 8f32.to_radians())
        }
        0x624 | 0xd00 => spread(0, 2, 7., heading),
        0x625 | 0xd01 => spread(0, 3, 7., heading),
        0x626 | 0xd02 => spread(0, 4, 7., heading),
        0xd03 | 0xd04 => spread(0, 5, 7., heading),
        0x632 => {
            if (elapsed / 1000).is_multiple_of(2) {
                spread(0, 3, 15., heading)
            } else {
                vec![(1, heading)]
            }
        }
        0x633 => vec![(u8::from(elapsed % 10000 >= 5000), heading)],
        0x640 => {
            if (elapsed / 500).is_multiple_of(2) {
                spread(0, 3, 7., heading)
            } else {
                vec![(1, heading)]
            }
        }
        0x646 => {
            if (elapsed / 500).is_multiple_of(3) {
                ring(0, 20, 0.)
            } else {
                vec![(0, heading)]
            }
        }
        0x650 => spread(0, 3, 7., heading),
        0x651 => spread(0, 3, 15., heading),
        0x652 | 0xd05 => {
            let mut shots = spread(0, 4, 10., heading);
            shots.push((1, heading));
            shots
        }
        0x654 | 0x655 => spread(0, 5, 10., heading),
        0x656 => {
            let mut shots = ring(0, 12, heading);
            shots.push((1, heading));
            shots
        }
        0x657 => ring(0, 5, heading),
        0x658 => {
            if (elapsed / 650).is_multiple_of(2) {
                spread(0, 5, 10., heading)
            } else {
                vec![(1, heading)]
            }
        }
        0x659 => spread(0, 7, 25., heading),
        0x228 | 0xd95 | 0xd98 | 0xd9e | 0xd9f => spread(0, 3, 6., heading),
        0x225 => spread(0, 3, 6., heading),
        0x917 if (3750..5625).contains(&hp) => vec![],
        0x917 if (1875..3750).contains(&hp) => ring(1, 8, elapsed as f32 / 1000.),
        0x917 => (0..4)
            .flat_map(|i| spread(0, 2, 10., i as f32 * std::f32::consts::FRAC_PI_2))
            .collect(),
        0xd06 if elapsed % 43300 < 2300 => vec![],
        0xd06 if elapsed % 43300 < 12300 => {
            let mut shots = spread(0, 2, 10., heading);
            shots.extend(spread(0, 2, 10., heading + std::f32::consts::PI));
            shots
        }
        0xd06 if elapsed % 43300 < 32300 => ring(0, 3, elapsed as f32 / 1000.),
        0xd06 => spread(0, 3, 15., heading),
        0xd07..=0xd0a => {
            let corner: f32 = match kind {
                0xd07 => 90.,
                0xd08 => 0.,
                0xd09 => 270.,
                0xd0a => 180.,
                _ => unreachable!(),
            };
            if elapsed < 2000 {
                return vec![];
            }
            let mut shots = vec![(0, corner.to_radians()), (0, (corner + 90.).to_radians())];
            if (elapsed - 2000) / 6000 == 1 {
                shots.push((0, (corner + 45.).to_radians()));
            }
            shots
        }
        0xd90 if elapsed % 13000 < 3000 => vec![],
        0xd90 => match (elapsed / 13000) % 4 {
            0 => ring(0, 3, ((elapsed / 150) % 20) as f32 * 18f32.to_radians()),
            1 => ring(4, 12, 0.),
            2 => {
                let mut shots = ring(1, 8, elapsed as f32 / 1000.);
                shots.extend(spread(2, 3, 20., heading));
                shots
            }
            3 => spread(1, 3, 10., heading),
            _ => unreachable!(),
        },
        0xd99 | 0xd9a => {
            let mut shots = vec![(0, heading)];
            shots.extend(ring(1, 8, 0.));
            shots
        }
        0xda2 | 0xda3 => ring(0, 12, heading),
        0xda5 | 0xda6 => vec![(u8::from((elapsed / 4000) % 2 == 1), heading)],
        _ => panic!("attack requested without an explicit encounter profile"),
    }
}
pub fn validate(content: &Content, kind: u16) -> Result<()> {
    ensure!(
        profile(kind).is_some(),
        "missing encounter profile for {kind:#x}"
    );
    for hp in [1, 2000, 4000, 6000, content.get(kind)?.hp] {
        for elapsed in (0..60000).step_by(500) {
            for (index, _) in phased_attacks(kind, hp, 0., elapsed) {
                ensure!(
                    content.get(kind)?.projectiles.contains_key(&index),
                    "encounter {kind:#x} references absent selected projectile {index}"
                );
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn candidate_profiles_preserve_boss_threshold_and_cube_patterns() {
        assert!(attacks(0x927, 640, 0.).is_empty());
        assert_eq!(attacks(0x927, 639, 0.4), vec![(0, 0.4), (1, 0.4)]);
        assert_eq!(attacks(0x610, 70, 0.).len(), 5);
        let red = attacks(0x60e, 70, 0.);
        assert!((red[1].1 - red[0].1 - 10f32.to_radians()).abs() < 0.00001);
        assert!(profile(0x601).is_none());
        assert!(attacks(0x68a, 5, 0.).is_empty());
    }
}
