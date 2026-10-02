//! Shot value (spec Fase 5 (c1), item 1): expected goals from the shooter's
//! geometry, the finisher's skill, and how much of the goal mouth the bodies
//! in front cover. Pure functions of the tick state — no RNG, no sampling.

use fm_core::{math, pitch, GoalEnd, Vec2};
use fm_entities::PlayerAttributes;

use crate::formation::Role;
use crate::state::{MatchState, PLAYERS};
use crate::tick_frame::TickFrame;
use crate::tuning::XgTuning;

/// Signed angle from `a` to `b` (rad, in `(-π, π]`).
#[inline]
fn angle_between(a: Vec2, b: Vec2) -> f32 {
    math::atan2(a.x * b.y - a.y * b.x, a.dot(b))
}

/// The two posts of the goal at `end`.
#[must_use]
pub fn goal_posts(end: GoalEnd) -> (Vec2, Vec2) {
    let c = end.goal_centre();
    let half = pitch::GOAL_WIDTH / 2.0;
    (Vec2::new(c.x, c.y - half), Vec2::new(c.x, c.y + half))
}

/// Angle the goal mouth subtends at `pos` (rad): wide close in and central,
/// zero on the goal line beside the posts.
#[must_use]
pub fn goal_angle(pos: Vec2, end: GoalEnd) -> f32 {
    let (a, b) = goal_posts(end);
    angle_between(a - pos, b - pos).abs()
}

/// Geometric xG of an unblocked shot from `pos` by an average finisher.
#[must_use]
pub fn xg(pos: Vec2, end: GoalEnd, t: &XgTuning) -> f32 {
    let logit =
        t.intercept + t.w_angle * goal_angle(pos, end) + t.w_dist * pos.distance(end.goal_centre());
    1.0 / (1.0 + math::exp(-logit))
}

/// Shooting skill in `[0, 1]`: finishing (or long shots beyond
/// `long_shot_dist`), composure and technique.
#[must_use]
pub fn shot_skill(a: &PlayerAttributes, dist: f32, long_shot_dist: f32) -> f32 {
    let strike = if dist > long_shot_dist {
        a.technical.long_shots
    } else {
        a.technical.finishing
    };
    0.6 * f32::from(strike) / 100.0
        + 0.2 * f32::from(a.mental.composure) / 100.0
        + 0.2 * f32::from(a.technical.technique) / 100.0
}

/// Finisher factor (1.0 for an average finisher). `skill` in `[0, 1]`.
#[inline]
#[must_use]
pub fn finisher(skill: f32, t: &XgTuning) -> f32 {
    t.skill_base + t.skill_k * skill
}

/// Fraction of the goal mouth (by angle, seen from the shooter) covered by
/// opposing outfield bodies between the shooter and the goal. The keeper is
/// part of the geometric xG, not of the coverage.
#[must_use]
pub fn coverage(state: &MatchState, frame: &TickFrame, shooter: usize, end: GoalEnd) -> f32 {
    let block_radius = state.tuning.xg.block_radius;
    let pos = frame.pos(shooter);
    let to_goal = end.goal_centre() - pos;
    let goal_dist = to_goal.length();
    let (pa, pb) = goal_posts(end);
    let (a0, a1) = {
        let (x, y) = (
            angle_between(to_goal, pa - pos),
            angle_between(to_goal, pb - pos),
        );
        if x <= y {
            (x, y)
        } else {
            (y, x)
        }
    };
    let span = a1 - a0;
    if span <= 1e-4 {
        return 1.0;
    }
    let side = state.players[shooter].side;
    // Blocked intervals, clipped to the goal mouth; ≤ 10 outfielders.
    let mut lo = [0.0_f32; PLAYERS];
    let mut hi = [0.0_f32; PLAYERS];
    let mut count = 0;
    for (j, o) in state.players.iter().enumerate() {
        if o.side == side || !o.active() || o.role == Role::Goalkeeper {
            continue;
        }
        let rel = frame.pos(j) - pos;
        let d = rel.length();
        // Only bodies in front of the shooter and short of the goal block.
        if rel.dot(to_goal) <= 0.0 || d >= goal_dist {
            continue;
        }
        let centre = angle_between(to_goal, rel);
        let half = math::atan2(block_radius, d);
        let (l, h) = ((centre - half).max(a0), (centre + half).min(a1));
        if l < h {
            lo[count] = l;
            hi[count] = h;
            count += 1;
        }
    }
    // Union of the intervals: insertion sort by start (deterministic), merge.
    for i in 1..count {
        let mut k = i;
        while k > 0 && lo[k - 1] > lo[k] {
            lo.swap(k - 1, k);
            hi.swap(k - 1, k);
            k -= 1;
        }
    }
    let mut covered = 0.0;
    let mut i = 0;
    while i < count {
        let (start, mut end_) = (lo[i], hi[i]);
        i += 1;
        while i < count && lo[i] <= end_ {
            end_ = end_.max(hi[i]);
            i += 1;
        }
        covered += end_ - start;
    }
    (covered / span).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> XgTuning {
        XgTuning::default()
    }

    #[test]
    fn xg_matches_its_central_anchors() {
        let end = GoalEnd::Right;
        let c = end.goal_centre();
        for (dist, want) in [(6.0, 0.40), (11.0, 0.17), (20.0, 0.05), (25.0, 0.03)] {
            let got = xg(Vec2::new(c.x - dist, c.y), end, &t());
            assert!((got - want).abs() < 0.01, "{dist} m: {got} vs {want}");
        }
    }

    #[test]
    fn xg_falls_with_distance_and_angle() {
        let end = GoalEnd::Left;
        let c = end.goal_centre();
        let central = xg(Vec2::new(c.x + 12.0, c.y), end, &t());
        let wide = xg(Vec2::new(c.x + 12.0, c.y + 15.0), end, &t());
        let far = xg(Vec2::new(c.x + 24.0, c.y), end, &t());
        assert!(central > wide && central > far, "{central} {wide} {far}");
        // On the goal line beside the post the mouth closes completely.
        let byline = goal_angle(Vec2::new(c.x, c.y + 10.0), end);
        assert!(byline < 1e-3, "{byline}");
    }

    #[test]
    fn finisher_is_neutral_at_average_skill() {
        assert!((finisher(0.5, &t()) - 1.0).abs() < 1e-6);
        assert!(finisher(0.9, &t()) > finisher(0.3, &t()));
    }

    /// Home striker (10) shooting; one away outfielder (13) placed.
    fn state_with(shooter: Vec2, defender: Vec2) -> (MatchState, usize, usize) {
        let s = crate::test_support::placed_state(&[(10, shooter), (13, defender)], 10);
        (s, 10, 13)
    }

    #[test]
    fn body_in_front_covers_the_goal_and_wide_one_does_not() {
        let end = GoalEnd::Right;
        let c = end.goal_centre();
        let shooter = Vec2::new(c.x - 16.0, c.y);
        let (s, me, _) = state_with(shooter, Vec2::new(c.x - 14.0, c.y));
        assert_eq!(s.attacking(s.players[me].side), end, "home attacks right");
        let f = TickFrame::capture(&s);
        let close = coverage(&s, &f, me, end);
        assert!(close > 0.99, "2 m in front covers the mouth: {close}");

        let (s, me, _) = state_with(shooter, Vec2::new(c.x - 4.0, c.y + 1.0));
        let f = TickFrame::capture(&s);
        let far = coverage(&s, &f, me, end);
        assert!(
            far > 0.05 && far < 0.5,
            "4 m off the line covers part: {far}"
        );

        let (s, me, _) = state_with(shooter, Vec2::new(c.x - 10.0, c.y + 12.0));
        let f = TickFrame::capture(&s);
        assert_eq!(coverage(&s, &f, me, end), 0.0, "wide body covers nothing");

        let (s, me, _) = state_with(shooter, Vec2::new(c.x - 20.0, c.y));
        let f = TickFrame::capture(&s);
        assert_eq!(coverage(&s, &f, me, end), 0.0, "body behind the shooter");
    }
}
