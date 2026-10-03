//! `cargo run --release -p fm-match --example calibrate -- key=value ...` —
//! match statistics under tuning overrides (calibration sweeps, SPEC Fase 5).

// Diagnostic tool: approximate float stats over counters are fine here.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::items_after_statements,
    clippy::format_collect,
    clippy::many_single_char_names
)]
use fm_match::demo::demo_match;
use fm_match::{EventKind, LodLevel, MatchEngine, TuningParams};

fn apply(t: &mut TuningParams, key: &str, v: f32) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let u = v as u32;
    match key {
        "shoot_range" => t.decision.shoot_range = v,
        "block_radius" => t.xg.block_radius = v,
        "act_margin" => t.value.act_margin = v,
        "through_balls" => t.value.through_balls = v > 0.5,
        "run_ticks" => t.runs.run_ticks = u,
        "max_runners" => t.runs.max_runners = u,
        "mark_dist" => t.runs.mark_dist = v,
        "mark_urgency" => t.runs.mark_urgency = v,
        "decision_cadence_ticks" => t.decision.decision_cadence_ticks = u,
        "pass_intercept_max" => t.value.pass_intercept_max = v,
        "hold_keep_pressed" => t.value.hold_keep_pressed = v,
        "forced_release_ticks" => t.value.forced_release_ticks = u,
        "keeper_base" => t.shot.keeper_base = v,
        "control_pressure" => t.control.pressure_penalty = v,
        "pressure_radius" => t.decision.pressure_radius = v,
        "min_hold_ticks" => t.decision.min_hold_ticks = u,
        "throw_in" => t.restart.throw_in = u,
        "goal_kick" => t.restart.goal_kick = u,
        "free_kick" => t.restart.free_kick = u,
        "corner" => t.restart.corner = u,
        "kick_off" => t.restart.kick_off = u,
        "penalty" => t.restart.penalty = u,
        "arrival_speed" => t.pass.arrival_speed = v,
        "angle_base" => t.pass.angle_base = v,
        "length_base" => t.pass.length_base = v,
        "challenge_threshold" => t.defending.challenge_threshold = v,
        "w_fresh" => t.defending.w_fresh = v,
        "foul_base" => t.discipline.foul_base = v,
        "on_target_base" => t.shot.on_target_base = v,
        "intercept_radius" => t.control.intercept_radius = v,
        "control_base" => t.control.base = v,
        _ => panic!("unknown key {key}"),
    }
}

fn main() {
    let mut tuning = TuningParams::default();
    for arg in std::env::args().skip(1) {
        let (k, v) = arg.split_once('=').expect("key=value");
        apply(&mut tuning, k, v.parse().expect("number"));
    }
    // FM_MATCHES overrides the 30 matches per run (180 for a full measure).
    let n: u32 = std::env::var("FM_MATCHES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);
    let (mut goals, mut shots, mut on, mut passes, mut ok, mut tackles, mut fouls) =
        (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let mut dead = 0.0;
    let mut decisions = 0.0;
    let mut throughs = 0.0;
    let (mut through_ok, mut passes_all, mut passes_ok_all) = (0.0, 0.0, 0.0);
    // Possession: held ticks per side, possession spells, and xG per shot.
    let (mut held_home, mut held_all, mut spells) = (0u64, 0u64, 0u64);
    let (mut xg_sum, mut xg_shots) = (0.0f64, 0u64);
    // Off-ball runs: started, ticks with a runner, runner-ticks past the line.
    let (mut runs_started, mut run_ticks, mut runner_ticks, mut beyond) = (0u64, 0u64, 0u64, 0u64);
    // Defensive line of the side without the ball (second-last player's
    // distance from its own goal line) on held-ball ticks: all, and those
    // with an opposing run live.
    let (mut line_sum, mut line_n, mut line_run_sum, mut line_run_n) = (0.0f64, 0u64, 0.0f64, 0u64);
    let (mut rel_pass, mut rel_shot) = ([0u64; 6], [0u64; 6]);
    // How pass flights end: receiver, other teammate, opponent, out of play,
    // knocked loose (miscontrol / deflection).
    let mut pass_end = [0u64; 5];
    // FM_SEEDS_FROM shifts the seed set (to measure sampling noise).
    let first: u64 = std::env::var("FM_SEEDS_FROM")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    for seed in first..first + u64::from(n) {
        // FM_FORMATIONS=433,442 overrides the demo's 4-4-2 (home) vs 4-3-3.
        let (db, mut setup) = match std::env::var("FM_FORMATIONS").ok().as_deref() {
            Some(spec) => {
                let f = |s: &str| match s {
                    "442" => fm_match::Formation::F442,
                    "433" => fm_match::Formation::F433,
                    other => panic!("unknown formation {other}"),
                };
                let (h, a) = spec.split_once(',').expect("home,away");
                fm_match::demo::demo_match_with(seed, f(h), f(a))
            }
            None => demo_match(seed),
        };
        setup.tuning = tuning;
        let mut e = MatchEngine::new(&setup, &db);
        let mut dead_ticks = 0u32;
        let mut ticks = 0u32;
        let mut prev_ht = 0u32;
        let mut prev_held = false;
        let mut open_pass: Option<(u8, fm_match::Side)> = None;
        let mut last_side: Option<fm_match::Side> = None;
        let mut prev_frame = fm_match::TickFrame::capture(e.state());
        while !e.is_finished() {
            e.tick_logic();
            ticks += 1;
            {
                let s = e.state();
                if let fm_match::state::BallState::Flight { flight, intent } = s.ball {
                    if prev_held && flight.kick_ms == s.now_ms() {
                        let b = [5, 10, 15, 20, 55]
                            .iter()
                            .position(|&x| prev_ht + 1 < x)
                            .unwrap_or(5);
                        match intent {
                            fm_match::state::FlightIntent::Pass { .. } => rel_pass[b] += 1,
                            fm_match::state::FlightIntent::Shot { shooter, .. } => {
                                rel_shot[b] += 1;
                                // Unblocked xG of the shot, as the resolver
                                // computes it, from where it was struck.
                                let p = &s.players[shooter as usize];
                                let end = s.attacking(p.side);
                                let pos = prev_frame.pos(shooter as usize);
                                let skill = fm_match::xg::shot_skill(
                                    &p.attrs,
                                    pos.distance(end.goal_centre()),
                                    s.tuning.decision.long_shot_dist,
                                );
                                xg_sum += f64::from(
                                    fm_match::xg::xg(pos, end, &s.tuning.xg)
                                        * fm_match::xg::finisher(skill, &s.tuning.xg),
                                );
                                xg_shots += 1;
                            }
                            fm_match::state::FlightIntent::Loose => {}
                        }
                    }
                }
                use fm_match::state::{BallState, FlightIntent};
                if let Some((recv, side)) = open_pass {
                    let end = match s.ball {
                        BallState::Held { holder } if holder == recv => Some(0),
                        BallState::Held { holder } if s.players[holder as usize].side == side => {
                            Some(1)
                        }
                        BallState::Held { .. } => Some(2),
                        BallState::Dead(_) => Some(3),
                        BallState::Flight { flight, intent } => {
                            if flight.kick_ms == s.now_ms()
                                && !matches!(intent, FlightIntent::Pass { .. })
                            {
                                Some(4)
                            } else {
                                None
                            }
                        }
                    };
                    if let Some(k) = end {
                        pass_end[k] += 1;
                        open_pass = None;
                    }
                }
                if let BallState::Flight {
                    flight,
                    intent: FlightIntent::Pass { receiver },
                } = s.ball
                {
                    if flight.kick_ms == s.now_ms() {
                        open_pass = Some((receiver, s.players[receiver as usize].side));
                    }
                }
                if let fm_match::state::BallState::Held { holder } = s.ball {
                    let side = s.players[holder as usize].side;
                    held_all += 1;
                    if side == fm_match::Side::Home {
                        held_home += 1;
                    }
                    if last_side != Some(side) {
                        spells += 1;
                        last_side = Some(side);
                    }
                }
                let mut f = fm_match::TickFrame::capture(s);
                f.observe_ball(s);
                f.set_phases(s.phases);
                f.compute_offside(s);
                let mut any = false;
                for (i, p) in s.players.iter().enumerate() {
                    if p.run_until == s.tick + s.tuning.runs.run_ticks {
                        runs_started += 1;
                    }
                    if fm_match::runs::active_run(s, &f, i).is_some() {
                        any = true;
                        runner_ticks += 1;
                        if let Some(line) = f.offside_line(p.side) {
                            let dir = s.attacking(p.side).direction();
                            if (f.pos(i).x - line) * dir > 0.0 {
                                beyond += 1;
                            }
                        }
                    }
                }
                if any {
                    run_ticks += 1;
                }
                if let fm_match::state::BallState::Held { holder } = s.ball {
                    let side = s.players[holder as usize].side;
                    if let Some(line) = f.offside_line(side) {
                        let depth = f64::from((line - s.attacking(side).goal_line_x()).abs());
                        line_sum += depth;
                        line_n += 1;
                        if any {
                            line_run_sum += depth;
                            line_run_n += 1;
                        }
                    }
                }
                prev_frame = fm_match::TickFrame::capture(s);
                prev_held = matches!(s.ball, fm_match::state::BallState::Held { .. });
                prev_ht = s.holder_ticks;
            }
            if matches!(e.state().ball, fm_match::state::BallState::Dead(_)) {
                dead_ticks += 1;
            }
        }
        let _ = LodLevel::Abstract;
        dead += f64::from(dead_ticks) / f64::from(ticks);
        for ev in e.events() {
            match ev.kind {
                EventKind::Goal { .. } => goals += 1.0,
                EventKind::Shot { on_target, .. } => {
                    shots += 1.0;
                    if on_target {
                        on += 1.0;
                    }
                }
                EventKind::Foul { .. } => fouls += 1.0,
                _ => {}
            }
        }
        let s = e.state();
        passes += f64::from(s.teams[0].passes + s.teams[1].passes);
        ok += f64::from(s.teams[0].passes_completed + s.teams[1].passes_completed);
        tackles += f64::from(s.teams[0].tackles + s.teams[1].tackles);
        decisions += f64::from(s.teams[0].decisions + s.teams[1].decisions);
        throughs += f64::from(s.teams[0].through_passes + s.teams[1].through_passes);
        through_ok += f64::from(s.teams[0].through_completed + s.teams[1].through_completed);
        passes_all += f64::from(s.teams[0].passes + s.teams[1].passes);
        passes_ok_all += f64::from(s.teams[0].passes_completed + s.teams[1].passes_completed);
    }
    let n = f64::from(n);
    println!(
        "goals {:.2} | shots {:.1} ({:.1}) | passes {:.0} ({:.0}%) | tackles {:.0} | fouls {:.1} | dead {:.0}%",
        goals / n,
        shots / n,
        on / n,
        passes / n,
        100.0 * ok / passes.max(1.0),
        tackles / n,
        fouls / n,
        100.0 * dead / n
    );
    let fmt = |h: &[u64; 6]| {
        let t: u64 = h.iter().sum::<u64>().max(1);
        h.iter()
            .map(|&c| format!("{:4.0}%", 100.0 * c as f64 / t as f64))
            .collect::<String>()
    };
    let pe: u64 = pass_end.iter().sum::<u64>().max(1);
    println!(
        "  pass end: receiver {:.0}% | teammate {:.0}% | opponent {:.0}% | out {:.0}% | loose {:.0}%",
        100.0 * pass_end[0] as f64 / pe as f64,
        100.0 * pass_end[1] as f64 / pe as f64,
        100.0 * pass_end[2] as f64 / pe as f64,
        100.0 * pass_end[3] as f64 / pe as f64,
        100.0 * pass_end[4] as f64 / pe as f64
    );
    println!(
        "  carrier decisions per match: {:.0} | through balls {:.1}",
        decisions / n,
        throughs / n
    );
    println!(
        "  completion: through balls {:.1}% | other passes {:.1}%",
        100.0 * through_ok / f64::max(throughs, 1.0),
        100.0 * (passes_ok_all - through_ok) / f64::max(passes_all - throughs, 1.0)
    );
    println!(
        "  runs/match {:.0} | ticks with a runner {:.1}% | runners per tick {:.3} | runner-ticks past the line {:.1}%",
        runs_started as f64 / n,
        100.0 * run_ticks as f64 / (54_000.0 * n),
        runner_ticks as f64 / (54_000.0 * n),
        100.0 * beyond as f64 / runner_ticks.max(1) as f64
    );
    println!(
        "  defensive line depth {:.2} m (held ball) | {:.2} m (with a run live)",
        line_sum / line_n.max(1) as f64,
        line_run_sum / line_run_n.max(1) as f64
    );
    println!(
        "  possession home {:.1}% | spell {:.1} s held | xG/shot {:.3} | xG/match {:.2}",
        100.0 * held_home as f64 / held_all.max(1) as f64,
        held_all as f64 / spells.max(1) as f64 / 10.0,
        xg_sum / xg_shots.max(1) as f64,
        xg_sum / n
    );
    println!(
        "  release ticks [<5 <10 <15 <20 <55 55+] pass:{} shot:{}",
        fmt(&rel_pass),
        fmt(&rel_shot)
    );
}
