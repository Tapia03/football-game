//! Pitch dimensions (FIFA standard, metres) and spatial helpers.
//!
//! Coordinate system: origin at the bottom-left corner, `x` along the length
//! (0 → 105), `y` along the width (0 → 68). The left goal sits at `x = 0`.

use crate::geometry::Vec2;

pub const LENGTH: f32 = 105.0;
pub const WIDTH: f32 = 68.0;
pub const HALF_LENGTH: f32 = LENGTH / 2.0;
pub const HALF_WIDTH: f32 = WIDTH / 2.0;
pub const CENTRE: Vec2 = Vec2::new(HALF_LENGTH, HALF_WIDTH);

pub const GOAL_WIDTH: f32 = 7.32;
pub const GOAL_HEIGHT: f32 = 2.44;
pub const PENALTY_AREA_DEPTH: f32 = 16.5;
pub const PENALTY_AREA_WIDTH: f32 = 40.32;
pub const GOAL_AREA_DEPTH: f32 = 5.5;
pub const GOAL_AREA_WIDTH: f32 = 18.32;
pub const PENALTY_SPOT_DISTANCE: f32 = 11.0;
pub const CENTRE_CIRCLE_RADIUS: f32 = 9.15;

/// Which goal line a team attacks or defends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GoalEnd {
    /// Goal at `x = 0`.
    Left,
    /// Goal at `x = LENGTH`.
    Right,
}

impl GoalEnd {
    #[inline]
    #[must_use]
    pub const fn opposite(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }

    /// x coordinate of this goal line.
    #[inline]
    #[must_use]
    pub const fn goal_line_x(self) -> f32 {
        match self {
            Self::Left => 0.0,
            Self::Right => LENGTH,
        }
    }

    /// Centre of the goal mouth.
    #[inline]
    #[must_use]
    pub const fn goal_centre(self) -> Vec2 {
        Vec2::new(self.goal_line_x(), HALF_WIDTH)
    }

    /// +1 when moving toward this goal increases `x`, -1 otherwise.
    #[inline]
    #[must_use]
    pub const fn direction(self) -> f32 {
        match self {
            Self::Left => -1.0,
            Self::Right => 1.0,
        }
    }

    /// Penalty spot in front of this goal.
    #[inline]
    #[must_use]
    pub fn penalty_spot(self) -> Vec2 {
        Vec2::new(
            self.goal_line_x() - self.direction() * PENALTY_SPOT_DISTANCE,
            HALF_WIDTH,
        )
    }
}

/// Inside the field of play, lines included (lines belong to the pitch).
#[inline]
#[must_use]
pub fn in_pitch(p: Vec2) -> bool {
    (0.0..=LENGTH).contains(&p.x) && (0.0..=WIDTH).contains(&p.y)
}

/// Inside the penalty area in front of `end`, lines included.
#[inline]
#[must_use]
pub fn in_penalty_area(p: Vec2, end: GoalEnd) -> bool {
    let depth = match end {
        GoalEnd::Left => p.x,
        GoalEnd::Right => LENGTH - p.x,
    };
    let half = PENALTY_AREA_WIDTH / 2.0;
    (0.0..=PENALTY_AREA_DEPTH).contains(&depth)
        && (HALF_WIDTH - half..=HALF_WIDTH + half).contains(&p.y)
}

/// Straight-line distance to the centre of the goal at `end`.
#[inline]
#[must_use]
pub fn distance_to_goal(p: Vec2, end: GoalEnd) -> f32 {
    p.distance(end.goal_centre())
}

/// True when `p` is past the defensive line at `line_x`, seen by a team
/// attacking `attacking`. Exactly level is NOT behind (level is onside).
#[inline]
#[must_use]
pub fn is_behind_defense_line(p: Vec2, line_x: f32, attacking: GoalEnd) -> bool {
    (p.x - line_x) * attacking.direction() > 0.0
}

#[cfg(test)]
mod tests {
    use super::{
        distance_to_goal, in_penalty_area, in_pitch, is_behind_defense_line, GoalEnd, CENTRE,
        HALF_LENGTH, LENGTH, WIDTH,
    };
    use crate::geometry::Vec2;

    #[test]
    fn pitch_bounds_include_lines() {
        assert!(in_pitch(Vec2::ZERO));
        assert!(in_pitch(Vec2::new(LENGTH, WIDTH)));
        assert!(in_pitch(CENTRE));
        assert!(!in_pitch(Vec2::new(-0.01, 10.0)));
        assert!(!in_pitch(Vec2::new(50.0, 68.01)));
        assert!(!in_pitch(Vec2::new(105.01, 10.0)));
    }

    #[test]
    fn penalty_areas_are_mirrored() {
        assert!(in_penalty_area(Vec2::new(10.0, 34.0), GoalEnd::Left));
        assert!(!in_penalty_area(Vec2::new(10.0, 34.0), GoalEnd::Right));
        assert!(in_penalty_area(Vec2::new(95.0, 34.0), GoalEnd::Right));
        // Edges: 16.5 m deep, 40.32 m wide centred on y = 34.
        assert!(in_penalty_area(Vec2::new(16.5, 13.84), GoalEnd::Left));
        assert!(!in_penalty_area(Vec2::new(16.6, 34.0), GoalEnd::Left));
        assert!(!in_penalty_area(Vec2::new(5.0, 13.0), GoalEnd::Left));
        assert!(!in_penalty_area(Vec2::new(5.0, 55.0), GoalEnd::Left));
        assert!(!in_penalty_area(Vec2::new(-1.0, 34.0), GoalEnd::Left));
    }

    #[test]
    fn distance_to_goal_and_spots() {
        assert!((distance_to_goal(CENTRE, GoalEnd::Right) - HALF_LENGTH).abs() < 1e-4);
        assert!((distance_to_goal(CENTRE, GoalEnd::Left) - HALF_LENGTH).abs() < 1e-4);
        let spot = GoalEnd::Right.penalty_spot();
        assert!((distance_to_goal(spot, GoalEnd::Right) - 11.0).abs() < 1e-4);
        assert_eq!(GoalEnd::Left.penalty_spot(), Vec2::new(11.0, 34.0));
        assert_eq!(GoalEnd::Left.opposite(), GoalEnd::Right);
    }

    #[test]
    fn behind_defense_line_respects_direction() {
        let p = Vec2::new(80.0, 30.0);
        assert!(is_behind_defense_line(p, 75.0, GoalEnd::Right));
        assert!(!is_behind_defense_line(p, 85.0, GoalEnd::Right));
        assert!(
            !is_behind_defense_line(p, 80.0, GoalEnd::Right),
            "level is onside"
        );
        assert!(is_behind_defense_line(
            Vec2::new(20.0, 30.0),
            25.0,
            GoalEnd::Left
        ));
        assert!(!is_behind_defense_line(
            Vec2::new(30.0, 30.0),
            25.0,
            GoalEnd::Left
        ));
    }
}
