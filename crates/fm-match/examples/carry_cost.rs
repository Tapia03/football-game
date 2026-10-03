//! `cargo run --release -p fm-match --example carry_cost` — what carrying
//! the ball costs in the engine as it plays (SPEC Fase 5, cost of
//! carrying): how often the carrier loses the ball per tick on it, per
//! metre carried and by local pressure, against what the decision assumes
//! (`dribble_keep_open` / cramped keep per 5 m step) and against the cost
//! of holding (erosion) and of passing (measured failure).
//!
//! In the engine a carrier only loses the ball to a tackle (won clean, or
//! poked loose and picked up by the opponents) or by running it out of
//! play. Restarts lost at once (the taker "runs the ball out" on the tick
//! he takes a throw-in or corner) are counted apart: they are not carrying.
//! Observed from outside, tick by tick: no engine change.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::many_single_char_names
)]
use fm_match::state::{BallState, FlightIntent};
use fm_match::{Action, Formation, MatchEngine, Side, TickFrame};

/// Exposure and losses of one kind of tick on the ball.
#[derive(Default, Clone, Copy)]
struct Cell {
    ticks: u64,
    metres: f64,
    tackles: u64,
    fouls_won: u64,
    lost: u64,
}

impl Cell {
    fn line(&self, label: &str, matches: u64) -> String {
        let t = self.ticks.max(1) as f64;
        format!(
            "    {label:<34} {:7.0} ticks/match | tackled {:5.2} per 1000 ticks | lost {:5.2} per 1000 ticks | lost {:5.2} per 100 m | fouls won {:4.2} per 1000 ticks",
            self.ticks as f64 / matches as f64,
            1000.0 * self.tackles as f64 / t,
            1000.0 * self.lost as f64 / t,
            100.0 * self.lost as f64 / self.metres.max(1.0),
            1000.0 * self.fouls_won as f64 / t,
        )
    }
}

/// What the previous tick looked like, if somebody had the ball.
#[derive(Clone, Copy)]
struct Before {
    holder: usize,
    side: Side,
    /// [dribble open | dribble cramped | hold].
    kind: usize,
    pressed: bool,
    /// Nearest opponent [<1.8 | <2.5 | <4.5 | 4.5+ m].
    near: usize,
    tackles: u16,
    /// The spell started from a restart (throw-in, corner...).
    from_restart: bool,
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
        let mut kind = [Cell::default(); 3];
        let mut pressed = [Cell::default(); 2];
        let mut near = [Cell::default(); 4];
        // Tackle outcomes: foul | won clean | poked loose | carrier beats it.
        let mut outcome = [0u64; 4];
        // Balls poked loose: next possession to the tackler's side.
        let (mut loose, mut loose_lost) = (0u64, 0u64);
        let mut out_of_play = 0u64;
        // Restarts whose taker loses the ball out of play straight away.
        let (mut restarts, mut restarts_lost) = (0u64, 0u64);
        let (mut keep_open, mut keep_cramped, mut cramped_n) = (0.0f64, 0.0f64, 0u64);
        let mut erosion = 0.0f64;
        let mut step = 0.0f64;
        for seed in 0..n {
            let (db, setup) = fm_match::demo::demo_match_with(seed, home, away);
            let mut e = MatchEngine::new(&setup, &db);
            let tuning = e.state().tuning;
            keep_open = f64::from(tuning.value.dribble_keep_open);
            erosion = f64::from(tuning.value.hold_erosion);
            step = f64::from(tuning.decision.dribble_step);
            let mut prev = TickFrame::capture(e.state());
            let mut before: Option<Before> = None;
            let mut was_dead = false;
            let mut from_restart = false;
            // A ball poked loose off this side's carrier, and the cells it
            // would be charged to.
            let mut pending: Option<Before> = None;
            while !e.is_finished() {
                e.tick_logic();
                let s = e.state();
                let frame = TickFrame::capture(s);
                if let (Some(b), BallState::Held { holder }) = (pending, s.ball) {
                    loose += 1;
                    if s.players[holder as usize].side != b.side {
                        loose_lost += 1;
                        kind[b.kind].lost += 1;
                        pressed[usize::from(b.pressed)].lost += 1;
                        near[b.near].lost += 1;
                    }
                    pending = None;
                }
                if let Some(b) = before {
                    let moved = f64::from(frame.pos(b.holder).distance(prev.pos(b.holder)));
                    let tackled = s.team(b.side.other()).tackles > b.tackles;
                    let cells: [&mut Cell; 3] = [
                        &mut kind[b.kind],
                        &mut pressed[usize::from(b.pressed)],
                        &mut near[b.near],
                    ];
                    // 0 none, 1 lost, 2 foul won.
                    let mut event = 0;
                    if tackled {
                        let o = match s.ball {
                            BallState::Dead(_) => 0,
                            BallState::Held { holder } if holder as usize == b.holder => 3,
                            BallState::Held { .. } => 1,
                            BallState::Flight { .. } => 2,
                        };
                        outcome[o] += 1;
                        match o {
                            0 => event = 2,
                            1 => event = 1,
                            2 => pending = Some(b),
                            _ => {}
                        }
                    } else if let BallState::Dead(r) = s.ball {
                        // Carried out of play.
                        if r.side != b.side {
                            if b.from_restart {
                                restarts_lost += 1;
                            } else {
                                out_of_play += 1;
                                event = 1;
                            }
                        }
                    }
                    for c in cells {
                        c.ticks += 1;
                        c.metres += moved;
                        c.tackles += u64::from(tackled);
                        c.lost += u64::from(event == 1);
                        c.fouls_won += u64::from(event == 2);
                    }
                }
                match s.ball {
                    BallState::Held { .. } if was_dead => {
                        from_restart = true;
                        restarts += 1;
                    }
                    BallState::Held { .. } => {}
                    _ => from_restart = false,
                }
                was_dead = matches!(s.ball, BallState::Dead(_));
                before = match s.ball {
                    BallState::Held { holder } => {
                        let h = holder as usize;
                        let p = &s.players[h];
                        let at = frame.pos(h);
                        let opp_dist = |to: fm_core::Vec2| {
                            (0..22)
                                .filter(|&j| s.players[j].side != p.side && !s.players[j].sent_off)
                                .map(|j| frame.pos(j).distance(to))
                                .fold(f32::MAX, f32::min)
                        };
                        let nearest = opp_dist(at);
                        let k = match s.carrier_plan {
                            Action::Dribble { target } => {
                                // The decision's own test of "cramped".
                                let dir = (target - at).normalize();
                                let ahead = at + dir * tuning.decision.dribble_probe_ahead;
                                if opp_dist(ahead) > tuning.decision.dribble_space_min {
                                    0
                                } else {
                                    keep_cramped += f64::from(
                                        tuning.value.dribble_keep_cramped_base
                                            + tuning.value.dribble_keep_cramped_skill
                                                * f32::from(p.attrs.technical.dribbling)
                                                / 100.0,
                                    );
                                    cramped_n += 1;
                                    1
                                }
                            }
                            _ => 2,
                        };
                        Some(Before {
                            holder: h,
                            side: p.side,
                            kind: k,
                            pressed: nearest < tuning.decision.pressure_radius,
                            near: [1.8, 2.5, 4.5]
                                .iter()
                                .position(|&x| nearest < x)
                                .unwrap_or(3),
                            tackles: s.team(p.side.other()).tackles,
                            from_restart,
                        })
                    }
                    BallState::Flight {
                        intent: FlightIntent::Loose,
                        ..
                    } => None,
                    _ => {
                        pending = None;
                        None
                    }
                };
                prev = frame;
            }
        }
        println!("== {name} ({n} matches)");
        println!("  by what the carrier is doing:");
        for (c, label) in ["carrying, space ahead", "carrying, cramped", "holding"]
            .iter()
            .enumerate()
        {
            println!("{}", kind[c].line(label, n));
        }
        println!("  by pressure (an opponent within pressure_radius):");
        for (c, label) in ["not pressed", "pressed"].iter().enumerate() {
            println!("{}", pressed[c].line(label, n));
        }
        println!("  by the nearest opponent:");
        for (c, label) in ["< 1.8 m", "1.8-2.5 m", "2.5-4.5 m", "4.5+ m"]
            .iter()
            .enumerate()
        {
            println!("{}", near[c].line(label, n));
        }
        let tk = outcome.iter().sum::<u64>().max(1) as f64;
        println!(
            "  tackles {:.1}/match: foul {:.0}% | won clean {:.0}% | poked loose {:.0}% | carrier beats it {:.0}% ; poked loose -> opponents {:.0}% ; carried out of play {:.1}/match",
            tk / n as f64,
            100.0 * outcome[0] as f64 / tk,
            100.0 * outcome[1] as f64 / tk,
            100.0 * outcome[2] as f64 / tk,
            100.0 * outcome[3] as f64 / tk,
            100.0 * loose_lost as f64 / loose.max(1) as f64,
            out_of_play as f64 / n as f64
        );
        println!(
            "  restarts taken {:.0}/match, of which lost out of play at once {:.1}/match ({:.0}%) -- not counted above",
            restarts as f64 / n as f64,
            restarts_lost as f64 / n as f64,
            100.0 * restarts_lost as f64 / restarts.max(1) as f64
        );
        // Per 5 m step, as the decision counts it.
        let per_step = |c: &Cell| {
            let ticks_per_step = step / (c.metres / c.ticks.max(1) as f64).max(1e-6);
            let p = c.lost as f64 / c.ticks.max(1) as f64;
            // p is tiny: 1 − (1 − p)^n ≈ n·p.
            (ticks_per_step, (p * ticks_per_step).min(1.0))
        };
        let (t_open, l_open) = per_step(&kind[0]);
        let (t_cramped, l_cramped) = per_step(&kind[1]);
        println!(
            "  one {step:.0} m step, space ahead: {t_open:.1} ticks, ball lost {:.2}% (the decision assumes {:.1}%)",
            100.0 * l_open,
            100.0 * (1.0 - keep_open)
        );
        println!(
            "  one {step:.0} m step, cramped:     {t_cramped:.1} ticks, ball lost {:.2}% (the decision assumes {:.1}%)",
            100.0 * l_cramped,
            100.0 * (1.0 - keep_cramped / cramped_n.max(1) as f64)
        );
        println!(
            "  for comparison: holding erodes {:.0}% of its value per tick ({:.0}% over {t_open:.1} ticks); a pass fails ~40% of the time",
            100.0 * erosion,
            100.0 * (erosion * t_open).min(1.0)
        );
    }
}
