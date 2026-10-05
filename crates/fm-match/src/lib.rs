//! Match engine: one logical tick (10 Hz) for every LOD, analytic player and
//! ball motion, event-indexed RNG (spec 3.A, 3.B).

#![forbid(unsafe_code)]

pub mod anchor;
pub mod ball;
pub mod decision;
pub mod demo;
pub mod engine;
pub mod events;
pub mod formation;
pub mod frame;
pub mod kinematics;
pub mod parity;
pub mod phase;
pub mod resolver;
pub mod role;
pub mod runs;
pub mod snapshot;
pub mod state;
pub mod tactics;
#[cfg(test)]
mod test_support;
pub mod tick_frame;
pub mod tuning;
pub mod value;
pub mod xg;

pub use anchor::{AnchorTuning, FormationAnchor, PhaseShape};
pub use ball::BallFlight;
pub use decision::{Action, DecisionSystem};
pub use engine::{MatchEngine, MatchSetup, TeamSheet, FULL_TICKS, HALF_TICKS};
pub use events::{CardKind, EventKind, MatchEvent, RestartKind};
pub use formation::{Formation, Line, Role, Slot};
pub use frame::{Rel, TeamFrame};
pub use kinematics::{PlayerKinematics, Trajectory};
pub use phase::{Phase, PhaseStateMachine, Possession, Side, LOGICAL_DT_MS, TRANSITION_TICKS};
pub use resolver::ActionResolver;
pub use role::{RoleBehavior, RoleContext, RoleIntent};
pub use snapshot::{LodLevel, MatchSnapshot, PlayerSnapshot};
pub use state::MatchState;
pub use tactics::{LineHeight, Mentality, Pressing, Tactics, Width};
pub use tick_frame::TickFrame;
pub use tuning::TuningParams;
