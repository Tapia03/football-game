//! `cargo run --release -p fm-match --example pass_failures` — why passes
//! fail (pass-execution diagnosis, SPEC Fase 5): every pass is followed
//! from the kick to the first player who touches the ball (or to the ball
//! going dead) and put in one class:
//!
//! - **ok**: the intended receiver controls it;
//! - **team-mate**: another team-mate touches it first;
//! - **intercepted in flight**: an opponent touches it while it is still
//!   travelling toward the point it was aimed at;
//! - **bad first touch**: the receiver touches it and does not control it;
//! - **off target**: the ball got past the aim point without ever coming
//!   within the receiver's reach (`receiver_radius`) of it;
//! - **receiver off the spot**: the ball did come within reach of the aim
//!   point, and got past it without the receiver touching it.
//!
//! The aim point is where the receiver stood at the kick (what the resolver
//! aims at). Observed from outside, tick by tick: no engine change.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::many_single_char_names
)]
use fm_core::Vec2;
use fm_match::state::{BallState, FlightIntent};
use fm_match::{BallFlight, Formation, MatchEngine, TickFrame};

const CLASSES: [&str; 6] = [
    "ok",
    "team-mate",
    "intercepted in flight",
    "bad first touch",
    "off target",
    "receiver off the spot",
];
const LENGTHS: [&str; 4] = ["<10 m", "10-20 m", "20-28 m", "28+ m (lofted)"];

/// A pass being followed.
struct Open {
    flight: BallFlight,
    receiver: usize,
    target: Vec2,
    length: usize,
    /// Closest the ball has come to the aim point so far (m).
    miss: f32,
    /// The ball has started moving away from the aim point.
    past: bool,
}

#[derive(Default, Clone, Copy)]
struct Tally {
    n: [[u64; 4]; 6],
    /// Of the failures (classes 2..), next possession to the opponents.
    lost: [u64; 6],
    resolved: [u64; 6],
    /// Sum of the miss distance (off target) and of how far the receiver
    /// had moved from the aim point when the pass ended.
    miss_sum: [f64; 6],
    moved_sum: [f64; 6],
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
        let mut t = Tally::default();
        for seed in 0..n {
            let (db, setup) = fm_match::demo::demo_match_with(seed, home, away);
            let mut e = MatchEngine::new(&setup, &db);
            let reach = e.state().tuning.control.receiver_radius;
            let retouch = e.state().tuning.control.retouch_ticks;
            let lofted = e.state().tuning.pass.lofted_dist;
            let mut prev = TickFrame::capture(e.state());
            let mut open: Option<Open> = None;
            // A failed pass waiting for the next possession: (class, side).
            let mut pending: Option<(usize, fm_match::Side)> = None;
            while !e.is_finished() {
                e.tick_logic();
                let s = e.state();
                let now = s.now_ms();
                let frame = TickFrame::capture(s);
                if let (Some((class, side)), BallState::Held { holder }) = (pending, s.ball) {
                    t.resolved[class] += 1;
                    if s.players[holder as usize].side != side {
                        t.lost[class] += 1;
                    }
                    pending = None;
                }
                // Is the followed pass still the ball in flight?
                let same = matches!((&open, s.ball),
                    (Some(o), BallState::Flight { flight, .. }) if flight.kick_ms == o.flight.kick_ms);
                if let (Some(o), true) = (open.as_mut(), same) {
                    let d = o.flight.pos_at(now).xy().distance(o.target);
                    if d < o.miss {
                        o.miss = d;
                    } else if d > o.miss + 0.3 {
                        o.past = true;
                    }
                }
                if let (Some(o), false) = (open.as_ref(), same) {
                    let side = s.players[o.receiver].side;
                    // Who ended it: the holder, or whoever kicked the new
                    // flight this tick; nobody if the ball went dead.
                    let (toucher, controlled) = match s.ball {
                        BallState::Held { holder } => (Some(holder as usize), true),
                        BallState::Flight { intent, .. } => (
                            (0..22).find(|&i| s.players[i].touch_ready_tick == s.tick + retouch),
                            !matches!(intent, FlightIntent::Loose),
                        ),
                        BallState::Dead(_) => (None, false),
                    };
                    let class = match toucher {
                        Some(i) if i == o.receiver && controlled => 0,
                        Some(i) if i == o.receiver => 3,
                        Some(i) if s.players[i].side == side => 1,
                        // An opponent, or out of play.
                        other => {
                            if other.is_some() && !o.past {
                                2
                            } else if o.miss > reach {
                                4
                            } else {
                                5
                            }
                        }
                    };
                    t.n[class][o.length] += 1;
                    t.miss_sum[class] += f64::from(o.miss);
                    t.moved_sum[class] += f64::from(frame.pos(o.receiver).distance(o.target));
                    if class >= 2 {
                        match s.ball {
                            BallState::Held { holder } => {
                                t.resolved[class] += 1;
                                if s.players[holder as usize].side != side {
                                    t.lost[class] += 1;
                                }
                            }
                            _ => pending = Some((class, side)),
                        }
                    }
                    open = None;
                }
                if let BallState::Flight {
                    flight,
                    intent: FlightIntent::Pass { receiver },
                } = s.ball
                {
                    if flight.kick_ms == now {
                        let r = receiver as usize;
                        let target = prev.pos(r);
                        let d = flight.pos_at(now).xy().distance(target);
                        open = Some(Open {
                            flight,
                            receiver: r,
                            target,
                            length: if d > lofted {
                                3
                            } else {
                                [10.0, 20.0].iter().position(|&x| d < x).unwrap_or(2)
                            },
                            miss: d,
                            past: false,
                        });
                        pending = None;
                    }
                }
                prev = frame;
            }
        }
        let total: u64 = t.n.iter().flatten().sum::<u64>().max(1);
        let failed: u64 = t.n[2..].iter().flatten().sum::<u64>().max(1);
        println!(
            "== {name} ({n} matches): {:.0} passes/match, {:.1}% fail",
            total as f64 / n as f64,
            100.0 * failed as f64 / total as f64
        );
        println!(
            "  {:<24} {:>7} {:>8} {:>9} | by length: {}",
            "class",
            "/match",
            "of all",
            "of fails",
            LENGTHS.join(" | ")
        );
        for (c, label) in CLASSES.iter().enumerate() {
            let k: u64 = t.n[c].iter().sum();
            let by_len = (0..4)
                .map(|l| {
                    let col: u64 = t.n.iter().map(|row| row[l]).sum::<u64>().max(1);
                    format!("{:5.1}%", 100.0 * t.n[c][l] as f64 / col as f64)
                })
                .collect::<Vec<_>>()
                .join(" | ");
            let of_fails = if c >= 2 {
                format!("{:8.1}%", 100.0 * k as f64 / failed as f64)
            } else {
                format!("{:>9}", "-")
            };
            println!(
                "  {label:<24} {:7.1} {:7.1}% {of_fails} | {by_len}",
                k as f64 / n as f64,
                100.0 * k as f64 / total as f64,
            );
        }
        let per_len = (0..4)
            .map(|l| {
                format!(
                    "{:.0}",
                    t.n.iter().map(|row| row[l]).sum::<u64>() as f64 / n as f64
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        println!("  passes/match by length: {per_len}");
        for (c, label) in CLASSES.iter().enumerate().skip(2) {
            let k = t.n[c].iter().sum::<u64>().max(1) as f64;
            println!(
                "  {:<24} next possession to the opponents {:.0}% | closest the ball came to the aim point {:.1} m | receiver had moved {:.1} m",
                label,
                100.0 * t.lost[c] as f64 / t.resolved[c].max(1) as f64,
                t.miss_sum[c] / k,
                t.moved_sum[c] / k,
            );
        }
    }
}
