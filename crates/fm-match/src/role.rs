//! `RoleBehavior` (spec 3.E, step 2). Phase 3 ships stubs: every role holds
//! its formation anchor. Phase 5 replaces each arm with a real state machine
//! (pressing, overlaps, marking...), keeping this signature.

use fm_core::Vec2;

use crate::formation::Role;
use crate::phase::Phase;

/// What a player intends to do with its body this tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RoleIntent {
    /// Move to (or stay at) this point at the given fraction of top speed.
    MoveTo { target: Vec2, urgency: f32 },
}

/// Inputs a role needs; grows in Phase 5.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoleContext {
    pub anchor: Vec2,
    pub ball: Vec2,
    pub phase: Phase,
}

pub struct RoleBehavior;

impl RoleBehavior {
    #[must_use]
    pub fn update(role: Role, ctx: &RoleContext) -> RoleIntent {
        // Urgency: chase shape harder in transitions, jog otherwise.
        let urgency = match ctx.phase {
            Phase::TransitionAttack | Phase::TransitionDefense => 0.9,
            Phase::InPossession | Phase::OutOfPossession | Phase::SetPiece => 0.6,
        };
        match role {
            Role::Goalkeeper
            | Role::CentreBack
            | Role::FullBack
            | Role::WingBack
            | Role::DefensiveMidfielder
            | Role::CentralMidfielder
            | Role::WideMidfielder
            | Role::AttackingMidfielder
            | Role::Winger
            | Role::Striker => RoleIntent::MoveTo {
                target: ctx.anchor,
                urgency,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RoleBehavior, RoleContext, RoleIntent};
    use crate::formation::Formation;
    use crate::phase::Phase;
    use fm_core::Vec2;

    #[test]
    fn stubs_hold_the_anchor() {
        let ctx = RoleContext {
            anchor: Vec2::new(30.0, 20.0),
            ball: Vec2::new(50.0, 34.0),
            phase: Phase::TransitionDefense,
        };
        for f in Formation::ALL {
            for s in f.slots() {
                let RoleIntent::MoveTo { target, urgency } = RoleBehavior::update(s.role, &ctx);
                assert_eq!(target, ctx.anchor);
                assert!((urgency - 0.9).abs() < 1e-6);
            }
        }
    }
}
