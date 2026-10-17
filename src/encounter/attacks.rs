#[derive(Clone, Copy, Debug)]
pub(crate) enum Aim {
    Target,
    Fixed(f32),
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum Prediction {
    None,
    Lead(f32),
    Chance(f64),
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum Action {
    Shoot { projectile: u8, choice: Option<u8> },
    Grenade { radius: f32, damage: i32 },
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Attack {
    pub action: Action,
    pub aim: Aim,
    pub count: usize,
    pub gap: f32,
    pub offset: f32,
    pub interval: u64,
    pub delay: u64,
    pub range: f32,
    pub predict: Prediction,
    pub enabled: bool,
}
impl Attack {
    fn fan(projectile: u8, count: usize, gap: f32, interval: u64, range: f32) -> Self {
        Self {
            action: Action::Shoot {
                projectile,
                choice: None,
            },
            aim: Aim::Target,
            count,
            gap: gap.to_radians(),
            offset: 0.,
            interval,
            delay: 0,
            range,
            predict: Prediction::None,
            enabled: true,
        }
    }
    fn ring(projectile: u8, count: usize, interval: u64, range: f32) -> Self {
        Self::fan(projectile, count, 360. / count as f32, interval, range)
            .offset(180. - 180. / count as f32)
    }
    fn grenade(radius: f32, damage: i32, interval: u64, range: f32) -> Self {
        Self {
            action: Action::Grenade { radius, damage },
            ..Self::fan(0, 1, 0., interval, range)
        }
    }
    fn offset(mut self, degrees: f32) -> Self {
        self.offset += degrees.to_radians();
        self
    }
    fn delay(mut self, ms: u64) -> Self {
        self.delay = ms;
        self
    }
    fn predict(mut self, factor: f32) -> Self {
        self.predict = Prediction::Lead(factor);
        self
    }
    fn chance(mut self, chance: f64) -> Self {
        self.predict = Prediction::Chance(chance);
        self
    }
    fn fixed(mut self, angle: f32) -> Self {
        self.aim = Aim::Fixed(angle);
        self
    }
    fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    fn choice(mut self, index: u8) -> Self {
        let Action::Shoot { choice, .. } = &mut self.action else {
            unreachable!()
        };
        *choice = Some(index);
        self
    }
    pub fn shots(self, projectile: u8, heading: f32) -> Vec<(u8, f32)> {
        let heading = match self.aim {
            Aim::Target => heading,
            Aim::Fixed(angle) => angle,
        };
        (0..self.count)
            .map(|i| {
                (
                    projectile,
                    heading + self.offset + (i as f32 - (self.count - 1) as f32 / 2.) * self.gap,
                )
            })
            .collect()
    }
}
#[derive(Debug)]
pub(crate) struct Schedule {
    pub state: u64,
    pub age: u64,
    pub attacks: Vec<Attack>,
}
impl Schedule {
    pub fn snapshot(&self, heading: f32) -> Vec<(u8, f32)> {
        self.attacks
            .iter()
            .filter(|a| {
                a.enabled && self.age >= a.delay && (self.age - a.delay).is_multiple_of(a.interval)
            })
            .flat_map(|a| match a.action {
                Action::Shoot { projectile, .. } => a.shots(projectile, heading),
                Action::Grenade { .. } => vec![],
            })
            .collect()
    }
}
pub(crate) fn schedule(kind: u16, hp: i32, elapsed: u64) -> Option<Schedule> {
    let fan = Attack::fan;
    let ring = Attack::ring;
    let grenade = Attack::grenade;
    let mut state = 0;
    let mut age = elapsed;
    let attacks = match kind {
        0x954 => vec![fan(0, 1, 0., 2000, 8.).choice(1)],
        0x60c => vec![fan(0, 1, 0., 5000, 10.), fan(1, 1, 0., 400, 5.)],
        0x953 => vec![fan(0, 1, 0., 1000, 8.), grenade(2., 12, 2500, 5.)],
        0x614 => {
            vec![
                fan(0, 1, 0., 1000, 10.),
                fan(1, 1, 0., if hp <= 182 { 1000 } else { 1300 }, 10.),
            ]
        }
        0x609 => {
            state = elapsed / 2000;
            age = elapsed % 2000;
            match state % 3 {
                0 => vec![fan(0, 3, 14., 1000, 10.).chance(0.3)],
                1 => vec![fan(0, 3, 10., 1000, 10.).chance(0.5)],
                _ => vec![],
            }
        }
        0x617 | 0x61a => {
            let mut attacks = (0..3)
                .map(|i| {
                    ring(i, 15, 1200, 25.)
                        .fixed(i as f32 * 8f32.to_radians())
                        .delay(i as u64 * 400)
                })
                .collect::<Vec<_>>();
            if kind == 0x61a {
                attacks.push(fan(3, 1, 0., 1000, 12.));
            }
            attacks
        }
        0x632 => vec![
            fan(0, 3, 15., 2000, 10.),
            fan(1, 1, 0., 2000, 10.).delay(1000),
        ],
        0x633 => {
            state = elapsed / 5000;
            age = elapsed % 5000;
            vec![fan(
                (state % 2) as u8,
                1,
                0.,
                if state.is_multiple_of(2) { 250 } else { 500 },
                10.,
            )]
        }
        0x640 => vec![
            fan(0, 3, 7., 1000, 10.),
            fan(1, 1, 0., 1000, 10.).delay(500).predict(1.),
        ],
        0x645 => vec![
            fan(0, 1, 0., 1000, 10.),
            grenade(2., 55, 1000, 5.).delay(500),
        ],
        0x649 => vec![
            fan(0, 1, 0., 2000, 10.).predict(0.5),
            fan(1, 1, 0., 2000, 10.).delay(1000).predict(1.),
        ],
        0xd05 => vec![fan(0, 4, 10., 1000, 12.), fan(1, 1, 0., 1000, 12.)],
        0x652 => vec![
            fan(0, 4, 10., 1000, 12.).predict(1.),
            fan(1, 1, 0., 1000, 10.).predict(1.),
        ],
        0x656 => vec![
            ring(0, 5, 750, 12.).predict(0.5),
            fan(1, 1, 0., 1000, 10.).predict(1.),
        ],
        0x658 => vec![
            fan(0, 5, 10., 1000, 12.).predict(1.),
            fan(1, 1, 0., 650, 10.).predict(1.),
        ],
        0x91a if hp >= 400 => {
            let phase = elapsed % 3750;
            state = elapsed / 3750 * 3
                + if phase < 2500 {
                    0
                } else if phase > 3500 {
                    2
                } else {
                    1
                };
            if phase < 2500 {
                age = phase;
                vec![]
            } else {
                age = phase - 2500;
                let step = (age.saturating_sub(200) / 200).min(4) as f32;
                if age > 1000 {
                    vec![]
                } else {
                    vec![
                        ring(0, 4, 200, 10.)
                            .fixed((step * 10.).to_radians())
                            .delay(200),
                        ring(0, 4, 200, 10.)
                            .fixed((90. - step * 10.).to_radians())
                            .delay(200),
                    ]
                }
            }
        }
        0x6d8 => {
            state = elapsed / 1500;
            age = elapsed % 1500;
            match state % 3 {
                0 => (0..3)
                    .map(|i| fan(0, 3, 10., 300, 15.).offset(i as f32 * 120.))
                    .collect(),
                1 => (0..3)
                    .map(|i| fan(0, 4, 10., 300, 15.).offset(60. + i as f32 * 120.))
                    .collect(),
                _ => vec![fan(1, 2, 15., 100, 15.).predict(1.)],
            }
        }
        0x906 | 0x907 if hp < 400 => vec![ring(0, 8, 50, 0.).fixed(0.)],
        0x917 => {
            state = super::stage(kind, hp, elapsed);
            match state {
                1 => vec![],
                2 => vec![ring(1, 8, 500, 10.)],
                _ => (0..4)
                    .map(|i| fan(0, 2, 10., 1000, 0.).fixed(i as f32 * std::f32::consts::FRAC_PI_2))
                    .collect(),
            }
        }
        0xd06 => {
            let t = elapsed % 43300;
            let cycle = elapsed / 43300 * 6;
            match t {
                0..2000 => {
                    state = cycle;
                    age = t;
                    vec![]
                }
                2000..2300 => {
                    state = cycle + 1;
                    age = t - 2000;
                    vec![]
                }
                2300..12300 => {
                    state = cycle + 2;
                    age = t - 2300;
                    vec![
                        fan(0, 2, 10., 400, 8.).offset(0.7).chance(0.4),
                        fan(0, 2, 180., 400, 8.).offset(0.7).chance(0.4),
                        fan(0, 18, 20., 1500, 8.)
                            .offset(0.4)
                            .chance(0.4)
                            .enabled(false),
                    ]
                }
                12300..32300 => {
                    let local = (t - 12300) % 5000;
                    if local < 1500 {
                        state = cycle + 3;
                        age = local;
                        vec![
                            fan(0, 1, 0., 1000, 8.).chance(0.1),
                            fan(0, 3, 120., 500, 8.).offset(0.3).chance(0.1),
                        ]
                    } else {
                        state = cycle + 4;
                        age = local - 1500;
                        vec![fan(0, 3, 120., 800, 8.).offset(0.3).chance(0.2)]
                    }
                }
                _ => {
                    state = cycle + 5;
                    age = t - 32300;
                    vec![fan(0, 3, 15., 1300, 8.)]
                }
            }
        }
        0xd07..=0xd0a => {
            let corner: f32 = match kind {
                0xd07 => 90.,
                0xd08 => 0.,
                0xd09 => 270.,
                0xd0a => 180.,
                _ => unreachable!(),
            };
            if elapsed < 2000 {
                vec![]
            } else {
                state = if elapsed < 8000 {
                    1
                } else if elapsed < 14000 {
                    2
                } else {
                    3
                };
                age = elapsed - [0, 2000, 8000, 14000][state as usize];
                let mut attacks = vec![
                    fan(0, 1, 0., 300, 0.).fixed(corner.to_radians()),
                    fan(0, 1, 0., 300, 0.).fixed((corner + 90.).to_radians()),
                ];
                if state == 2 {
                    attacks.push(fan(0, 1, 0., 300, 0.).fixed((corner + 45.).to_radians()));
                }
                attacks
            }
        }
        0xd90 => {
            let phase = elapsed / 13000;
            let local = elapsed % 13000;
            state = phase * 2 + u64::from(local >= 3000);
            if local < 3000 {
                age = local;
                vec![]
            } else {
                age = local - 3000;
                match phase % 4 {
                    0 => (0..5)
                        .map(|i| {
                            fan(0, 3, 120., 1000, 0.)
                                .fixed((i as f32 * 24.).to_radians())
                                .delay(i * 200)
                        })
                        .collect(),
                    1 => vec![fan(4, 12, 30., 2000, 10.)],
                    2 => vec![
                        fan(1, 8, 45., 1000, 10.),
                        fan(1, 8, 45., 1000, 10.).offset(22.5).delay(500),
                        fan(2, 3, 20., 2000, 8.),
                    ],
                    _ => vec![fan(1, 3, 10., 1000, 8.)],
                }
            }
        }
        0xd99 => vec![fan(0, 3, 20., 1000, 10.), fan(1, 1, 0., 1000, 10.)],
        0xd9a => vec![
            fan(0, 3, 20., 1000, 10.),
            fan(1, 3, 20., 2000, 10.).delay(100),
        ],
        0xda5 | 0xda6 => vec![fan(0, 1, 0., 500, 10.), fan(0, 1, 0., 1000, 3.)],
        _ => return None,
    };
    Some(Schedule {
        state,
        age,
        attacks,
    })
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Clocks {
    state: Option<u64>,
    deadlines: Vec<Option<u64>>,
}
impl Clocks {
    pub fn due(&mut self, schedule: &Schedule, now: u64) -> Vec<usize> {
        if self.state != Some(schedule.state) {
            self.state = Some(schedule.state);
            self.deadlines = vec![None; schedule.attacks.len()];
        }
        assert_eq!(
            self.deadlines.len(),
            schedule.attacks.len(),
            "attack channels changed without a phase transition"
        );
        self.deadlines
            .iter_mut()
            .enumerate()
            .filter_map(|(i, deadline)| {
                let attack = schedule.attacks[i];
                if !attack.enabled {
                    *deadline = None;
                    return None;
                }
                let deadline = deadline.get_or_insert(now + attack.delay);
                if now < *deadline {
                    return None;
                }
                *deadline = now + attack.interval;
                Some(i)
            })
            .collect()
    }
    pub fn suspend(&mut self, delta: u64) {
        for deadline in self.deadlines.iter_mut().flatten() {
            *deadline += delta;
        }
    }
}
