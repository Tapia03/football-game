//! Team-relative coordinates.
//!
//! Formations and tactics are authored once, from the point of view of a team
//! attacking to its "front". `TeamFrame` maps that view onto the pitch for
//! either attacking direction, so both teams share the same tables.
//!
//! Relative space: `depth` 0 = own goal line, 1 = opponent goal line;
//! `lateral` -1 = team's right touchline, +1 = team's left touchline.

use fm_core::pitch::{HALF_WIDTH, LENGTH};
use fm_core::{GoalEnd, Vec2};

/// A point in team-relative space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rel {
    pub depth: f32,
    pub lateral: f32,
}

impl Rel {
    #[inline]
    #[must_use]
    pub const fn new(depth: f32, lateral: f32) -> Self {
        Self { depth, lateral }
    }
}

/// Converts between team-relative and pitch coordinates for one team.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeamFrame {
    /// Goal this team is attacking.
    pub attacking: GoalEnd,
}

impl TeamFrame {
    #[inline]
    #[must_use]
    pub const fn new(attacking: GoalEnd) -> Self {
        Self { attacking }
    }

    #[inline]
    #[must_use]
    pub fn to_pitch(self, r: Rel) -> Vec2 {
        match self.attacking {
            // Facing +x: the team's left is +y.
            GoalEnd::Right => Vec2::new(r.depth * LENGTH, HALF_WIDTH + r.lateral * HALF_WIDTH),
            // Facing -x: everything is mirrored through the centre spot.
            GoalEnd::Left => Vec2::new(
                LENGTH - r.depth * LENGTH,
                HALF_WIDTH - r.lateral * HALF_WIDTH,
            ),
        }
    }

    #[inline]
    #[must_use]
    pub fn to_rel(self, p: Vec2) -> Rel {
        match self.attacking {
            GoalEnd::Right => Rel::new(p.x / LENGTH, (p.y - HALF_WIDTH) / HALF_WIDTH),
            GoalEnd::Left => Rel::new((LENGTH - p.x) / LENGTH, (HALF_WIDTH - p.y) / HALF_WIDTH),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Rel, TeamFrame};
    use fm_core::pitch::CENTRE;
    use fm_core::{GoalEnd, Vec2};

    fn close(a: Vec2, b: Vec2) -> bool {
        a.distance(b) < 1e-3
    }

    #[test]
    fn round_trip_both_directions() {
        for end in [GoalEnd::Left, GoalEnd::Right] {
            let f = TeamFrame::new(end);
            for p in [
                Vec2::new(0.0, 0.0),
                Vec2::new(30.0, 50.0),
                Vec2::new(105.0, 68.0),
            ] {
                assert!(close(f.to_pitch(f.to_rel(p)), p));
            }
        }
    }

    #[test]
    fn own_goal_and_left_side_map_correctly() {
        let right = TeamFrame::new(GoalEnd::Right);
        let left = TeamFrame::new(GoalEnd::Left);
        assert!(close(
            right.to_pitch(Rel::new(0.0, 0.0)),
            Vec2::new(0.0, 34.0)
        ));
        assert!(close(
            left.to_pitch(Rel::new(0.0, 0.0)),
            Vec2::new(105.0, 34.0)
        ));
        // Team's left touchline: +y when attacking right, -y when attacking left.
        assert!(close(
            right.to_pitch(Rel::new(0.5, 1.0)),
            Vec2::new(52.5, 68.0)
        ));
        assert!(close(
            left.to_pitch(Rel::new(0.5, 1.0)),
            Vec2::new(52.5, 0.0)
        ));
        assert!(close(right.to_pitch(Rel::new(0.5, 0.0)), CENTRE));
    }
}
