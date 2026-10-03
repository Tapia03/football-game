//! `cargo run --release -p fm-match --example tackle_stats` — why so few
//! tackles (SPEC Fase 5: ~6.5 a match against ~70 in real football): on
//! every tick with the ball held, what the defending side's challenge
//! decision sees (`DecisionSystem::challenge_score` /
//! `choose_challenger`) and why it does or does not send a tackle in.
//!
//! Each held tick is one of: nobody within `engage_range`; somebody there
//! but all still recovering from a previous challenge; somebody eligible
//! but no score above `challenge_threshold` (contain); a challenger chosen
//! but still outside `tackle_range` (closing); a challenger in range (the
//! tackle goes in). Observed from outside after each tick: the values are
//! those of the next tick's decision. No engine change.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::many_single_char_names
)]
use fm_core::pitch;
use fm_match::state::BallState;
use fm_match::{DecisionSystem, Formation, MatchEngine, Phase, Role, TickFrame};

const CLASSES: [&str; 5] = [
    "nobody within engage range",
    "all in range still recovering",
    "eligible, score below threshold",
    "challenger chosen, closing",
    "challenger in tackle range",
];
const SCORES: [f32; 5] = [0.6, 0.8, 1.0, 1.15, 1.3];

fn pct(v: &[u64]) -> String {
    let total = v.iter().sum::<u64>().max(1) as f64;
    v.iter()
        .map(|&c| format!("{:.1}%", 100.0 * c as f64 / total))
        .collect::<Vec<_>>()
        .join(" | ")
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
        // All held ticks, and those with a defender inside tackle range.
        let mut class = [0u64; 5];
        let mut class_in_reach = [0u64; 5];
        // Best score among the eligible defenders [<0.6 | <0.8 | <1.0 |
        // <1.15 | <1.3 | 1.3+]: within engage range, within tackle range.
        let mut best_engage = [0u64; 6];
        let mut best_reach = [0u64; 6];
        // Mean score terms of the best eligible defender inside tackle
        // range: goal side, fresh, tackling, decisions, aggression,
        // transition, own-box penalty; and their total.
        let mut terms = [0.0f64; 8];
        let mut terms_n = 0u64;
        // When a challenger is chosen: is it the nearest defender, and is
        // somebody else already inside tackle range.
        let (mut chosen, mut chosen_nearest, mut chosen_far_other_in_reach) = (0u64, 0u64, 0u64);
        // Closing spells (a challenger chosen and running in): how they end
        // [tackle | score drops / lost eligibility | ball released].
        let mut closing_end = [0u64; 3];
        let mut closing_ticks = 0u64;
        let mut tackles = 0u64;
        let mut threshold = 0.0;
        for seed in 0..n {
            let (db, setup) = fm_match::demo::demo_match_with(seed, home, away);
            let mut e = MatchEngine::new(&setup, &db);
            let t = e.state().tuning;
            threshold = t.defending.challenge_threshold;
            // A challenger currently closing in: (defender, carrier).
            let mut closing: Option<(usize, usize)> = None;
            while !e.is_finished() {
                e.tick_logic();
                let s = e.state();
                let BallState::Held { holder } = s.ball else {
                    if closing.take().is_some() {
                        closing_end[2] += 1;
                    }
                    continue;
                };
                let h = holder as usize;
                let frame = TickFrame::capture(s);
                // The decision the defence makes next tick.
                let mut next = s.clone();
                next.holder_ticks += 1;
                let mut f = frame;
                f.observe_ball(&next);
                f.set_phases(next.phases);
                let side = s.players[h].side.other();
                let at = frame.pos(h);
                let own_box = s.attacking(side).opposite();
                let keeper_may = t.defending.keeper_smother && pitch::in_penalty_area(at, own_box);
                let (mut in_engage, mut eligible) = (0, 0);
                let mut any_in_reach = false;
                let mut nearest = (f32::MAX, usize::MAX);
                let mut best = (f32::MIN, usize::MAX);
                let mut best_in_reach = (f32::MIN, usize::MAX);
                for (i, p) in s.players.iter().enumerate() {
                    if p.side != side || p.sent_off {
                        continue;
                    }
                    if p.role == Role::Goalkeeper && !keeper_may {
                        continue;
                    }
                    let d = frame.pos(i).distance(at);
                    if d < nearest.0 {
                        nearest = (d, i);
                    }
                    if d >= t.defending.engage_range {
                        continue;
                    }
                    in_engage += 1;
                    any_in_reach |= d < t.duel.tackle_range;
                    if next.tick + 1 < p.tackle_ready_tick {
                        continue;
                    }
                    eligible += 1;
                    let score = DecisionSystem::challenge_score(&next, &f, i, h);
                    if score > best.0 {
                        best = (score, i);
                    }
                    if d < t.duel.tackle_range && score > best_in_reach.0 {
                        best_in_reach = (score, i);
                    }
                }
                let challenger = DecisionSystem::choose_challenger(&next, &f, side, h);
                let k = if in_engage == 0 {
                    0
                } else if eligible == 0 {
                    1
                } else {
                    match challenger {
                        None => 2,
                        Some(c) if frame.pos(c).distance(at) < t.duel.tackle_range => 4,
                        Some(_) => 3,
                    }
                };
                class[k] += 1;
                if any_in_reach {
                    class_in_reach[k] += 1;
                }
                let bucket = |v: f32| SCORES.iter().position(|&x| v < x).unwrap_or(SCORES.len());
                if eligible > 0 {
                    best_engage[bucket(best.0)] += 1;
                }
                if best_in_reach.1 != usize::MAX {
                    best_reach[bucket(best_in_reach.0)] += 1;
                    let d = &s.players[best_in_reach.1];
                    let unit = |v: u8| f64::from(v) / 100.0;
                    let w = &t.defending;
                    let parts = [
                        f64::from(w.w_goal_side)
                            * f64::from(DecisionSystem::goal_side_cos(
                                &next,
                                &f,
                                best_in_reach.1,
                                h,
                            )),
                        if next.holder_ticks < w.fresh_ticks {
                            f64::from(w.w_fresh)
                        } else {
                            0.0
                        },
                        f64::from(w.w_tackling) * unit(d.attrs.technical.tackling),
                        f64::from(w.w_decisions) * unit(d.attrs.mental.decisions),
                        f64::from(w.w_aggression) * unit(d.attrs.mental.aggression),
                        if f.phase(side) == Phase::TransitionDefense {
                            f64::from(w.transition_bonus)
                        } else {
                            0.0
                        },
                        if pitch::in_penalty_area(at, own_box) && d.role != Role::Goalkeeper {
                            -f64::from(w.own_box_penalty)
                        } else {
                            0.0
                        },
                    ];
                    for (acc, p) in terms.iter_mut().zip(parts) {
                        *acc += p;
                    }
                    terms[7] += f64::from(best_in_reach.0);
                    terms_n += 1;
                }
                if let Some(c) = challenger {
                    chosen += 1;
                    chosen_nearest += u64::from(c == nearest.1);
                    chosen_far_other_in_reach += u64::from(k == 3 && any_in_reach);
                }
                match (closing, challenger, k) {
                    (Some(_), Some(c), 4) => {
                        closing_end[0] += 1;
                        closing = None;
                        let _ = c;
                    }
                    (Some((d, car)), Some(c), 3) if d == c && car == h => closing_ticks += 1,
                    (Some(_), _, _) => {
                        closing_end[1] += 1;
                        closing = match (challenger, k) {
                            (Some(c), 3) => Some((c, h)),
                            _ => None,
                        };
                    }
                    (None, Some(c), 3) => {
                        closing = Some((c, h));
                        closing_ticks += 1;
                    }
                    _ => {}
                }
            }
            tackles += u64::from(e.state().teams[0].tackles + e.state().teams[1].tackles);
        }
        let held: u64 = class.iter().sum();
        let reach: u64 = class_in_reach.iter().sum();
        println!(
            "== {name} ({n} matches): {:.1} tackles/match; ball held {:.0} ticks/match, a defender inside tackle range in {:.0}",
            tackles as f64 / n as f64,
            held as f64 / n as f64,
            reach as f64 / n as f64
        );
        println!("  what the challenge decision does on a held tick:");
        for (c, label) in CLASSES.iter().enumerate() {
            println!(
                "    {label:<34} {:6.0}/match ({:4.1}%) | with a defender inside tackle range: {:6.0}/match ({:4.1}%)",
                class[c] as f64 / n as f64,
                100.0 * class[c] as f64 / held.max(1) as f64,
                class_in_reach[c] as f64 / n as f64,
                100.0 * class_in_reach[c] as f64 / reach.max(1) as f64
            );
        }
        println!(
            "  best score among the eligible, threshold {threshold} [<0.6 | <0.8 | <1.0 | <1.15 | <1.3 | 1.3+]:"
        );
        println!("    within engage range: {}", pct(&best_engage));
        println!("    within tackle range: {}", pct(&best_reach));
        let m = terms_n.max(1) as f64;
        println!(
            "  mean terms of the best defender inside tackle range: goal side {:+.2} | fresh {:+.2} | tackling {:+.2} | decisions {:+.2} | aggression {:+.2} | transition {:+.2} | own box {:+.2} = {:.2}",
            terms[0] / m,
            terms[1] / m,
            terms[2] / m,
            terms[3] / m,
            terms[4] / m,
            terms[5] / m,
            terms[6] / m,
            terms[7] / m
        );
        println!(
            "  challenger chosen on {:.0} ticks/match: the nearest defender in {:.0}%; still closing while another defender is inside tackle range in {:.0}%",
            chosen as f64 / n as f64,
            100.0 * chosen_nearest as f64 / chosen.max(1) as f64,
            100.0 * chosen_far_other_in_reach as f64 / chosen.max(1) as f64
        );
        let ends = closing_end.iter().sum::<u64>().max(1) as f64;
        println!(
            "  closing spells {:.0}/match, {:.1} ticks each: end in a tackle {:.0}% | score drops or another takes over {:.0}% | ball released first {:.0}%",
            ends / n as f64,
            closing_ticks as f64 / ends,
            100.0 * closing_end[0] as f64 / ends,
            100.0 * closing_end[1] as f64 / ends,
            100.0 * closing_end[2] as f64 / ends
        );
    }
}
