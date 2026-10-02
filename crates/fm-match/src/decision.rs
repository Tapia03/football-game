//! `DecisionSystem::choose_action` (spec 3.E, step 3) — Phase 4 minimum.
//!
//! Deterministic scoring, no RNG: randomness belongs to `ActionResolver`.
//! Takes only the match state and the actor — no `dt`, no LOD (criterion 18).
//! Phase 5 replaces the scoring with full role behaviours.

use fm_core::{pitch, Vec2};

use crate::formation::Role;
use crate::state::{MatchState, PLAYERS};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    Pass {
        to: u8,
    },
    Shoot,
    /// Carry the ball toward `target`.
    Dribble {
        target: Vec2,
    },
    /// Keep the ball where it is (shield / wait for support).
    Hold,
    Tackle {
        on: u8,
    },
}

/// Distance at which an opponent counts as pressing the carrier.
pub const PRESSURE_RADIUS: f32 = 2.5;
/// Distance from which a tackle can be attempted.
pub const TACKLE_RANGE: f32 = 1.8;
/// A carrier needs this many ticks on the ball before releasing it, unless
/// pressed (first touch + look up ≈ 0.4 s).
pub const MIN_HOLD_TICKS: u32 = 4;
/// Keepers take longer to distribute (≈ 1.5 s).
pub const KEEPER_HOLD_TICKS: u32 = 15;
/// Maximum shooting distance considered (m).
pub const SHOOT_RANGE: f32 = 25.0;
/// Minimum shot score to pull the trigger (≈ inside 15 m for an average
/// finisher, central).
pub const SHOOT_THRESHOLD: f32 = 0.60;

pub struct DecisionSystem;

/// Shortest distance from `p` to the segment `a`→`b`.
fn dist_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_squared();
    if len2 < 1e-6 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

impl DecisionSystem {
    /// What the carrier `me` does this tick. Call only for the ball holder.
    #[must_use]
    pub fn choose_action(state: &MatchState, me: usize) -> Action {
        let now = state.now_ms();
        let p = &state.players[me];
        let my_pos = p.pos(now);
        let goal = state.attacking(p.side).goal_centre();
        let forward = state.attacking(p.side).direction();

        let nearest_opp = state
            .players
            .iter()
            .filter(|o| o.side != p.side && o.active())
            .map(|o| o.pos(now).distance(my_pos))
            .fold(f32::MAX, f32::min);
        let pressed = nearest_opp < PRESSURE_RADIUS;
        let min_hold = if p.role == Role::Goalkeeper {
            KEEPER_HOLD_TICKS
        } else {
            MIN_HOLD_TICKS
        };
        if state.holder_ticks < min_hold && !pressed {
            return Action::Hold;
        }

        let dist_goal = my_pos.distance(goal);
        let shoot_score = Self::shoot_score(state, me, my_pos, goal);
        let (best_pass, best_to) = Self::best_pass(state, me, my_pos, forward);
        let dribble_score = Self::dribble_score(state, me, my_pos, forward, dist_goal);

        if shoot_score > SHOOT_THRESHOLD && shoot_score >= best_pass && shoot_score >= dribble_score
        {
            return Action::Shoot;
        }
        if let Some(to) = best_to {
            if best_pass >= dribble_score {
                return Action::Pass { to };
            }
        }
        if dribble_score > f32::MIN {
            let target = Vec2::new(
                (my_pos.x + forward * 5.0).clamp(1.0, pitch::LENGTH - 1.0),
                // Drift slightly toward the goal's centre line.
                my_pos.y + (goal.y - my_pos.y).clamp(-1.0, 1.0),
            );
            return Action::Dribble { target };
        }
        Action::Hold
    }

    /// Shot value: worth more the closer and more central (rationale: xG
    /// falls steeply with distance and with angle off-centre; most shots come
    /// from inside ~18 m).
    fn shoot_score(state: &MatchState, me: usize, my_pos: Vec2, goal: Vec2) -> f32 {
        let now = state.now_ms();
        let p = &state.players[me];
        let dist_goal = my_pos.distance(goal);
        if dist_goal >= SHOOT_RANGE || p.role == Role::Goalkeeper {
            return 0.0;
        }
        let skill = if dist_goal > 20.0 {
            f32::from(p.attrs.technical.long_shots)
        } else {
            f32::from(p.attrs.technical.finishing)
        } / 100.0;
        let central = (1.0 - (my_pos.y - pitch::HALF_WIDTH).abs() / 30.0).clamp(0.2, 1.0);
        // Bodies in the shooting lane (outfield only) block most shots.
        let lane = state
            .players
            .iter()
            .filter(|o| o.side != p.side && o.active() && o.role != Role::Goalkeeper)
            .map(|o| dist_to_segment(o.pos(now), my_pos, goal))
            .fold(f32::MAX, f32::min);
        let blocked = if lane < 1.0 { 0.3 } else { 1.0 };
        (0.25 + 0.75 * skill) * (1.0 - dist_goal / SHOOT_RANGE) * central * blocked * 2.2
    }

    /// Best pass target and its score: progress toward goal, open lane, not
    /// too long.
    fn best_pass(state: &MatchState, me: usize, my_pos: Vec2, forward: f32) -> (f32, Option<u8>) {
        let now = state.now_ms();
        let p = &state.players[me];
        let mut best_pass = f32::MIN;
        let mut best_to = None;
        for (i, t) in state.players.iter().enumerate() {
            if i == me || t.side != p.side || !t.active() {
                continue;
            }
            let tp = t.pos(now);
            let d = my_pos.distance(tp);
            if !(4.0..=45.0).contains(&d) {
                continue;
            }
            let progress = (tp.x - my_pos.x) * forward;
            let openness = state
                .players
                .iter()
                .filter(|o| o.side != p.side && o.active())
                .map(|o| dist_to_segment(o.pos(now), my_pos, tp))
                .fold(6.0_f32, f32::min);
            let mut score = 0.4 + progress / 40.0 + openness / 10.0 - d / 80.0;
            // Lanes narrower than ~2.5 m tend to get cut out.
            if openness < 2.5 {
                score -= (2.5 - openness) * 0.25;
            }
            if t.role == Role::Goalkeeper {
                score -= 0.3;
            }
            // Strict `>` keeps the lowest index on ties: deterministic.
            if score > best_pass {
                best_pass = score;
                best_to = u8::try_from(i).ok();
            }
        }
        (best_pass, best_to)
    }

    /// Dribble value: needs space in front; decays with time on the ball.
    fn dribble_score(
        state: &MatchState,
        me: usize,
        my_pos: Vec2,
        forward: f32,
        dist_goal: f32,
    ) -> f32 {
        let now = state.now_ms();
        let p = &state.players[me];
        if p.role == Role::Goalkeeper {
            return f32::MIN;
        }
        let ahead = Vec2::new(my_pos.x + forward * 6.0, my_pos.y);
        let space = state
            .players
            .iter()
            .filter(|o| o.side != p.side && o.active())
            .map(|o| o.pos(now).distance(ahead))
            .fold(f32::MAX, f32::min);
        // Long carries are rare: the value of dribbling decays after ~2.5 s
        // on the ball, pushing the carrier to release it.
        #[allow(clippy::cast_precision_loss)] // ticks on the ball ≪ 2^23
        let carry_decay = (1.0 - (state.holder_ticks.saturating_sub(25) as f32) / 30.0).max(0.0);
        // Near goal, running into a packed box rarely beats a pass.
        let near_goal = if dist_goal < 22.0 { 0.5 } else { 1.0 };
        if space > 5.0 {
            (0.55 + space.min(20.0) / 40.0) * carry_decay * near_goal
        } else {
            0.15 * carry_decay * near_goal
        }
    }

    /// The defender of `side` closest to the ball, who presses it this tick.
    #[must_use]
    pub fn presser(state: &MatchState, side: crate::phase::Side) -> Option<usize> {
        let ball = state.ball_pos().xy();
        let now = state.now_ms();
        let mut best = None;
        let mut best_d = f32::MAX;
        for i in 0..PLAYERS {
            let p = &state.players[i];
            if p.side != side || !p.active() || p.role == Role::Goalkeeper {
                continue;
            }
            let d = p.pos(now).distance(ball);
            if d < best_d {
                best_d = d;
                best = Some(i);
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::dist_to_segment;
    use fm_core::Vec2;

    #[test]
    fn segment_distance() {
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(10.0, 0.0);
        assert!((dist_to_segment(Vec2::new(5.0, 3.0), a, b) - 3.0).abs() < 1e-5);
        assert!((dist_to_segment(Vec2::new(-4.0, 3.0), a, b) - 5.0).abs() < 1e-5);
        assert!(
            (dist_to_segment(Vec2::new(1.0, 1.0), a, a) - fm_core::math::sqrt(2.0)).abs() < 1e-5
        );
    }
}

#[cfg(test)]
mod signature_audit {
    //! Criterion 18: `choose_action` and `resolve` must not receive `dt` or
    //! `LodLevel`. Audited on the source text so the rule survives refactors.

    fn signature<'a>(src: &'a str, name: &str) -> &'a str {
        let start = src
            .find(&format!("pub fn {name}("))
            .expect("function exists");
        let end = start + src[start..].find('{').expect("body");
        &src[start..end]
    }

    #[test]
    fn decision_and_resolver_take_no_dt_or_lod() {
        let decision = include_str!("decision.rs");
        let resolver = include_str!("resolver.rs");
        let engine = include_str!("engine.rs");
        for (src, name) in [
            (decision, "choose_action"),
            (resolver, "resolve"),
            (resolver, "resolve_pass"),
            (resolver, "resolve_shot"),
            (resolver, "resolve_tackle"),
            (resolver, "try_receive"),
            (engine, "tick_logic"),
        ] {
            let sig = signature(src, name);
            let lowered = sig.to_lowercase();
            assert!(!sig.contains("LodLevel"), "{name} takes LodLevel: {sig}");
            for banned in ["dt:", "dt :", "delta", "lod"] {
                assert!(!lowered.contains(banned), "{name} takes `{banned}`: {sig}");
            }
        }
        assert_eq!(
            signature(engine, "tick_logic"),
            "pub fn tick_logic(&mut self) "
        );
    }
}
