//! Phase 4 exit criteria (spec Fase 4): cross-LOD consistency and
//! determinism across runs. Parity native↔WASM lives in `parity.rs` and
//! `fm-wasm/tests/web.rs`.

use fm_match::demo::demo_match;
use fm_match::state::BallState;
use fm_match::{
    EventKind, LodLevel, MatchEngine, MatchSnapshot, Mentality, Pressing, Role, Side, Tactics,
};

fn play(seed: u64, lod: LodLevel) -> (MatchEngine, usize) {
    let (db, setup) = demo_match(seed);
    let mut engine = MatchEngine::new(&setup, &db);
    let mut snapshots = 0;
    engine.run(lod, |_| snapshots += 1);
    (engine, snapshots)
}

/// BLOCKING (spec Fase 4.5): same seed ⇒ identical score, scorers, cards
/// and shots in Full, Reduced and Abstract — and, stronger, an identical
/// final match state, bit for bit.
#[test]
fn test_cross_lod_consistency() {
    for seed in [1_u64, 7, 42] {
        let (full, n_full) = play(seed, LodLevel::Full);
        let (reduced, n_reduced) = play(seed, LodLevel::Reduced);
        let (abstract_, n_abstract) = play(seed, LodLevel::Abstract);

        // LOD really changes the sampling rate…
        assert_eq!(n_full, 6 * n_reduced, "Full samples 60 Hz, Reduced 10 Hz");
        assert_eq!(n_abstract, 0);
        assert!(n_reduced > 50_000);

        // …and nothing else.
        assert_eq!(
            full.events(),
            reduced.events(),
            "seed {seed}: Full vs Reduced"
        );
        assert_eq!(
            full.events(),
            abstract_.events(),
            "seed {seed}: Full vs Abstract"
        );
        assert_eq!(full.state(), reduced.state(), "seed {seed}: final state");
        assert_eq!(full.state(), abstract_.state(), "seed {seed}: final state");

        // Spell out the spec's list explicitly too.
        let pick = |e: &MatchEngine| {
            e.events()
                .iter()
                .filter(|ev| {
                    matches!(
                        ev.kind,
                        EventKind::Goal { .. } | EventKind::Card { .. } | EventKind::Shot { .. }
                    )
                })
                .copied()
                .collect::<Vec<_>>()
        };
        assert_eq!(pick(&full), pick(&abstract_));
        assert!(full.is_finished() && abstract_.is_finished());
    }
}

/// Spec Fase 4.6: same seed ⇒ same positions at logical tick 500.
#[test]
fn test_determinism_across_runs() {
    let at_tick = |seed| -> MatchSnapshot {
        let (db, setup) = demo_match(seed);
        let mut e = MatchEngine::new(&setup, &db);
        for _ in 0..500 {
            e.tick_logic();
        }
        assert_eq!(e.state().tick, 500);
        e.sample(LodLevel::Reduced, e.state().now_ms())
            .expect("snapshot")
    };
    let bits = |s: &MatchSnapshot| -> Vec<u32> {
        s.players
            .iter()
            .flat_map(|p| [p.pos.x.to_bits(), p.pos.y.to_bits()])
            .chain([s.ball.x.to_bits(), s.ball.y.to_bits(), s.ball.z.to_bits()])
            .collect()
    };
    for seed in [3_u64, 99] {
        let a = at_tick(seed);
        let b = at_tick(seed);
        assert_eq!(bits(&a), bits(&b), "seed {seed}");
        assert_eq!(a, b);
    }
    // A different seed must actually produce a different match.
    assert_ne!(bits(&at_tick(3)), bits(&at_tick(4)));
}

/// Sampling is read-only: interleaving samples at arbitrary times does not
/// change the match.
#[test]
fn sampling_has_no_side_effects() {
    let (db, setup) = demo_match(11);
    let mut quiet = MatchEngine::new(&setup, &db);
    let mut noisy = MatchEngine::new(&setup, &db);
    for _ in 0..3_000 {
        quiet.tick_logic();
        noisy.tick_logic();
        let now = noisy.state().now_ms();
        for off in [99, 0, 50, 13, 77] {
            let _ = noisy.sample(LodLevel::Full, now + off);
        }
    }
    assert_eq!(quiet.state(), noisy.state());
}

/// Guards against a degenerate engine (Phase 4 calibration baseline; Phase 5
/// tightens toward real football). Bounds are deliberately wide.
#[test]
fn match_statistics_are_plausible() {
    let seeds = 0_u64..6;
    let n = 6.0;
    let (mut goals, mut shots, mut fouls, mut reds) = (0.0, 0.0, 0.0, 0.0);
    for seed in seeds {
        let (e, _) = play(seed, LodLevel::Abstract);
        assert_eq!(e.state().events.dropped(), 0, "event log overflow");
        for ev in e.events() {
            match ev.kind {
                EventKind::Goal { .. } => goals += 1.0,
                EventKind::Shot { .. } => shots += 1.0,
                EventKind::Foul { .. } => fouls += 1.0,
                EventKind::Card {
                    card: fm_match::CardKind::Red,
                    ..
                } => reds += 1.0,
                _ => {}
            }
        }
        // Every match has both halves and a final whistle.
        assert!(e.events().iter().any(|ev| ev.kind == EventKind::HalfTime));
        assert_eq!(
            e.events().last().map(|ev| ev.kind),
            Some(EventKind::FullTime)
        );
    }
    let (goals, shots, fouls, reds) = (goals / n, shots / n, fouls / n, reds / n);
    assert!((0.5..=7.0).contains(&goals), "goals/match {goals}");
    assert!((3.0..=80.0).contains(&shots), "shots/match {shots}");
    // Real football has ~22 fouls a match; the floor of 2 is only a
    // regression guard. The engine is at 2.8 (180-match mean) and will go
    // lower during (c1).
    assert!((2.0..=60.0).contains(&fouls), "fouls/match {fouls}");
    assert!(reds <= 2.0, "reds/match {reds}");
}

/// Spec Fase 5 (c1) item 4: after a run ends the striker/winger goes back to
/// its formation anchor (the shape does not collapse over the match).
/// Measured on one full match: time from the end of a run until the runner
/// is back within 5 m of its anchor, and the runners' mean distance to
/// their anchors per 15-minute window.
#[test]
fn runners_return_to_shape_after_a_run() {
    use fm_match::runs::is_runner;
    use fm_match::TickFrame;

    let (db, setup) = demo_match(3);
    let mut e = MatchEngine::new(&setup, &db);
    // Per player: tick its last run ended, if it has not returned yet.
    let mut ended: [Option<u32>; 22] = [None; 22];
    let mut returns: Vec<u32> = Vec::new();
    let mut unreturned = 0_u32;
    let mut window = [(0.0_f64, 0_u32); 6];
    while !e.is_finished() {
        e.tick_logic();
        let s = e.state();
        let mut f = TickFrame::capture(s);
        f.observe_ball(s);
        f.set_phases(s.phases);
        f.compute_anchors(s);
        for (i, p) in s.players.iter().enumerate() {
            if !is_runner(p.role) || !p.active() {
                continue;
            }
            let dist = f.pos(i).distance(f.anchor(i));
            let w = ((s.tick / 9_000) as usize).min(5);
            window[w].0 += f64::from(dist);
            window[w].1 += 1;
            if p.run_until == s.tick {
                ended[i] = Some(s.tick);
            } else if p.run_until > s.tick {
                // A new run started before it got back: not a return.
                if ended[i].take().is_some() {
                    unreturned += 1;
                }
            } else if let Some(t) = ended[i] {
                if dist < 5.0 {
                    returns.push(s.tick - t);
                    ended[i] = None;
                }
            }
        }
    }
    returns.sort_unstable();
    // Percentile in percent (integer index arithmetic, no float casts).
    let pct = |q: usize| returns[(returns.len() - 1) * q / 100];
    let means: Vec<f64> = window
        .iter()
        .map(|(sum, n)| sum / f64::from((*n).max(1)))
        .collect();
    println!(
        "runs ended {} (+{unreturned} rerun before returning); back within 5 m after p50 {} / p90 {} / max {} ticks; mean distance to anchor per 15 min: {means:.1?}",
        returns.len(),
        pct(50),
        pct(90),
        returns.last().copied().unwrap_or(0)
    );
    assert!(returns.len() > 100, "runs happen and end");
    assert!(pct(50) <= 30, "median return ≤ 3 s");
    assert!(pct(90) <= 60, "90% back within 6 s");
    // No drift: the last 15 minutes are not looser than the first.
    assert!(
        means[5] <= means[0] * 1.5 + 2.0,
        "shape collapses: {means:?}"
    );
}

/// A restart is never lost at once: the taker takes the ball inside the
/// lines, so the "carried ball out of play" test cannot hand it straight to
/// the other side (it used to, for 48% of the restarts: the taker stood on
/// the line and the ball he carries 0.5 m ahead of him was outside).
#[test]
fn restarts_are_not_lost_at_once() {
    use fm_match::state::BallState;

    let (mut taken, mut lost_at_once) = (0_u32, 0_u32);
    for seed in 0..30 {
        let (db, setup) = demo_match(seed);
        let mut e = MatchEngine::new(&setup, &db);
        let mut was_dead = false;
        // Taker and tick of a restart just taken.
        let mut fresh: Option<(u8, u32)> = None;
        let mut tackle_count = 0_u16;
        while !e.is_finished() {
            e.tick_logic();
            let s = e.state();
            let tackled = s.teams[0].tackles + s.teams[1].tackles > tackle_count;
            tackle_count = s.teams[0].tackles + s.teams[1].tackles;
            match s.ball {
                BallState::Held { holder } => {
                    if was_dead {
                        taken += 1;
                        fresh = Some((holder, s.tick));
                    } else if fresh.is_some_and(|(taker, _)| taker != holder) {
                        fresh = None;
                    }
                }
                BallState::Dead(r) => {
                    if let Some((taker, at)) = fresh.take() {
                        let side = s.players[taker as usize].side;
                        if r.side != side && !tackled && s.tick - at < 5 {
                            lost_at_once += 1;
                        }
                    }
                }
                BallState::Flight { .. } => fresh = None,
            }
            was_dead = matches!(s.ball, BallState::Dead(_));
        }
    }
    println!("restarts taken {taken}, lost out of play within 5 ticks {lost_at_once}");
    assert!(taken > 1_000, "restarts are taken");
    assert_eq!(lost_at_once, 0, "restarts lost straight out of play");
}

/// Mean depth of `side`'s ten outfielders toward the goal it attacks (m).
fn block_depth(engine: &MatchEngine, side: Side) -> f32 {
    let state = engine.state();
    let goal = state.attacking(side).opposite();
    let now = state.now_ms();
    let (sum, n) = state
        .players
        .iter()
        .filter(|p| p.side == side && p.active() && p.role != Role::Goalkeeper)
        .fold((0.0, 0.0), |(sum, n), p| {
            (
                sum + (p.pos(now).x - goal.goal_line_x()) * -goal.direction(),
                n + 1.0,
            )
        });
    sum / n
}

/// Acceptance criterion 3 (spec Section 9, item 19; Fase 6, 6C): switching
/// a team from Defensive to Attacking in the middle of a match moves its
/// centre of mass up by at least 5 m within 5 s. Measured against the same
/// match left on Defensive, so that the ball moving the block does not
/// count: the two only differ by the instruction.
#[test]
fn mentality_moves_the_block_five_metres_in_five_seconds() {
    let tactics = |mentality| Tactics {
        mentality,
        ..Tactics::default()
    };
    let mut gains = Vec::new();
    for seed in [3, 7, 11] {
        for side in [Side::Home, Side::Away] {
            for switch_at in [1_500, 9_000, 20_000, 33_000] {
                let (db, setup) = demo_match(seed);
                let mut control = MatchEngine::new(&setup, &db);
                control.set_tactics(side, tactics(Mentality::Defensive));
                for _ in 0..switch_at {
                    control.tick_logic();
                }
                let (db, setup) = demo_match(seed);
                let mut switched = MatchEngine::new(&setup, &db);
                switched.set_tactics(side, tactics(Mentality::Defensive));
                for _ in 0..switch_at {
                    switched.tick_logic();
                }
                assert_eq!(switched.state(), control.state(), "same match so far");
                switched.set_tactics(side, tactics(Mentality::Attacking));
                let mut by_second = [0.0; 5];
                for gain in &mut by_second {
                    for _ in 0..10 {
                        control.tick_logic();
                        switched.tick_logic();
                    }
                    *gain = block_depth(&switched, side) - block_depth(&control, side);
                }
                gains.push(by_second[4]);
            }
        }
    }
    gains.sort_by(f32::total_cmp);
    println!(
        "after 5 s: min {:.1} median {:.1} max {:.1}",
        gains[0],
        gains[gains.len() / 2],
        gains[gains.len() - 1]
    );
    assert!(gains[0] >= 5.0, "least gain after 5 s: {:.1} m", gains[0]);
}

/// Distance from the ball carrier to the nearest outfielder of the other
/// side, when the carrier is outside that side's box zone; `None` otherwise.
fn carrier_gap(engine: &MatchEngine, defending: Side) -> Option<f32> {
    let state = engine.state();
    let BallState::Held { holder } = state.ball else {
        return None;
    };
    let carrier = &state.players[holder as usize];
    if carrier.side == defending {
        return None;
    }
    let now = state.now_ms();
    let at = carrier.pos(now);
    let own_goal = state.attacking(defending).opposite().goal_centre();
    if at.distance(own_goal) <= state.tuning.defending.zone_box_dist {
        return None;
    }
    state
        .players
        .iter()
        .filter(|p| p.side == defending && p.active() && p.role != Role::Goalkeeper)
        .map(|p| p.pos(now).distance(at))
        .min_by(f32::total_cmp)
}

/// Acceptance criterion 5 (spec Fase 6, 6C): switching the pressing level
/// in the middle of a match moves the distance from the carrier to the
/// nearest defender within 5 s — tighter for `UltraHigh`, looser for `Low`.
/// Measured against the same match left on `Medium`, over the 5 s that
/// follow the switch.
#[test]
fn pressing_moves_the_distance_to_the_carrier_within_five_seconds() {
    // Sum of gaps and number of ticks with one, for [Medium, UltraHigh, Low].
    let mut gaps = [(0.0_f32, 0.0_f32); 3];
    for seed in [3, 7, 11] {
        for switch_at in [1_500, 9_000, 20_000, 33_000] {
            let mut engines = [Pressing::Medium, Pressing::UltraHigh, Pressing::Low].map(|to| {
                let (db, setup) = demo_match(seed);
                let mut engine = MatchEngine::new(&setup, &db);
                for _ in 0..switch_at {
                    engine.tick_logic();
                }
                engine.set_tactics(
                    Side::Home,
                    Tactics {
                        pressing: to,
                        ..Tactics::default()
                    },
                );
                engine
            });
            for (engine, (sum, n)) in engines.iter_mut().zip(&mut gaps) {
                for _ in 0..50 {
                    engine.tick_logic();
                    if let Some(gap) = carrier_gap(engine, Side::Home) {
                        *sum += gap;
                        *n += 1.0;
                    }
                }
            }
        }
    }
    let [medium, ultra, low] = gaps.map(|(sum, n)| sum / n);
    println!("gap over the 5 s after the switch: Low {low:.2} m, Medium {medium:.2} m, UltraHigh {ultra:.2} m");
    assert!(
        ultra < medium - 0.5,
        "UltraHigh {ultra:.2} vs Medium {medium:.2}"
    );
    assert!(low > medium + 0.5, "Low {low:.2} vs Medium {medium:.2}");
}
