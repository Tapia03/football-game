//! `cargo run --release -p fm-match --features diagnostics --example
//! forced_release` — why half the passes leave at the forced release (the
//! 5.5 s deadlock guard): for every spell on the ball that ends in a pass,
//! the value of the carrier's options at each decision it made
//! (`DecisionSystem::option_values`), and whether the pass left by value or
//! at the guard. A forced spell is then one of:
//!
//! - **no option**: at no decision was any pass worth more than zero;
//! - **pass below holding**: a pass was worth more than zero at some
//!   decision, but never more than keeping the ball still;
//! - **chose to carry**: a pass beat holding at some decision, but
//!   dribbling (or a shot) was worth more;
//! - **pass was best, not played**: the pass was the best option at some
//!   decision and still was not played (sanity bucket).
//!
//! Observed from outside after each tick: the values are those of the next
//! decision (same positions, one more tick on the ball). No engine change.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::many_single_char_names
)]
use fm_core::Vec2;
use fm_match::state::{BallState, FlightIntent};
use fm_match::{DecisionSystem, Formation, MatchEngine, Role, TickFrame};

const FORCED: [&str; 4] = [
    "no option (no pass worth > 0)",
    "pass below holding",
    "chose to carry (pass > hold)",
    "pass was best, not played",
];

/// One spell on the ball.
#[derive(Clone, Copy)]
struct Spell {
    holder: usize,
    start: Vec2,
    decisions: u32,
    pass_positive: u32,
    pass_beats_hold: u32,
    pass_best: u32,
    /// Decisions won by [hold | shot | pass | dribble].
    won: [u32; 4],
    pressed: u32,
    /// Sums over the decisions: hold, best pass, dribble, pass success.
    sum: [f64; 4],
    nobody_in_range: u32,
    /// Decisions with a dribble value (keepers do not dribble) and with a
    /// pass candidate: the denominators of the dribble and pass means.
    dribblers: u32,
    passers: u32,
    /// Decisions where the dribble was worth more than the best pass.
    dribble_over_pass: u32,
}

#[derive(Default, Clone, Copy)]
struct Group {
    dribblers: u64,
    passers: u64,
    dribble_over_pass: u64,
    spells: u64,
    decisions: u64,
    won: [u64; 4],
    pressed: u64,
    sum: [f64; 4],
    pass_positive: u64,
    nobody_in_range: u64,
    carried: f64,
    /// Release in the own / middle / final third.
    third: [u64; 3],
    /// Released by a keeper / defender / midfielder / forward.
    line: [u64; 4],
}

impl Group {
    fn add(&mut self, sp: &Spell, carried: f32, third: usize, line: usize) {
        self.spells += 1;
        self.decisions += u64::from(sp.decisions);
        for k in 0..4 {
            self.won[k] += u64::from(sp.won[k]);
            self.sum[k] += sp.sum[k];
        }
        self.pressed += u64::from(sp.pressed);
        self.pass_positive += u64::from(sp.pass_positive);
        self.nobody_in_range += u64::from(sp.nobody_in_range);
        self.dribblers += u64::from(sp.dribblers);
        self.passers += u64::from(sp.passers);
        self.dribble_over_pass += u64::from(sp.dribble_over_pass);
        self.carried += f64::from(carried);
        self.third[third] += 1;
        self.line[line] += 1;
    }

    fn print(&self, label: &str, matches: u64) {
        let d = self.decisions.max(1) as f64;
        let s = self.spells.max(1) as f64;
        println!(
            "  {label}: {:.0} spells/match, {:.1} decisions each, carried {:.1} m",
            self.spells as f64 / matches as f64,
            d / s,
            self.carried / s
        );
        println!(
            "    decisions won by [hold | shot | pass | dribble]: {:.0}% | {:.0}% | {:.0}% | {:.0}% ; pressed at {:.0}%",
            100.0 * self.won[0] as f64 / d,
            100.0 * self.won[1] as f64 / d,
            100.0 * self.won[2] as f64 / d,
            100.0 * self.won[3] as f64 / d,
            100.0 * self.pressed as f64 / d
        );
        println!(
            "    mean value x1000 [hold | best pass | dribble]: {:.2} | {:.2} | {:.2} ; dribble worth more than the best pass at {:.0}% ; best pass worth > 0 at {:.0}% of decisions (estimated success {:.0}%), nobody in range at {:.0}%",
            1000.0 * self.sum[0] / d,
            1000.0 * self.sum[1] / self.passers.max(1) as f64,
            1000.0 * self.sum[2] / self.dribblers.max(1) as f64,
            100.0 * self.dribble_over_pass as f64 / d,
            100.0 * self.pass_positive as f64 / d,
            100.0 * self.sum[3] / d,
            100.0 * self.nobody_in_range as f64 / d
        );
        println!(
            "    released in the own / middle / final third: {:.0}% | {:.0}% | {:.0}% ; by keeper / defender / midfielder / forward: {:.0}% | {:.0}% | {:.0}% | {:.0}%",
            100.0 * self.third[0] as f64 / s,
            100.0 * self.third[1] as f64 / s,
            100.0 * self.third[2] as f64 / s,
            100.0 * self.line[0] as f64 / s,
            100.0 * self.line[1] as f64 / s,
            100.0 * self.line[2] as f64 / s,
            100.0 * self.line[3] as f64 / s
        );
    }
}

fn line_of(role: Role) -> usize {
    match role {
        Role::Goalkeeper => 0,
        Role::CentreBack | Role::FullBack | Role::WingBack => 1,
        Role::DefensiveMidfielder
        | Role::CentralMidfielder
        | Role::WideMidfielder
        | Role::AttackingMidfielder => 2,
        Role::Winger | Role::Striker => 3,
    }
}

fn main() {
    let n: u64 = std::env::var("FM_MATCHES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);
    for (name, home, away) in [
        ("4-4-2 (home) v 4-3-3", Formation::F442, Formation::F433),
        ("4-3-3 (home) v 4-4-2", Formation::F433, Formation::F442),
    ] {
        let mut by_value = Group::default();
        let mut forced_all = Group::default();
        let mut forced = [Group::default(); 4];
        // The forced pass itself: worth > 0, and its estimated success.
        let (mut forced_positive, mut forced_success) = (0u64, 0.0f64);
        for seed in 0..n {
            let (db, setup) = fm_match::demo::demo_match_with(seed, home, away);
            let mut e = MatchEngine::new(&setup, &db);
            let forced_at = e.state().tuning.value.forced_release_ticks;
            let margin = e.state().tuning.value.act_margin;
            let min_hold = e.state().tuning.decision.min_hold_ticks;
            let mut spell: Option<Spell> = None;
            let mut prev = TickFrame::capture(e.state());
            let mut prev_ht = 0;
            let mut last = None;
            while !e.is_finished() {
                e.tick_logic();
                let s = e.state();
                let frame = TickFrame::capture(s);
                match s.ball {
                    BallState::Held { holder } => {
                        let h = holder as usize;
                        if spell.map_or(true, |sp| sp.holder != h) || s.holder_ticks < prev_ht {
                            spell = Some(Spell {
                                holder: h,
                                start: frame.pos(h),
                                decisions: 0,
                                pass_positive: 0,
                                pass_beats_hold: 0,
                                pass_best: 0,
                                won: [0; 4],
                                pressed: 0,
                                sum: [0.0; 4],
                                nobody_in_range: 0,
                                dribblers: 0,
                                passers: 0,
                                dribble_over_pass: 0,
                            });
                        }
                        // The decision the carrier makes next tick.
                        let mut next = s.clone();
                        next.holder_ticks += 1;
                        if DecisionSystem::redecides(&next, &frame, h) {
                            let v = DecisionSystem::option_values(&next, &frame, h);
                            last = Some(v);
                            let waiting = next.holder_ticks < min_hold && !v.pressed;
                            if let (Some(sp), false) = (spell.as_mut(), waiting) {
                                let bar = v.hold + margin;
                                let winner =
                                    if v.shot > bar && v.shot >= v.pass && v.shot >= v.dribble {
                                        1
                                    } else if v.pass > bar && v.pass >= v.dribble {
                                        2
                                    } else if v.dribble > bar {
                                        3
                                    } else {
                                        0
                                    };
                                sp.decisions += 1;
                                sp.won[winner] += 1;
                                sp.pressed += u32::from(v.pressed);
                                sp.pass_positive += u32::from(v.pass > 0.0);
                                sp.pass_beats_hold += u32::from(v.pass > bar);
                                sp.pass_best += u32::from(winner == 2);
                                sp.nobody_in_range += u32::from(v.pass_to.is_none());
                                sp.sum[0] += f64::from(v.hold);
                                if v.pass_to.is_some() {
                                    sp.passers += 1;
                                    sp.sum[1] += f64::from(v.pass);
                                }
                                if v.dribble > f32::MIN {
                                    sp.dribblers += 1;
                                    sp.sum[2] += f64::from(v.dribble);
                                }
                                sp.dribble_over_pass += u32::from(v.dribble > v.pass);
                                sp.sum[3] += f64::from(v.pass_success);
                            }
                        }
                    }
                    BallState::Flight {
                        flight,
                        intent: FlightIntent::Pass { .. },
                    } if flight.kick_ms == s.now_ms() => {
                        if let Some(sp) = spell.take() {
                            let p = &s.players[sp.holder];
                            let at = prev.pos(sp.holder);
                            let depth = if s.attacking(p.side).direction() > 0.0 {
                                at.x
                            } else {
                                105.0 - at.x
                            };
                            let third = ((depth / 35.0) as usize).min(2);
                            let carried = at.distance(sp.start);
                            if prev_ht + 1 >= forced_at {
                                let class = if sp.pass_best > 0 {
                                    3
                                } else if sp.pass_beats_hold > 0 {
                                    2
                                } else {
                                    usize::from(sp.pass_positive > 0)
                                };
                                forced[class].add(&sp, carried, third, line_of(p.role));
                                forced_all.add(&sp, carried, third, line_of(p.role));
                                if let Some(v) = last {
                                    forced_positive += u64::from(v.pass > 0.0);
                                    forced_success += f64::from(v.pass_success);
                                }
                            } else {
                                by_value.add(&sp, carried, third, line_of(p.role));
                            }
                        }
                    }
                    _ => spell = None,
                }
                prev = frame;
                prev_ht = s.holder_ticks;
            }
        }
        let passes = (by_value.spells + forced_all.spells).max(1) as f64;
        let f = forced_all.spells.max(1) as f64;
        println!(
            "== {name} ({n} matches): {:.0} passes/match, {:.1}% at the forced release",
            passes / n as f64,
            100.0 * forced_all.spells as f64 / passes
        );
        by_value.print("passes played by value", n);
        forced_all.print("passes at the forced release", n);
        println!(
            "    the forced pass itself: worth > 0 in {:.0}%, estimated success {:.0}%",
            100.0 * forced_positive as f64 / f,
            100.0 * forced_success / f
        );
        println!("  forced spells by cause:");
        for (c, label) in FORCED.iter().enumerate() {
            println!(
                "    {label:<32} {:5.1}% of forced",
                100.0 * forced[c].spells as f64 / f
            );
        }
        for (c, label) in FORCED.iter().enumerate() {
            if forced[c].spells > 0 {
                forced[c].print(label, n);
            }
        }
    }
}
