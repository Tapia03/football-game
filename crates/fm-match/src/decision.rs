//! `DecisionSystem::choose_action` (spec 3.E, step 3) — Phase 4 minimum.
//!
//! Deterministic scoring, no RNG: randomness belongs to `ActionResolver`.
//! Takes only the match state and the actor — no `dt`, no LOD (criterion 18).
//! Phase 5 replaces the scoring with full role behaviours.

use fm_core::{pitch, Vec2};

use crate::formation::Role;
use crate::state::{MatchState, PLAYERS};
use crate::tick_frame::TickFrame;

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
    pub fn choose_action(state: &MatchState, frame: &TickFrame, me: usize) -> Action {
        let t = &state.tuning.decision;
        let p = &state.players[me];
        let my_pos = frame.pos(me);
        let goal = state.attacking(p.side).goal_centre();
        let forward = state.attacking(p.side).direction();

        let nearest_opp = state
            .players
            .iter()
            .enumerate()
            .filter(|(_, o)| o.side != p.side && o.active())
            .map(|(j, _)| frame.pos(j).distance(my_pos))
            .fold(f32::MAX, f32::min);
        let pressed = nearest_opp < t.pressure_radius;
        let min_hold = if p.role == Role::Goalkeeper {
            t.keeper_hold_ticks
        } else {
            t.min_hold_ticks
        };
        if state.holder_ticks < min_hold && !pressed {
            return Action::Hold;
        }

        let dist_goal = my_pos.distance(goal);
        let shoot_score = Self::shoot_score(state, frame, me, my_pos, goal);
        let (best_pass, best_to) = Self::best_pass(state, frame, me, my_pos, forward);
        let dribble_score = Self::dribble_score(state, frame, me, my_pos, forward, dist_goal);

        if shoot_score > t.shoot_threshold
            && shoot_score >= best_pass
            && shoot_score >= dribble_score
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
                (my_pos.x + forward * t.dribble_step).clamp(1.0, pitch::LENGTH - 1.0),
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
    fn shoot_score(
        state: &MatchState,
        frame: &TickFrame,
        me: usize,
        my_pos: Vec2,
        goal: Vec2,
    ) -> f32 {
        let t = &state.tuning.decision;
        let p = &state.players[me];
        let dist_goal = my_pos.distance(goal);
        if dist_goal >= t.shoot_range || p.role == Role::Goalkeeper {
            return 0.0;
        }
        let skill = if dist_goal > t.long_shot_dist {
            f32::from(p.attrs.technical.long_shots)
        } else {
            f32::from(p.attrs.technical.finishing)
        } / 100.0;
        let central = (1.0 - (my_pos.y - pitch::HALF_WIDTH).abs() / t.central_width)
            .clamp(t.central_min, 1.0);
        // Bodies in the shooting lane (outfield only) block most shots.
        let lane = state
            .players
            .iter()
            .enumerate()
            .filter(|(_, o)| o.side != p.side && o.active() && o.role != Role::Goalkeeper)
            .map(|(j, _)| dist_to_segment(frame.pos(j), my_pos, goal))
            .fold(f32::MAX, f32::min);
        let blocked = if lane < t.shot_block_dist {
            t.shot_blocked_factor
        } else {
            1.0
        };
        (t.shoot_base + t.shoot_skill * skill)
            * (1.0 - dist_goal / t.shoot_range)
            * central
            * blocked
            * t.shoot_gain
    }

    /// Best pass target and its score: progress toward goal, open lane, not
    /// too long.
    fn best_pass(
        state: &MatchState,
        frame: &TickFrame,
        me: usize,
        my_pos: Vec2,
        forward: f32,
    ) -> (f32, Option<u8>) {
        let k = &state.tuning.decision;
        let p = &state.players[me];
        let mut best_pass = f32::MIN;
        let mut best_to = None;
        for (i, t) in state.players.iter().enumerate() {
            if i == me || t.side != p.side || !t.active() {
                continue;
            }
            let tp = frame.pos(i);
            let d = my_pos.distance(tp);
            if !(k.pass_min_dist..=k.pass_max_dist).contains(&d) {
                continue;
            }
            let progress = (tp.x - my_pos.x) * forward;
            let openness = state
                .players
                .iter()
                .enumerate()
                .filter(|(_, o)| o.side != p.side && o.active())
                .map(|(j, _)| dist_to_segment(frame.pos(j), my_pos, tp))
                .fold(k.pass_lane_cap, f32::min);
            let mut score =
                k.pass_base + progress / k.pass_progress_div + openness / k.pass_open_div
                    - d / k.pass_len_div;
            // Lanes narrower than ~2.5 m tend to get cut out.
            if openness < k.pass_narrow_lane {
                score -= (k.pass_narrow_lane - openness) * k.pass_narrow_penalty;
            }
            if t.role == Role::Goalkeeper {
                score -= k.pass_to_keeper_penalty;
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
        frame: &TickFrame,
        me: usize,
        my_pos: Vec2,
        forward: f32,
        dist_goal: f32,
    ) -> f32 {
        let t = &state.tuning.decision;
        let p = &state.players[me];
        if p.role == Role::Goalkeeper {
            return f32::MIN;
        }
        let ahead = Vec2::new(my_pos.x + forward * t.dribble_probe_ahead, my_pos.y);
        let space = state
            .players
            .iter()
            .enumerate()
            .filter(|(_, o)| o.side != p.side && o.active())
            .map(|(j, _)| frame.pos(j).distance(ahead))
            .fold(f32::MAX, f32::min);
        // Long carries are rare: the value of dribbling decays after ~2.5 s
        // on the ball, pushing the carrier to release it.
        #[allow(clippy::cast_precision_loss)] // ticks on the ball ≪ 2^23
        let carry_decay = (1.0
            - (state.holder_ticks.saturating_sub(t.carry_decay_start) as f32) / t.carry_decay_span)
            .max(0.0);
        // Near goal, running into a packed box rarely beats a pass.
        let near_goal = if dist_goal < t.near_goal_dist {
            t.near_goal_factor
        } else {
            1.0
        };
        if space > t.dribble_space_min {
            (t.dribble_base + space.min(t.dribble_space_cap) / t.dribble_space_div)
                * carry_decay
                * near_goal
        } else {
            t.dribble_cramped * carry_decay * near_goal
        }
    }

    /// The defender of `side` closest to the ball, who presses it this tick.
    #[must_use]
    pub fn presser(
        state: &MatchState,
        frame: &TickFrame,
        side: crate::phase::Side,
    ) -> Option<usize> {
        let ball = frame.ball(state).xy();
        let mut best = None;
        let mut best_d = f32::MAX;
        for i in 0..PLAYERS {
            let p = &state.players[i];
            if p.side != side || !p.active() || p.role == Role::Goalkeeper {
                continue;
            }
            let d = frame.pos(i).distance(ball);
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
