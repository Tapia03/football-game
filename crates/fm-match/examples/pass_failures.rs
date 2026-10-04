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
    clippy::many_single_char_names,
    clippy::struct_excessive_bools
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
    /// At the kick: where from, on which tick, everybody's position, the
    /// decision's estimated success, and whether it was a forced release.
    from: Vec2,
    kick_tick: u32,
    at_kick: [Vec2; 22],
    estimate: f32,
    forced: bool,
    /// Receiver against the ball (measurement of "receiver off the spot"):
    /// closest at a tick instant, where the engine checks (distance, ball
    /// height, receiver still barred from touching); closest at any time
    /// (10 ms steps) with the ball low enough; closest ever, any height.
    near_tick: (f32, f32, bool),
    near_low: f32,
    near_any: f32,
    /// When the ball was closest to the aim point: how far the receiver
    /// was from that point, and whether he had moved toward the passer.
    left_spot: f32,
    toward_passer: bool,
    /// Receiver at the kick: speed (m/s) and whether on a live off-ball run.
    speed: f32,
    running: bool,
}

/// Detail of the "receiver off the spot" passes (measurement of commit 4).
#[derive(Default, Clone, Copy)]
struct Spot {
    n: u64,
    /// Exclusive mechanisms, in this order: barred (kicked the ball less
    /// than `retouch_ticks` ago), in reach at a tick but no touch, in reach
    /// only between ticks, in reach only while the ball was too high, left
    /// the spot, stayed and the ball went by out of reach.
    cause: [u64; 6],
    /// Closest receiver-ball distance at a tick [<1.5 | <2 | <3 | <5 | 5+ m].
    near: [u64; 5],
    /// Receiver's distance from the aim point when the ball got there
    /// [<1 | <1.5 | <3 | <6 | 6+ m], and how many of those who left went
    /// toward the passer.
    left: [u64; 5],
    left_toward: u64,
    left_n: u64,
}

/// Detail of the passes cut out in flight (measurement B).
#[derive(Default, Clone, Copy)]
struct Cuts {
    n: u64,
    controlled: u64,
    forced: u64,
    /// Ticks from the kick to the touch [1 | 2 | 3-5 | 6-10 | 11+].
    after: [u64; 5],
    /// Share of the pass length travelled at the touch [<25 | <50 | <75 | 75+ %].
    along: [u64; 4],
    /// Interceptor at the kick: distance to the passer [<2.5 | <5 | <10 | 10+ m]
    /// and to the pass lane [<1 | <2 | <4 | 4+ m].
    to_passer: [u64; 4],
    to_lane: [u64; 4],
    /// How far the interceptor moved from the kick to the touch [<1 | <3 | <6 | 6+ m].
    moved: [u64; 4],
    /// Opponents within 2 m of the lane at the kick [0 | 1 | 2 | 3+].
    in_lane: [u64; 4],
    estimate_sum: f64,
}

fn bucket(v: f32, edges: &[f32]) -> usize {
    edges.iter().position(|&x| v < x).unwrap_or(edges.len())
}

/// Distance from `p` to the segment `a → b`.
fn to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let d = a.distance(b).max(1e-3);
    let dir = (b - a) / d;
    let along = (p - a).dot(dir).clamp(0.0, d);
    p.distance(a + dir * along)
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
        let mut cuts = Cuts::default();
        // Estimated success and forced share of the passes that arrive.
        let (mut ok_estimate, mut ok_forced, mut ok_n) = (0.0f64, 0u64, 0u64);
        let mut ok_in_lane = [0u64; 4];
        // Ground passes by the nearest opponent's distance to the lane at
        // the kick [<0.45 | <0.9 | <1.5 | <2.5 | 2.5+ m]: all, and cut out.
        let mut by_offset = [[0u64; 2]; 5];
        let mut spot = Spot::default();
        // Receiver's speed at the kick [<1 | <3 | <5 | 5+ m/s] and live runs,
        // per class.
        let mut speed = [[0u64; 4]; 6];
        let mut on_run = [0u64; 6];
        for seed in 0..n {
            let (db, setup) = fm_match::demo::demo_match_with(seed, home, away);
            let mut e = MatchEngine::new(&setup, &db);
            let reach = e.state().tuning.control.receiver_radius;
            let retouch = e.state().tuning.control.retouch_ticks;
            let lofted = e.state().tuning.pass.lofted_dist;
            let max_height = e.state().tuning.control.max_height;
            let mut prev = TickFrame::capture(e.state());
            let mut prev_ht = 0;
            let forced_at = e.state().tuning.value.forced_release_ticks;
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
                    speed[class][bucket(o.speed, &[1.0, 3.0, 5.0])] += 1;
                    on_run[class] += u64::from(o.running);
                    let side_of = |i: usize| s.players[i].side;
                    let lane_count = (0..22)
                        .filter(|&i| side_of(i) != side && !s.players[i].sent_off)
                        .filter(|&i| to_segment(o.at_kick[i], o.from, o.target) < 2.0)
                        .count()
                        .min(3);
                    if o.length < 3 {
                        let nearest = (0..22)
                            .filter(|&i| side_of(i) != side && !s.players[i].sent_off)
                            .map(|i| to_segment(o.at_kick[i], o.from, o.target))
                            .fold(f32::MAX, f32::min);
                        let b = bucket(nearest, &[0.45, 0.9, 1.5, 2.5]);
                        by_offset[b][0] += 1;
                        by_offset[b][1] += u64::from(class == 2);
                    }
                    if class == 0 {
                        ok_n += 1;
                        ok_estimate += f64::from(o.estimate);
                        ok_forced += u64::from(o.forced);
                        ok_in_lane[lane_count] += 1;
                    }
                    if class == 5 {
                        spot.n += 1;
                        let (d, z, barred) = o.near_tick;
                        let cause = if barred && d < reach {
                            0
                        } else if d < reach && z < max_height {
                            1
                        } else if o.near_low < reach {
                            2
                        } else if o.near_any < reach {
                            3
                        } else if o.left_spot > reach {
                            4
                        } else {
                            5
                        };
                        spot.cause[cause] += 1;
                        spot.near[bucket(d, &[1.5, 2.0, 3.0, 5.0])] += 1;
                        spot.left[bucket(o.left_spot, &[1.0, 1.5, 3.0, 6.0])] += 1;
                        if o.left_spot > reach {
                            spot.left_n += 1;
                            spot.left_toward += u64::from(o.toward_passer);
                        }
                    }
                    if let (2, Some(i)) = (class, toucher) {
                        let ball = o.flight.pos_at(now).xy();
                        let length = o.from.distance(o.target).max(1e-3);
                        cuts.n += 1;
                        cuts.controlled += u64::from(controlled);
                        cuts.forced += u64::from(o.forced);
                        cuts.after
                            [bucket((s.tick - o.kick_tick) as f32, &[1.5, 2.5, 5.5, 10.5])] += 1;
                        cuts.along[bucket(o.from.distance(ball) / length, &[0.25, 0.5, 0.75])] += 1;
                        cuts.to_passer[bucket(o.at_kick[i].distance(o.from), &[2.5, 5.0, 10.0])] +=
                            1;
                        cuts.to_lane[bucket(
                            to_segment(o.at_kick[i], o.from, o.target),
                            &[1.0, 2.0, 4.0],
                        )] += 1;
                        cuts.moved
                            [bucket(frame.pos(i).distance(o.at_kick[i]), &[1.0, 3.0, 6.0])] += 1;
                        cuts.in_lane[lane_count] += 1;
                        cuts.estimate_sum += f64::from(o.estimate);
                    }
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
                        let from = flight.pos_at(now).xy();
                        let d = from.distance(target);
                        let passer = (0..22)
                            .find(|&i| s.players[i].touch_ready_tick == s.tick + retouch)
                            .unwrap_or(r);
                        let mut at_kick = [Vec2::ZERO; 22];
                        for (i, at) in at_kick.iter_mut().enumerate() {
                            *at = prev.pos(i);
                        }
                        open = Some(Open {
                            from,
                            kick_tick: s.tick,
                            at_kick,
                            estimate: fm_match::DecisionSystem::pass_success(s, &prev, passer, r),
                            forced: prev_ht + 1 >= forced_at,
                            near_tick: (f32::MAX, 0.0, false),
                            near_low: f32::MAX,
                            near_any: f32::MAX,
                            left_spot: 0.0,
                            toward_passer: false,
                            speed: frame.pos(r).distance(prev.pos(r)) * 10.0,
                            running: s.players[r].run_until > s.tick,
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
                // Follow the pass in flight: the ball against the aim point
                // and against the receiver, at this tick and until the next.
                if let (Some(o), BallState::Flight { flight, .. }) = (open.as_mut(), s.ball) {
                    if flight.kick_ms == o.flight.kick_ms {
                        let ball = flight.pos_at(now);
                        let at = frame.pos(o.receiver);
                        let d = ball.xy().distance(o.target);
                        if d <= o.miss {
                            o.miss = d;
                            o.left_spot = at.distance(o.target);
                            o.toward_passer = (at - o.target).dot(o.from - o.target) > 0.0;
                        } else if d > o.miss + 0.3 {
                            o.past = true;
                        }
                        let rb = at.distance(ball.xy());
                        if rb < o.near_tick.0 {
                            let barred = s.tick < s.players[o.receiver].touch_ready_tick;
                            o.near_tick = (rb, ball.z, barred);
                        }
                        for step in 0..10 {
                            let t = now + step * 10;
                            let b = flight.pos_at(t);
                            let r = s.players[o.receiver].pos(t).distance(b.xy());
                            o.near_any = o.near_any.min(r);
                            if b.z < max_height {
                                o.near_low = o.near_low.min(r);
                            }
                        }
                    }
                }
                prev = frame;
                prev_ht = s.holder_ticks;
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
        let pct = |v: &[u64]| {
            let total = v.iter().sum::<u64>().max(1) as f64;
            v.iter()
                .map(|&c| format!("{:.0}%", 100.0 * c as f64 / total))
                .collect::<Vec<_>>()
                .join(" | ")
        };
        let c = cuts.n.max(1) as f64;
        println!(
            "  -- passes cut out in flight: {:.0}/match",
            cuts.n as f64 / n as f64
        );
        println!(
            "  the interceptor controls it {:.0}% (deflects {:.0}%) | from a forced release {:.0}% (passes that arrive: {:.0}%)",
            100.0 * cuts.controlled as f64 / c,
            100.0 - 100.0 * cuts.controlled as f64 / c,
            100.0 * cuts.forced as f64 / c,
            100.0 * ok_forced as f64 / ok_n.max(1) as f64
        );
        println!(
            "  decision's estimated success: {:.1}% for the passes cut out, {:.1}% for the passes that arrive",
            100.0 * cuts.estimate_sum / c,
            100.0 * ok_estimate / ok_n.max(1) as f64
        );
        println!(
            "  ticks from kick to touch [1 | 2 | 3-5 | 6-10 | 11+]: {}",
            pct(&cuts.after)
        );
        println!(
            "  share of the pass travelled [<25 | <50 | <75 | 75+ %]: {}",
            pct(&cuts.along)
        );
        println!(
            "  interceptor at the kick, distance to the passer [<2.5 | <5 | <10 | 10+ m]: {}",
            pct(&cuts.to_passer)
        );
        println!(
            "  interceptor at the kick, distance to the lane [<1 | <2 | <4 | 4+ m]: {}",
            pct(&cuts.to_lane)
        );
        println!(
            "  interceptor moved from kick to touch [<1 | <3 | <6 | 6+ m]: {}",
            pct(&cuts.moved)
        );
        println!(
            "  opponents within 2 m of the lane at the kick [0 | 1 | 2 | 3+]: cut out {} ; arrive {}",
            pct(&cuts.in_lane),
            pct(&ok_in_lane)
        );
        let sp = spot.n.max(1) as f64;
        println!(
            "  -- receiver off the spot: {:.0}/match",
            spot.n as f64 / n as f64
        );
        println!(
            "  mechanism [barred after own kick | in reach at a tick, no touch | in reach only between ticks | in reach only while too high | receiver left the spot | stayed, ball out of reach]: {}",
            pct(&spot.cause)
        );
        println!(
            "  closest receiver-ball distance at a tick [<1.5 | <2 | <3 | <5 | 5+ m]: {}",
            pct(&spot.near)
        );
        println!(
            "  receiver's distance from the aim point when the ball got there [<1 | <1.5 | <3 | <6 | 6+ m]: {} ; of those who left, toward the passer {:.0}%",
            pct(&spot.left),
            100.0 * spot.left_toward as f64 / spot.left_n.max(1) as f64
        );
        let _ = sp;
        println!("  receiver's speed at the kick [<1 | <3 | <5 | 5+ m/s] and share on a live run:");
        for (c, label) in CLASSES.iter().enumerate() {
            println!(
                "    {label:<24} {} ; on a run {:.0}%",
                pct(&speed[c]),
                100.0 * on_run[c] as f64 / t.n[c].iter().sum::<u64>().max(1) as f64
            );
        }
        let total_by_speed = (0..4)
            .map(|b| {
                let all: u64 = speed.iter().map(|row| row[b]).sum::<u64>().max(1);
                format!(
                    "{:.0}% of {:.0}/match",
                    100.0 * speed[0][b] as f64 / all as f64,
                    all as f64 / n as f64
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        println!("  passes the receiver controls, by his speed at the kick: {total_by_speed}");
        println!(
            "  ground passes cut out, by the nearest opponent's distance to the lane at the kick [<0.45 | <0.9 | <1.5 | <2.5 | 2.5+ m]: {}",
            by_offset
                .iter()
                .map(|b| format!(
                    "{:.0}% of {:.0}/match",
                    100.0 * b[1] as f64 / b[0].max(1) as f64,
                    b[0] as f64 / n as f64
                ))
                .collect::<Vec<_>>()
                .join(" | ")
        );
    }
}
