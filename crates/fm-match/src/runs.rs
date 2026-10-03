//! Off-ball runs (spec Fase 5 (c1), item 4): strikers and wingers of the
//! side in possession run into the most open lane up to — never past — the
//! offside line, so that there is somebody to play forward to.
//!
//! A run is decided, not just allowed (spec Fase 5, Caminho A passo 2): it
//! starts only when the carrier can serve it (not pressed, the run ends
//! within passing range) and the side has fewer than `max_runners` runs
//! live; the runner with the most open lane goes.
//!
//! Deterministic (no RNG). Runs start on the carrier's decision cadence,
//! and the offside line is computed only on those ticks (spec: item 3 debt). Between looks the runner keeps
//! running to the same point; the line may move meanwhile — the engine does
//! not flag offside (known bias, spec Fase 5 "Impedimento").

use fm_core::{pitch, Vec2};

use crate::formation::Role;
use crate::phase::{Phase, Possession, Side};
use crate::state::{side_index, BallState, MatchState};
use crate::tick_frame::TickFrame;

/// Roles that make runs in behind.
#[inline]
#[must_use]
pub const fn is_runner(role: Role) -> bool {
    matches!(role, Role::Striker | Role::Winger)
}

/// Phases in which a side's runs are live.
#[inline]
#[must_use]
pub const fn runs_live(phase: Phase) -> bool {
    matches!(phase, Phase::InPossession | Phase::TransitionAttack)
}

/// Starts runs for the side in possession on cadence ticks.
pub fn plan_runs(state: &mut MatchState, frame: &mut TickFrame) {
    let Possession::Team(side) = frame.possession() else {
        return;
    };
    if frame.set_piece() || !runs_live(frame.phase(side)) {
        return;
    }
    let cadence = state.tuning.decision.decision_cadence_ticks.max(1);
    if state.tick % cadence != 0 {
        return;
    }
    // Runs start off a team-mate on the ball.
    let BallState::Held { holder } = state.ball else {
        return;
    };
    let tick = state.tick;
    let start = side_index(side) * 11;
    let eligible = |i: usize, s: &MatchState| {
        let p = &s.players[i];
        i != holder as usize
            && p.active()
            && is_runner(p.role)
            && p.run_until <= tick
            && p.run_ready_tick <= tick
    };
    let rt = state.tuning.runs;
    // Cap (spec Fase 5, Caminho A passo 2): a side has at most `max_runners`
    // runs live, whatever its formation; the other runners stay on their
    // anchors as short support.
    let mut active_runs = 0;
    let mut any_eligible = false;
    for i in start..start + 11 {
        active_runs += u32::from(i != holder as usize && state.players[i].run_until > tick);
        any_eligible |= eligible(i, state);
    }
    if active_runs >= rt.max_runners || !any_eligible {
        return;
    }
    // Trigger: the carrier must be able to serve the run — not pressed now…
    let hpos = frame.pos(holder as usize);
    let dt = &state.tuning.decision;
    let r2 = dt.pressure_radius * dt.pressure_radius;
    let other = side_index(side.other()) * 11;
    if (other..other + 11)
        .any(|j| state.players[j].active() && (frame.pos(j) - hpos).length_squared() < r2)
    {
        return;
    }
    // Compute the line only now that somebody could use it.
    frame.compute_offside(state);
    let Some(line) = frame.offside_line(side) else {
        return;
    };
    let dir = state.attacking(side).direction();
    // The most open lanes run, lowest index first on ties.
    for _ in active_runs..rt.max_runners {
        let mut best: Option<(f32, usize, Vec2)> = None;
        for i in start..start + 11 {
            if !eligible(i, state) {
                continue;
            }
            let pos = frame.pos(i);
            // Room in front of the runner, up to the line.
            if (line - pos.x) * dir < rt.min_room {
                continue;
            }
            let (gap, y) = open_lane(state, frame, side, pos.y, line);
            let target = Vec2::new(line - dir * rt.onside_margin, y);
            // …and the run must end within the carrier's passing range.
            let reach = target.distance(hpos);
            if reach < dt.pass_min_dist || reach > dt.pass_max_dist {
                continue;
            }
            if best.map_or(true, |(g, _, _)| gap > g) {
                best = Some((gap, i, target));
            }
        }
        let Some((_, i, target)) = best else {
            return;
        };
        let off_the_ball = f32::from(state.players[i].attrs.mental.off_the_ball) / 100.0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // ≥ 0, small
        let cooldown = (rt.cooldown_base - rt.cooldown_skill * off_the_ball).max(0.0) as u32;
        let p = &mut state.players[i];
        p.run_target = target;
        p.run_until = tick + rt.run_ticks;
        p.run_ready_tick = p.run_until + cooldown;
    }
}

/// Lateral position (within `lane_reach` of `from_y`) farthest from the
/// opposing outfielders near the line — the most open lane — and how far
/// that is: `(gap, y)`.
fn open_lane(
    state: &MatchState,
    frame: &TickFrame,
    side: Side,
    from_y: f32,
    line: f32,
) -> (f32, f32) {
    let rt = &state.tuning.runs;
    let start = side_index(side.other()) * 11;
    let mut best = (f32::MIN, from_y);
    // Straight ahead first, so it wins ties.
    for k in [0.0, -1.0, 1.0, -2.0, 2.0] {
        let y = (from_y + k * rt.lane_reach / 2.0)
            .clamp(rt.touchline_margin, pitch::WIDTH - rt.touchline_margin);
        let mut gap = f32::MAX;
        for j in start..start + 11 {
            let o = &state.players[j];
            if o.sent_off || o.role == Role::Goalkeeper {
                continue;
            }
            let op = frame.pos(j);
            if (op.x - line).abs() < rt.line_depth {
                gap = gap.min((op.y - y).abs());
            }
        }
        if gap > best.0 {
            best = (gap, y);
        }
    }
    best
}

/// The run target and urgency of player `i`, if it is running now.
#[inline]
#[must_use]
pub fn active_run(state: &MatchState, frame: &TickFrame, i: usize) -> Option<(Vec2, f32)> {
    let p = &state.players[i];
    (p.run_until > state.tick && runs_live(frame.phase(p.side)))
        .then_some((p.run_target, state.tuning.runs.urgency))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::placed_state;

    /// Home striker index in the demo formation.
    fn striker(s: &MatchState) -> usize {
        (0..11)
            .find(|&i| s.players[i].role == Role::Striker)
            .expect("a striker")
    }

    /// Home in possession (holder 5), away back line at x=80 (keeper at
    /// 104), home striker at `(60, 34)`; phases set to home in possession.
    fn setup(defenders: &[(usize, Vec2)]) -> (MatchState, TickFrame, usize) {
        let probe = placed_state(&[], 5);
        let st = striker(&probe);
        let mut placed = vec![
            (st, Vec2::new(60.0, 34.0)),
            (5, Vec2::new(50.0, 30.0)),
            (11, Vec2::new(104.0, 34.0)),
        ];
        placed.extend_from_slice(defenders);
        let mut s = placed_state(&placed, 5);
        let cadence = s.tuning.decision.decision_cadence_ticks;
        s.tick = 10 * cadence;
        // Only `st` may run: the other runners (parked far away) are resting.
        for i in (0..11).filter(|&i| i != st) {
            s.players[i].run_ready_tick = u32::MAX;
        }
        let mut f = TickFrame::capture(&s);
        f.observe_ball(&s);
        f.set_phases([Phase::InPossession, Phase::OutOfPossession]);
        (s, f, st)
    }

    fn back_line() -> Vec<(usize, Vec2)> {
        (12..16)
            .map(|j| {
                (
                    j,
                    Vec2::new(80.0, 14.0 + 13.0 * f32::from(u8::try_from(j - 12).unwrap())),
                )
            })
            .collect()
    }

    #[test]
    fn striker_runs_up_to_the_line_never_past_it() {
        let (mut s, mut f, st) = setup(&back_line());
        plan_runs(&mut s, &mut f);
        let line = f.offside_line(Side::Home).expect("line computed");
        assert_eq!(line, 80.0);
        let p = &s.players[st];
        assert!(p.run_until > s.tick, "a run started");
        assert!(
            p.run_target.x <= line,
            "onside: {} vs line {line}",
            p.run_target.x
        );
        assert!(p.run_target.x > 60.0, "forward");
        assert!(active_run(&s, &f, st).is_some());
    }

    #[test]
    fn run_aims_at_the_open_lane() {
        // Defenders at y = 14, 27, 40, 53: from y=34 the lanes at 34±… the
        // widest gap near the striker is between 27 and 40 (centre 33.5).
        let (mut s, mut f, st) = setup(&back_line());
        plan_runs(&mut s, &mut f);
        let y = s.players[st].run_target.y;
        let nearest = [14.0_f32, 27.0, 40.0, 53.0]
            .iter()
            .map(|d| (d - y).abs())
            .fold(f32::MAX, f32::min);
        assert!(
            nearest >= 5.0,
            "lane y={y} too close to a defender ({nearest} m)"
        );
    }

    #[test]
    fn no_runs_off_the_ball_without_possession_or_room() {
        // Opponents in possession: nothing starts.
        let (mut s, _, st) = setup(&back_line());
        let mut f = TickFrame::capture(&s);
        s.ball = BallState::Held { holder: 13 };
        f.observe_ball(&s);
        f.set_phases([Phase::OutOfPossession, Phase::InPossession]);
        plan_runs(&mut s, &mut f);
        assert_eq!(s.players[st].run_until, 0);
        // Striker already level with the line: no room, no run.
        let line_level: Vec<(usize, Vec2)> = (12..16)
            .map(|j| {
                (
                    j,
                    Vec2::new(62.0, 10.0 + 12.0 * f32::from(u8::try_from(j - 12).unwrap())),
                )
            })
            .collect();
        let (mut s, mut f, st) = setup(&line_level);
        plan_runs(&mut s, &mut f);
        assert_eq!(s.players[st].run_until, 0, "no room to run");
    }

    /// `setup` plus a second home runner `other`, placed at `at` and rested.
    fn setup_two(at: Vec2) -> (MatchState, TickFrame, usize, usize) {
        let (s, _, st) = setup(&back_line());
        let other = (0..11)
            .find(|&i| i != st && is_runner(s.players[i].role))
            .expect("a second runner");
        let mut placed = vec![
            (st, Vec2::new(60.0, 34.0)),
            (other, at),
            (5, Vec2::new(50.0, 30.0)),
            (11, Vec2::new(104.0, 34.0)),
        ];
        placed.extend_from_slice(&back_line());
        let mut s2 = placed_state(&placed, 5);
        s2.tick = s.tick;
        for i in (0..11).filter(|&i| i != st && i != other) {
            s2.players[i].run_ready_tick = u32::MAX;
        }
        let mut f = TickFrame::capture(&s2);
        f.observe_ball(&s2);
        f.set_phases([Phase::InPossession, Phase::OutOfPossession]);
        (s2, f, st, other)
    }

    fn running(s: &MatchState, i: usize) -> bool {
        s.players[i].run_until > s.tick
    }

    #[test]
    fn one_run_at_a_time_and_the_most_open_lane_goes() {
        // `st` at y=34 has the 27–40 gap (6 m either side at best); the
        // other runner at y=6 has the whole flank outside the first
        // defender (y=14): the wider lane, so it is the one that runs.
        let (mut s, mut f, st, other) = setup_two(Vec2::new(60.0, 6.0));
        plan_runs(&mut s, &mut f);
        assert!(running(&s, other), "the most open lane runs");
        assert!(!running(&s, st), "cap of one run per side");
        // While that run is live nobody else starts one.
        s.tick += s.tuning.decision.decision_cadence_ticks;
        plan_runs(&mut s, &mut f);
        assert!(!running(&s, st), "still capped");
        // Once it is over, the rested runner can go.
        s.tick = s.players[other]
            .run_until
            .next_multiple_of(s.tuning.decision.decision_cadence_ticks);
        plan_runs(&mut s, &mut f);
        assert!(running(&s, st), "the next run starts after the first ends");
    }

    #[test]
    fn a_higher_cap_lets_more_runners_go() {
        let (mut s, mut f, st, other) = setup_two(Vec2::new(60.0, 6.0));
        s.tuning.runs.max_runners = 2;
        plan_runs(&mut s, &mut f);
        assert!(running(&s, st) && running(&s, other));
    }

    #[test]
    fn no_run_when_the_carrier_cannot_serve_it() {
        // Pressed carrier: an opponent inside the pressure radius.
        let mut pressed = back_line();
        pressed.push((16, Vec2::new(51.0, 30.0)));
        let (mut s, mut f, st) = setup(&pressed);
        plan_runs(&mut s, &mut f);
        assert!(!running(&s, st), "pressed carrier: no run");
        // Carrier too far for the pass: the run would end out of range.
        let (mut s, _, st) = setup(&back_line());
        let now = s.now_ms();
        let far = Vec2::new(20.0, 30.0);
        s.players[5].traj =
            crate::kinematics::PlayerKinematics::plan_trajectory(far, far, 0.0, now);
        let mut f = TickFrame::capture(&s);
        f.observe_ball(&s);
        f.set_phases([Phase::InPossession, Phase::OutOfPossession]);
        plan_runs(&mut s, &mut f);
        assert!(!running(&s, st), "run out of passing range: no run");
    }

    #[test]
    fn only_strikers_and_wingers_run() {
        assert!(is_runner(Role::Striker) && is_runner(Role::Winger));
        for r in [
            Role::Goalkeeper,
            Role::CentreBack,
            Role::FullBack,
            Role::CentralMidfielder,
            Role::AttackingMidfielder,
        ] {
            assert!(!is_runner(r));
        }
    }
}
