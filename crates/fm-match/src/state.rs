//! Mutable match state shared by the decision system and the resolver.
//! Holds no LOD and no `dt`: time only advances through `tick_logic`.

use fm_core::{GoalEnd, Vec2, Vec3};
use fm_entities::{PlayerAttributes, PlayerId};

use crate::anchor::AnchorTuning;
use crate::ball::BallFlight;
use crate::events::{EventLog, RestartKind};
use crate::formation::{Formation, Role};
use crate::frame::TeamFrame;
use crate::kinematics::{PlayerKinematics, Trajectory};
use crate::phase::{Phase, PhaseStateMachine, Side};
use crate::tactics::Tactics;

pub const PLAYERS: usize = 22;

/// One of the 22 players, with attributes copied in at kick-off so the
/// engine never touches the `PlayerDatabase` during play.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatchPlayer {
    pub id: PlayerId,
    pub side: Side,
    /// Index into the team's formation slots.
    pub slot: u8,
    pub role: Role,
    pub attrs: PlayerAttributes,
    pub top_speed: f32,
    pub traj: Trajectory,
    /// Number of resolved actions so far: the RNG stream index (spec 3.B).
    pub action_count: u32,
    pub yellow_cards: u8,
    pub sent_off: bool,
    /// Tick before which the player cannot tackle again.
    pub tackle_ready_tick: u32,
    /// Tick before which the player cannot touch a loose ball (just kicked it).
    pub touch_ready_tick: u32,
}

impl MatchPlayer {
    #[inline]
    #[must_use]
    pub fn pos(&self, t_ms: u32) -> Vec2 {
        PlayerKinematics::pos_at(&self.traj, t_ms)
    }

    #[inline]
    #[must_use]
    pub fn active(&self) -> bool {
        !self.sent_off
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShotOutcome {
    Goal,
    Saved,
    OffTarget,
}

/// Why the ball is in the air / rolling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlightIntent {
    Pass {
        receiver: u8,
    },
    Shot {
        shooter: u8,
        outcome: ShotOutcome,
        arrive_ms: u32,
    },
    /// Deflection, lost control, tackle that pops the ball loose.
    Loose,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Restart {
    pub kind: RestartKind,
    pub side: Side,
    pub spot: Vec2,
    pub taker: u8,
    pub ready_tick: u32,
}

// `Flight` is large (the whole solved path, ~300 B) but boxing it would
// allocate inside `tick_logic` (criterion 17), so the size gap is accepted.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BallState {
    Held {
        holder: u8,
    },
    Flight {
        flight: BallFlight,
        intent: FlightIntent,
    },
    Dead(Restart),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TeamState {
    pub formation: Formation,
    pub tactics: Tactics,
    pub score: u8,
    pub shots: u16,
    pub shots_on_target: u16,
    pub passes: u16,
    pub passes_completed: u16,
    pub tackles: u16,
    /// Tick before which no player of this team may challenge.
    pub next_tackle_tick: u32,
}

/// Everything `tick_logic` reads and writes.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchState {
    pub match_seed: u64,
    pub tick: u32,
    pub players: [MatchPlayer; PLAYERS],
    pub teams: [TeamState; 2],
    pub ball: BallState,
    pub last_touch: Side,
    pub holder_ticks: u32,
    pub phase_sm: PhaseStateMachine,
    /// Phases computed at the current tick, `[home, away]`.
    pub phases: [Phase; 2],
    pub second_half: bool,
    pub finished: bool,
    pub tuning: AnchorTuning,
    pub events: EventLog,
}

impl MatchState {
    #[inline]
    #[must_use]
    pub const fn now_ms(&self) -> u32 {
        self.tick * crate::phase::LOGICAL_DT_MS
    }

    /// Goal the side attacks right now (sides swap at half time).
    #[must_use]
    pub const fn attacking(&self, side: Side) -> GoalEnd {
        match (side, self.second_half) {
            (Side::Home, false) | (Side::Away, true) => GoalEnd::Right,
            (Side::Away, false) | (Side::Home, true) => GoalEnd::Left,
        }
    }

    #[must_use]
    pub const fn frame(&self, side: Side) -> TeamFrame {
        TeamFrame::new(self.attacking(side))
    }

    #[must_use]
    pub const fn team(&self, side: Side) -> &TeamState {
        &self.teams[side_index(side)]
    }

    pub fn team_mut(&mut self, side: Side) -> &mut TeamState {
        &mut self.teams[side_index(side)]
    }

    /// Ball position at the current tick.
    #[must_use]
    pub fn ball_pos(&self) -> Vec3 {
        self.ball_pos_at(self.now_ms())
    }

    /// Ball position at `t_ms` (valid between this tick and the next).
    #[must_use]
    pub fn ball_pos_at(&self, t_ms: u32) -> Vec3 {
        match self.ball {
            BallState::Held { holder } => {
                let p = &self.players[holder as usize];
                // Ball sits half a metre ahead of the carrier, toward goal.
                let ahead = self.attacking(p.side);
                let dx = match ahead {
                    GoalEnd::Right => 0.5,
                    GoalEnd::Left => -0.5,
                };
                let pos = p.pos(t_ms);
                Vec3::new(pos.x + dx, pos.y, 0.0)
            }
            BallState::Flight { flight, .. } => flight.pos_at(t_ms),
            BallState::Dead(r) => r.spot.extend(0.0),
        }
    }

    /// The team's goalkeeper index, if still on the pitch.
    #[must_use]
    pub fn keeper(&self, side: Side) -> Option<usize> {
        self.players
            .iter()
            .position(|p| p.side == side && p.role == Role::Goalkeeper && p.active())
    }

    pub fn next_rng(&mut self, idx: usize) -> fm_core::Rng {
        let p = &mut self.players[idx];
        let rng = fm_core::rng_for_event(self.match_seed, p.id.0, p.action_count);
        p.action_count += 1;
        rng
    }
}

#[inline]
#[must_use]
pub const fn side_index(side: Side) -> usize {
    match side {
        Side::Home => 0,
        Side::Away => 1,
    }
}
