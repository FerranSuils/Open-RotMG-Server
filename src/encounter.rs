#[derive(Clone, Copy)]
pub enum Motion {
    Wander,
    Chase { sight: f32, stop: f32 },
    Circle,
}
#[derive(Clone, Copy)]
pub struct Profile {
    pub motion: Motion,
    pub speed: f32,
    pub interval: u64,
    pub range: f32,
}
pub fn profile(kind: u16) -> Option<Profile> {
    let (motion, speed, interval, range) = match kind {
        0x600 => (
            Motion::Chase {
                sight: 6.,
                stop: 0.,
            },
            8.5,
            2500,
            3.,
        ),
        0x60e => (Motion::Wander, 4., 1000, 5.),
        0x60f => (Motion::Wander, 4., 600, 5.),
        0x610 => (Motion::Wander, 4., 1800, 5.),
        0x683..=0x686 => (Motion::Circle, 6., 500, 100.),
        0x687..=0x689 => (
            Motion::Chase {
                sight: 11.,
                stop: 1.,
            },
            8.,
            500,
            100.,
        ),
        0x68a..=0x68f => (Motion::Wander, 2., 0, 0.),
        0x927 => (Motion::Wander, 2., 500, 10.),
        _ => return None,
    };
    Some(Profile {
        motion,
        speed,
        interval,
        range,
    })
}
pub fn attacks(kind: u16, hp: i32, heading: f32) -> Vec<(u8, f32)> {
    match kind {
        0x927 if hp < 640 => vec![(0, heading), (1, heading)],
        0x927 | 0x68a..=0x68f => vec![],
        0x60e => vec![
            (0, heading - 5f32.to_radians()),
            (0, heading + 5f32.to_radians()),
        ],
        0x610 => (0..5)
            .map(|i| (0, i as f32 * std::f32::consts::TAU / 5.))
            .collect(),
        0x600 | 0x60f | 0x683..=0x689 => vec![(0, heading)],
        _ => panic!("attack requested without a bounded encounter profile"),
    }
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
