//! `cargo run --release -p fm-match --example defending_stats` — distance of
//! the nearest defender to the carrier and the distribution of challenge
//! scores (used to calibrate the defending model, SPEC Fase 5).

// Diagnostic tool: approximate float stats over counters are fine here.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names
)]
use fm_match::demo::demo_match;
use fm_match::state::BallState;
use fm_match::{DecisionSystem, MatchEngine, Role, TickFrame};
fn main() {
    let n = 10u64;
    let mut hist = [0u64; 6];
    let mut held = 0u64;
    let mut scores: Vec<f32> = Vec::new();
    let mut in_reach_ticks = 0u64;
    for seed in 0..n {
        let (db, setup) = demo_match(seed);
        let mut e = MatchEngine::new(&setup, &db);
        while !e.is_finished() {
            e.tick_logic();
            let s = e.state();
            if let BallState::Held { holder } = s.ball {
                let f = TickFrame::capture(s);
                let h = holder as usize;
                let hp = f.pos(h);
                let side = s.players[h].side;
                let mut d = f32::MAX;
                let mut any = false;
                for j in 0..22 {
                    let p = &s.players[j];
                    if p.side == side || !p.active() || p.role == Role::Goalkeeper {
                        continue;
                    }
                    let dj = f.pos(j).distance(hp);
                    d = d.min(dj);
                    if dj < s.tuning.duel.tackle_range {
                        any = true;
                        scores.push(DecisionSystem::challenge_score(s, &f, j, h));
                    }
                }
                if any {
                    in_reach_ticks += 1;
                }
                held += 1;
                let b = if d < 1.0 {
                    0
                } else if d < 2.0 {
                    1
                } else if d < 3.0 {
                    2
                } else if d < 4.0 {
                    3
                } else if d < 6.0 {
                    4
                } else {
                    5
                };
                hist[b] += 1;
            }
        }
    }
    let pct = |x: u64| 100.0 * x as f64 / held as f64;
    println!("held ticks/match {:.0}; nearest defender: <1m {:.0}% 1-2m {:.0}% 2-3m {:.0}% 3-4m {:.0}% 4-6m {:.0}% >6m {:.0}%; someone in reach {:.0}% of held ticks",
        held as f64 / n as f64, pct(hist[0]), pct(hist[1]), pct(hist[2]), pct(hist[3]), pct(hist[4]), pct(hist[5]), pct(in_reach_ticks));
    scores.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |p: f64| scores[((scores.len() as f64 - 1.0) * p) as usize];
    println!(
        "challenge scores (in reach): p10 {:.2} p25 {:.2} p50 {:.2} p75 {:.2} p90 {:.2}",
        q(0.1),
        q(0.25),
        q(0.5),
        q(0.75),
        q(0.9)
    );
}
