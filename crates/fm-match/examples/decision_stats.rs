//! `cargo run --release -p fm-match --example decision_stats` — how carriers
//! release the ball: time on the ball, pass lengths, shot distances, how
//! possessions end (used to calibrate shots and passes jointly, SPEC Fase 5).

// Diagnostic tool: approximate float stats over counters are fine here.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::many_single_char_names
)]
use fm_match::demo::demo_match;
use fm_match::state::{BallState, FlightIntent};
use fm_match::{MatchEngine, Side, TickFrame};

fn bucket(v: f32, edges: &[f32]) -> usize {
    edges.iter().position(|&e| v < e).unwrap_or(edges.len())
}

fn pct(h: &[u64]) -> String {
    let n: u64 = h.iter().sum::<u64>().max(1);
    h.iter()
        .map(|&c| format!("{:5.1}%", 100.0 * c as f64 / n as f64))
        .collect::<Vec<_>>()
        .join(" ")
}

fn main() {
    let n = 20u64;
    let hold_edges = [5.0, 10.0, 25.0, 55.0];
    let pass_edges = [10.0, 20.0, 30.0];
    let shot_edges = [11.0, 16.5, 20.0];
    let (mut hold_pass, mut hold_shot) = ([0u64; 5], [0u64; 5]);
    let mut pass_len = [0u64; 4];
    let mut shot_dist = [0u64; 4];
    let mut shot_ht: Vec<u32> = Vec::new();
    let (mut held, mut flight, mut dead) = (0u64, 0u64, 0u64);
    let mut seq_passes: Vec<u32> = Vec::new();
    let (mut cur_side, mut cur_passes) = (None::<Side>, 0u32);
    for seed in 0..n {
        let (db, setup) = demo_match(seed);
        let mut e = MatchEngine::new(&setup, &db);
        let mut prev: Option<(BallState, u32, TickFrame)> = None;
        while !e.is_finished() {
            e.tick_logic();
            let s = e.state();
            let now = s.now_ms();
            match s.ball {
                BallState::Held { holder } => {
                    held += 1;
                    let side = s.players[holder as usize].side;
                    if cur_side != Some(side) {
                        if cur_side.is_some() {
                            seq_passes.push(cur_passes);
                        }
                        cur_side = Some(side);
                        cur_passes = 0;
                    }
                }
                BallState::Flight { .. } => flight += 1,
                BallState::Dead(_) => dead += 1,
            }
            if let (
                Some((BallState::Held { holder }, ht, f)),
                BallState::Flight { flight, intent },
            ) = (&prev, s.ball)
            {
                if flight.kick_ms == now {
                    let h = *holder as usize;
                    let hb = bucket(*ht as f32, &hold_edges);
                    match intent {
                        FlightIntent::Pass { receiver } => {
                            hold_pass[hb] += 1;
                            cur_passes += 1;
                            let d = f.pos(h).distance(f.pos(receiver as usize));
                            pass_len[bucket(d, &pass_edges)] += 1;
                        }
                        FlightIntent::Shot { .. } => {
                            hold_shot[hb] += 1;
                            shot_ht.push(*ht + 1);
                            let goal = s.attacking(s.players[h].side).goal_centre();
                            shot_dist[bucket(f.pos(h).distance(goal), &shot_edges)] += 1;
                        }
                        FlightIntent::Loose => {}
                    }
                }
            }
            prev = Some((s.ball, s.holder_ticks, TickFrame::capture(s)));
        }
    }
    let total = (held + flight + dead) as f64;
    println!(
        "ball state: held {:.1}%  flight {:.1}%  dead {:.1}%",
        100.0 * held as f64 / total,
        100.0 * flight as f64 / total,
        100.0 * dead as f64 / total
    );
    println!("ticks on ball at release  [<5 | 5-9 | 10-24 | 25-54 | 55+(forced)]");
    println!(
        "  passes: {}  ({:.0}/match)",
        pct(&hold_pass),
        hold_pass.iter().sum::<u64>() as f64 / n as f64
    );
    println!(
        "  shots:  {}  ({:.1}/match)",
        pct(&hold_shot),
        hold_shot.iter().sum::<u64>() as f64 / n as f64
    );
    println!(
        "pass length [<10 | 10-20 | 20-30 | 30+ m]: {}",
        pct(&pass_len)
    );
    println!(
        "shot dist   [<11 | 11-16.5 | 16.5-20 | 20+ m]: {}",
        pct(&shot_dist)
    );
    shot_ht.sort_unstable();
    let q = |f: f64| shot_ht[((shot_ht.len() - 1) as f64 * f) as usize];
    println!(
        "shot: ticks on ball p10/p50/p90 = {}/{}/{}; forced (>=55) {:.1}%",
        q(0.1),
        q(0.5),
        q(0.9),
        100.0 * shot_ht.iter().filter(|&&t| t >= 55).count() as f64 / shot_ht.len() as f64
    );
    let mut sp = [0u64; 5];
    for &k in &seq_passes {
        sp[bucket(k as f32, &[1.0, 3.0, 6.0, 10.0])] += 1;
    }
    println!(
        "passes per possession [0 | 1-2 | 3-5 | 6-9 | 10+]: {}  ({:.0} possessions/match)",
        pct(&sp),
        seq_passes.len() as f64 / n as f64
    );
}
