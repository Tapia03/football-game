//! `cargo run --release -p fm-match --example buildup_stats` — who has the
//! ball and where passes go (item 9, build-up from the back): share of
//! held-ball ticks and of passes received per role, pass direction, and
//! how passes are released (by value or by the forced release).

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::many_single_char_names
)]
use fm_match::state::{BallState, FlightIntent};
use fm_match::{Formation, MatchEngine, Role, TickFrame};

const ROLES: [Role; 10] = [
    Role::Goalkeeper,
    Role::CentreBack,
    Role::FullBack,
    Role::WingBack,
    Role::DefensiveMidfielder,
    Role::CentralMidfielder,
    Role::WideMidfielder,
    Role::AttackingMidfielder,
    Role::Winger,
    Role::Striker,
];

fn main() {
    let n: u64 = std::env::var("FM_MATCHES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);
    for (name, home, away) in [
        ("4-4-2 (home) v 4-3-3", Formation::F442, Formation::F433),
        ("4-3-3 (home) v 4-4-2", Formation::F433, Formation::F442),
    ] {
        // Per side (0 home, 1 away) and role.
        let mut held = [[0u64; 10]; 2];
        let mut passes_from = [[0u64; 10]; 2];
        let mut passes_to = [[0u64; 10]; 2];
        // Pass direction along the attack: back (< −2 m), square, forward.
        let mut dir = [[0u64; 3]; 2];
        let mut forced = [0u64; 2];
        let mut third = [[0u64; 3]; 2];
        // Pass outcome by how far the nearest opponent was from the receiver
        // at the kick [<3 | 3-6 | 6-12 | 12+ m]: receiver, team-mate,
        // opponent, dead ball.
        let mut outcome = [[0u64; 4]; 4];
        let mut est = [0.0f64; 4];
        for seed in 0..n {
            let (db, setup) = fm_match::demo::demo_match_with(seed, home, away);
            let mut e = MatchEngine::new(&setup, &db);
            let mut prev = TickFrame::capture(e.state());
            let mut prev_ht = 0;
            let forced_at = e.state().tuning.value.forced_release_ticks;
            let mut open: Option<(usize, usize)> = None;
            while !e.is_finished() {
                e.tick_logic();
                let s = e.state();
                let role_ix =
                    |i: usize| ROLES.iter().position(|r| *r == s.players[i].role).unwrap();
                let side_ix = |i: usize| usize::from(i >= 11);
                if let Some((recv, bucket)) = open {
                    let end = match s.ball {
                        BallState::Held { holder } if holder as usize == recv => Some(0),
                        BallState::Held { holder }
                            if s.players[holder as usize].side == s.players[recv].side =>
                        {
                            Some(1)
                        }
                        BallState::Held { .. } => Some(2),
                        BallState::Dead(_) => Some(3),
                        BallState::Flight { .. } => None,
                    };
                    if let Some(k) = end {
                        outcome[bucket][k] += 1;
                        open = None;
                    }
                }
                match s.ball {
                    BallState::Held { holder } => {
                        let h = holder as usize;
                        held[side_ix(h)][role_ix(h)] += 1;
                        let x = prev.pos(h).x;
                        let depth = if s.attacking(s.players[h].side).direction() > 0.0 {
                            x
                        } else {
                            105.0 - x
                        };
                        third[side_ix(h)][((depth / 35.0) as usize).min(2)] += 1;
                    }
                    BallState::Flight {
                        flight,
                        intent: FlightIntent::Pass { receiver },
                    } if flight.kick_ms == s.now_ms() => {
                        let r = receiver as usize;
                        // The passer: nearest team-mate to the kick point.
                        let from = flight.pos_at(s.now_ms()).xy();
                        let side = s.players[r].side;
                        let p = (0..22)
                            .filter(|&i| i != r && s.players[i].side == side)
                            .min_by(|&a, &b| {
                                prev.pos(a)
                                    .distance(from)
                                    .total_cmp(&prev.pos(b).distance(from))
                            })
                            .unwrap();
                        passes_from[side_ix(p)][role_ix(p)] += 1;
                        let near = (0..22)
                            .filter(|&i| s.players[i].side != side && !s.players[i].sent_off)
                            .map(|i| prev.pos(i).distance(prev.pos(r)))
                            .fold(f32::MAX, f32::min);
                        let bucket = [3.0, 6.0, 12.0].iter().position(|&x| near < x).unwrap_or(3);
                        est[bucket] +=
                            f64::from(fm_match::DecisionSystem::pass_success(s, &prev, p, r));
                        open = Some((r, bucket));
                        passes_to[side_ix(r)][role_ix(r)] += 1;
                        let adv = (prev.pos(r).x - from.x) * s.attacking(side).direction();
                        dir[side_ix(p)][if adv < -2.0 {
                            0
                        } else if adv <= 2.0 {
                            1
                        } else {
                            2
                        }] += 1;
                        if prev_ht + 1 >= forced_at {
                            forced[side_ix(p)] += 1;
                        }
                    }
                    _ => {}
                }
                prev = TickFrame::capture(s);
                prev_ht = s.holder_ticks;
            }
        }
        println!("== {name} ({n} matches)");
        for (b, label) in ["<3 m", "3-6 m", "6-12 m", "12+ m"].iter().enumerate() {
            let o = outcome[b];
            let t = o.iter().sum::<u64>().max(1) as f64;
            let failed = (o[1] + o[2] + o[3]).max(1) as f64;
            println!(
                "  nearest opponent to receiver {label}: {:.0} passes/match | receiver {:.1}% (estimated {:.1}%) | of the failed: team-mate {:.0}% opponent {:.0}% dead {:.0}%",
                t / n as f64,
                100.0 * o[0] as f64 / t,
                100.0 * est[b] / t,
                100.0 * o[1] as f64 / failed,
                100.0 * o[2] as f64 / failed,
                100.0 * o[3] as f64 / failed,
            );
        }
        for (sx, label) in [(0, "home"), (1, "away")] {
            let pct = |v: &[u64]| {
                let t: u64 = v.iter().sum::<u64>().max(1);
                v.iter()
                    .zip(ROLES)
                    .filter(|(c, _)| **c > 0)
                    .map(|(c, r)| format!("{r:?} {:.1}%", 100.0 * *c as f64 / t as f64))
                    .collect::<Vec<_>>()
                    .join(" | ")
            };
            let total: u64 = passes_from[sx].iter().sum::<u64>().max(1);
            let d = dir[sx];
            let dt = d.iter().sum::<u64>().max(1) as f64;
            let th = third[sx];
            let tt = th.iter().sum::<u64>().max(1) as f64;
            println!("  {label}: held-ball ticks  {}", pct(&held[sx]));
            println!("  {label}: passes made      {}", pct(&passes_from[sx]));
            println!("  {label}: passes received  {}", pct(&passes_to[sx]));
            println!(
                "  {label}: {:.0} passes/match | back {:.1}% square {:.1}% forward {:.1}% | forced release {:.1}% | ball held in own/middle/final third {:.0}/{:.0}/{:.0}%",
                total as f64 / n as f64,
                100.0 * d[0] as f64 / dt,
                100.0 * d[1] as f64 / dt,
                100.0 * d[2] as f64 / dt,
                100.0 * forced[sx] as f64 / total as f64,
                100.0 * th[0] as f64 / tt,
                100.0 * th[1] as f64 / tt,
                100.0 * th[2] as f64 / tt,
            );
        }
    }
}
