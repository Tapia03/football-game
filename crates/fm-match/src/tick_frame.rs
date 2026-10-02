//! `TickFrame`: everything about the current logical tick that is computed
//! once and then only read (spec Fase 5, `TickFrame`).
//!
//! Player positions are constant within a tick — trajectories are only
//! re-planned at its very end — so they are evaluated once here. The ball is
//! *not* cached: its state changes inside the tick (receptions, kicks,
//! tackles), so `ball()` derives its position from the current `BallState`
//! using the cached player positions. Anchors and phases are filled in once
//! the ball has been stepped, exactly where the engine used to compute them.

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
            BallState::Flight { flight, .. } => flight.pos_at(self.now_ms),
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
