//! `TickFrame`: everything about the current logical tick that is computed
//! once and then only read (spec Fase 5, `TickFrame`).
//!
//! Player positions are constant within a tick — trajectories are only
//! re-planned at its very end — so they are evaluated once here. The ball's
//! state changes inside the tick (receptions, kicks, tackles), so `ball()`
//! derives its position from the current `BallState` using the cached player
//! positions. The one exception is a flight already under way at capture:
//! its position at `now_ms` is evaluated once and reused while the ball is
//! still that flight. Every flight is launched at the current tick's time
//! (`ActionResolver::kick` is the only constructor call), so a flight whose
//! `kick_ms` differs from `now_ms` can only be the one captured; any pass,
//! shot or rebound inside the tick has `kick_ms == now_ms` and bypasses the
//! cache. Anchors and phases are filled in once the ball has been stepped,
//! exactly where the engine used to compute them.

use fm_core::{GoalEnd, Vec2, Vec3};

use crate::anchor::FormationAnchor;
use crate::phase::{Phase, Possession, Side};
use crate::state::{side_index, BallState, MatchState, PLAYERS};

/// Ball offset ahead of its carrier, toward the goal they attack (m).
const CARRY_OFFSET: f32 = 0.5;

/// Where the ball sits when `carrier_pos` carries it toward `attacking`.
#[inline]
#[must_use]
pub fn carried_ball(carrier_pos: Vec2, attacking: GoalEnd) -> Vec3 {
    let dx = match attacking {
        GoalEnd::Right => CARRY_OFFSET,
        GoalEnd::Left => -CARRY_OFFSET,
    };
    Vec3::new(carrier_pos.x + dx, carrier_pos.y, 0.0)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TickFrame {
    pub tick: u32,
    pub now_ms: u32,
    positions: [Vec2; PLAYERS],
    anchors: [Vec2; PLAYERS],
    phases: [Phase; 2],
    possession: Possession,
    set_piece: bool,
    /// `(kick_ms, position at now_ms)` of the flight in progress at capture.
    flight_ball: Option<(u32, Vec3)>,
}

impl TickFrame {
    /// Evaluates the 22 player positions at the current tick. Anchors and
    /// phases are filled by the later stages (`observe_ball`, `set_phases`,
    /// `compute_anchors`).
    #[must_use]
    pub fn capture(state: &MatchState) -> Self {
        let now = state.now_ms();
        let mut positions = [Vec2::ZERO; PLAYERS];
        for (out, p) in positions.iter_mut().zip(&state.players) {
            *out = p.pos(now);
        }
        Self {
            tick: state.tick,
            now_ms: now,
            positions,
            anchors: [Vec2::ZERO; PLAYERS],
            phases: state.phases,
            possession: Possession::Loose,
            set_piece: false,
            flight_ball: match state.ball {
                BallState::Flight { flight, .. } if flight.kick_ms != now => {
                    Some((flight.kick_ms, flight.pos_at(now)))
                }
                _ => None,
            },
        }
    }

    /// Position of player `i` at this tick.
    #[inline]
    #[must_use]
    pub fn pos(&self, i: usize) -> Vec2 {
        self.positions[i]
    }

    /// Ball position now, from the *current* ball state.
    #[must_use]
    pub fn ball(&self, state: &MatchState) -> Vec3 {
        match state.ball {
            BallState::Held { holder } => {
                let h = holder as usize;
                carried_ball(self.positions[h], state.attacking(state.players[h].side))
            }
            BallState::Flight { flight, .. } => match self.flight_ball {
                Some((kick_ms, at)) if kick_ms == flight.kick_ms && kick_ms != self.now_ms => {
                    debug_assert_eq!(at, flight.pos_at(self.now_ms), "stale flight cache");
                    at
                }
                _ => flight.pos_at(self.now_ms),
            },
            BallState::Dead(r) => r.spot.extend(0.0),
        }
    }

    /// Records possession and set-piece status after the ball was stepped.
    pub fn observe_ball(&mut self, state: &MatchState) {
        self.possession = match state.ball {
            BallState::Held { holder } => Possession::Team(state.players[holder as usize].side),
            BallState::Dead(r) => Possession::Team(r.side),
            BallState::Flight { .. } => Possession::Loose,
        };
        self.set_piece = matches!(state.ball, BallState::Dead(_));
    }

    #[must_use]
    pub const fn possession(&self) -> Possession {
        self.possession
    }

    #[must_use]
    pub const fn set_piece(&self) -> bool {
        self.set_piece
    }

    pub fn set_phases(&mut self, phases: [Phase; 2]) {
        self.phases = phases;
    }

    #[must_use]
    pub const fn phase(&self, side: Side) -> Phase {
        self.phases[side_index(side)]
    }

    /// Formation anchors for all 22, for the ball as it is now.
    pub fn compute_anchors(&mut self, state: &MatchState) {
        let ball = self.ball(state).xy();
        for (i, p) in state.players.iter().enumerate() {
            if !p.active() {
                continue;
            }
            let team = state.team(p.side);
            let slot = &team.formation.slots()[p.slot as usize];
            self.anchors[i] = FormationAnchor::compute(
                slot,
                ball,
                self.phase(p.side),
                team.tactics,
                state.frame(p.side),
                &state.tuning.anchor,
            );
        }
    }

    #[inline]
    #[must_use]
    pub fn anchor(&self, i: usize) -> Vec2 {
        self.anchors[i]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ball::BallFlight;
    use crate::demo::demo_match;
    use crate::engine::MatchEngine;
    use crate::phase::LOGICAL_DT_MS;
    use crate::state::FlightIntent;

    fn state() -> MatchState {
        let (db, setup) = demo_match(3);
        MatchEngine::new(&setup, &db).state().clone()
    }

    fn fly(s: &mut MatchState, kick_ms: u32, from: Vec3, v: Vec3) -> BallFlight {
        let flight = BallFlight::kick(kick_ms, from, v);
        s.ball = BallState::Flight {
            flight,
            intent: FlightIntent::Loose,
        };
        flight
    }

    #[test]
    fn flight_in_progress_is_cached_at_capture() {
        let mut s = state();
        s.tick = 40;
        let old = fly(
            &mut s,
            39 * LOGICAL_DT_MS,
            Vec3::new(10.0, 30.0, 0.0),
            Vec3::new(12.0, 3.0, 4.0),
        );
        let frame = TickFrame::capture(&s);
        assert_eq!(
            frame.flight_ball,
            Some((old.kick_ms, old.pos_at(frame.now_ms)))
        );
        assert_eq!(frame.ball(&s), old.pos_at(frame.now_ms));
    }

    #[test]
    fn pass_in_tick_n_does_not_reuse_cache_from_tick_n_minus_1() {
        let mut s = state();
        s.tick = 40;
        let now = s.now_ms();
        // Flight kicked in tick N-1, still travelling when tick N is captured.
        let old = fly(
            &mut s,
            now - LOGICAL_DT_MS,
            Vec3::new(10.0, 30.0, 0.0),
            Vec3::new(12.0, 3.0, 4.0),
        );
        let frame = TickFrame::capture(&s);
        assert!(frame.flight_ball.is_some());
        // A pass inside tick N replaces the ball state: it must be read fresh.
        let new = fly(
            &mut s,
            now,
            Vec3::new(50.0, 20.0, 0.0),
            Vec3::new(-8.0, 6.0, 0.0),
        );
        assert_ne!(old.pos_at(now), new.pos_at(now));
        assert_eq!(frame.ball(&s), new.pos_at(now));
        // And tick N+1 caches the new flight, not the old one.
        s.tick += 1;
        let next = TickFrame::capture(&s);
        assert_eq!(next.flight_ball, Some((now, new.pos_at(next.now_ms))));
        assert_eq!(next.ball(&s), new.pos_at(next.now_ms));
    }

    #[test]
    fn flight_kicked_this_tick_is_never_cached() {
        let mut s = state();
        s.tick = 40;
        let now = s.now_ms();
        fly(
            &mut s,
            now,
            Vec3::new(10.0, 30.0, 0.0),
            Vec3::new(12.0, 3.0, 4.0),
        );
        assert_eq!(TickFrame::capture(&s).flight_ball, None);
    }
}
