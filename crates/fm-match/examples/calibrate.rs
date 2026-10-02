//! `cargo run --release -p fm-match --example calibrate -- key=value ...` —
//! match statistics under tuning overrides (calibration sweeps, SPEC Fase 5).

// Diagnostic tool: approximate float stats over counters are fine here.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::items_after_statements,
    clippy::format_collect
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
    let n = 30u32;
    let (mut goals, mut shots, mut on, mut passes, mut ok, mut tackles, mut fouls) =
        (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let mut dead = 0.0;
    let (mut rel_pass, mut rel_shot) = ([0u64; 6], [0u64; 6]);
    // How pass flights end: receiver, other teammate, opponent, out of play,
    // knocked loose (miscontrol / deflection).
    let mut pass_end = [0u64; 5];
    for seed in 0..u64::from(n) {
        let (db, mut setup) = demo_match(seed);
        setup.tuning = tuning;
        let mut e = MatchEngine::new(&setup, &db);
        let mut dead_ticks = 0u32;
        let mut ticks = 0u32;
        let mut prev_ht = 0u32;
        let mut prev_held = false;
        let mut open_pass: Option<(u8, fm_match::Side)> = None;
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
                            fm_match::state::FlightIntent::Shot { .. } => rel_shot[b] += 1,
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
        "  release ticks [<5 <10 <15 <20 <55 55+] pass:{} shot:{}",
        fmt(&rel_pass),
        fmt(&rel_shot)
    );
}
