//! Off-ball runs (spec Fase 5 (c1), item 4): strikers and wingers of the
//! side in possession run into the most open lane up to — never past — the
//! offside line, so that there is somebody to play forward to.
//!
//! Deterministic (no RNG). Runners pick their target on the carrier's
//! decision cadence, all on the same tick, and the offside line is computed
//! only on those ticks (spec: item 3 debt). Between looks the runner keeps
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
    // Compute the line only if somebody could use it.
    if !(start..start + 11).any(|i| eligible(i, state)) {
        return;
    }
    frame.compute_offside(state);
    let Some(line) = frame.offside_line(side) else {
        return;
    };
    let rt = state.tuning.runs;
    let dir = state.attacking(side).direction();
    for i in start..start + 11 {
        if !eligible(i, state) {
            continue;
        }
        let pos = frame.pos(i);
        // Room in front of the runner, up to the line.
        if (line - pos.x) * dir < rt.min_room {
            continue;
        }
        let target = Vec2::new(
            line - dir * rt.onside_margin,
            open_lane_y(state, frame, side, pos.y, line),
        );
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
/// opposing outfielders near the line: the most open lane.
fn open_lane_y(state: &MatchState, frame: &TickFrame, side: Side, from_y: f32, line: f32) -> f32 {
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
    best.1
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
