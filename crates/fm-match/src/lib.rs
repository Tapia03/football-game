//! Match engine. Phase 3: team-relative frame, formations, anchors, phases,
//! analytic player kinematics and role stubs. The logical tick, action
//! resolver and snapshots arrive in Phase 4.

#![forbid(unsafe_code)]

pub mod anchor;
pub mod formation;
pub mod frame;
pub mod kinematics;
pub mod phase;
pub mod role;
pub mod tactics;

pub use anchor::{AnchorTuning, FormationAnchor, PhaseShape};
pub use formation::{Formation, Line, Role, Slot};
pub use frame::{Rel, TeamFrame};
pub use kinematics::{PlayerKinematics, Trajectory};
pub use phase::{Phase, PhaseStateMachine, Possession, Side, LOGICAL_DT_MS, TRANSITION_TICKS};
pub use role::{RoleBehavior, RoleContext, RoleIntent};
pub use tactics::{LineHeight, Mentality, Tactics, Width};
