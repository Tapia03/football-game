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
    /// Kick it long and away: the carrier has nothing better (forced release
    /// without a pass on).
    Clear,
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

/// Whole ticks in the carry-decay span (it is configured as `f32`).
fn carry_span_ticks(span: f32) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // small, ≥ 0
    let ticks = span.max(0.0).ceil() as u32;
    ticks
}

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
        // A shot is taken on its own value: the estimated xG after blocking
        // (spec Fase 5 (c1), item 1).
        if Self::shot_xg(state, frame, me) >= state.tuning.xg.shoot_xg_min {
            return Action::Shoot;
        }
        let (best_pass, best_to) = Self::best_pass(state, frame, me, my_pos, forward);
        let dribble_score = Self::dribble_score(state, frame, me, my_pos, forward, dist_goal);

        // Forced release: once carrying has no value left, holding forever is
        // not an option (it deadlocked the match). Play the best available
        // pass whatever its score, or clear it. Never a forced shot: a shot
        // is only taken on its own value.
        if state.holder_ticks >= t.carry_decay_start + carry_span_ticks(t.carry_decay_span) {
            return best_to.map_or(Action::Clear, |to| Action::Pass { to });
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

    /// The carrier's estimated xG for a shot now: geometry × finisher ×
    /// uncovered share of the goal mouth. Zero for keepers and out of range.
    #[must_use]
    pub fn shot_xg(state: &MatchState, frame: &TickFrame, me: usize) -> f32 {
        let t = &state.tuning.decision;
        let p = &state.players[me];
        let end = state.attacking(p.side);
        let pos = frame.pos(me);
        let dist_goal = pos.distance(end.goal_centre());
        if dist_goal >= t.shoot_range || p.role == Role::Goalkeeper {
            return 0.0;
        }
        let skill = crate::xg::shot_skill(&p.attrs, dist_goal, t.long_shot_dist);
        crate::xg::xg(pos, end, &state.tuning.xg)
            * crate::xg::finisher(skill, &state.tuning.xg)
            * (1.0 - crate::xg::coverage(state, frame, me, end))
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

    /// cos of the angle at the carrier between the defender and the goal the
    /// defender protects: +1 squarely goal-side, −1 chasing from behind,
    /// 0 alongside. No trigonometry: normalised dot product.
    #[must_use]
    pub fn goal_side_cos(
        state: &MatchState,
        frame: &TickFrame,
        defender: usize,
        carrier: usize,
    ) -> f32 {
        let c = frame.pos(carrier);
        let own_goal = state
            .attacking(state.players[defender].side)
            .opposite()
            .goal_centre();
        (frame.pos(defender) - c)
            .normalize()
            .dot((own_goal - c).normalize())
    }

    /// Where the first defender stands against the carrier: goal-side, at a
    /// distance set by the danger zone of the carrier's position (spec Fase 5
    /// containment bands). Inside a band, aggressive defenders stand tighter.
    #[must_use]
    pub fn containment_point(
        state: &MatchState,
        frame: &TickFrame,
        defender: usize,
        carrier: usize,
    ) -> Vec2 {
        let t = &state.tuning.defending;
        let c = frame.pos(carrier);
        let own_goal = state
            .attacking(state.players[defender].side)
            .opposite()
            .goal_centre();
        let to_goal = own_goal - c;
        let danger = to_goal.length();
        let band = if danger <= t.zone_box_dist {
            t.contain_box
        } else if danger <= t.zone_mid_dist {
            t.contain_mid
        } else {
            t.contain_far
        };
        let aggression = f32::from(state.players[defender].attrs.mental.aggression) / 100.0;
        let dist = band.1 - (band.1 - band.0) * aggression;
        c + to_goal.normalize() * dist
    }

    /// Challenge decision for one eligible defender: deterministic score from
    /// goal-side angle, carrier vulnerability, defender profile and context.
    #[must_use]
    pub fn challenge_score(
        state: &MatchState,
        frame: &TickFrame,
        defender: usize,
        carrier: usize,
    ) -> f32 {
        let t = &state.tuning.defending;
        let d = &state.players[defender];
        let unit = |v: u8| f32::from(v) / 100.0;
        let fresh = if state.holder_ticks < t.fresh_ticks {
            t.w_fresh
        } else {
            0.0
        };
        let transition = if frame.phase(d.side) == crate::phase::Phase::TransitionDefense {
            t.transition_bonus
        } else {
            0.0
        };
        let own_box = state.attacking(d.side).opposite();
        let in_own_box = pitch::in_penalty_area(frame.pos(carrier), own_box);
        let is_keeper = d.role == Role::Goalkeeper;
        // A keeper coming out in their own box is the box's normal defence.
        let box_penalty = if in_own_box && !is_keeper {
            t.own_box_penalty
        } else {
            0.0
        };
        // A carrier who lingers on the ball becomes a target.
        #[allow(clippy::cast_precision_loss)] // ticks ≪ 2^23
        let linger =
            t.w_linger * (state.holder_ticks as f32 / t.linger_ticks.max(1) as f32).min(1.0);
        // Keepers read the duel with one-on-ones instead of tackling.
        let skill = if is_keeper {
            unit(d.attrs.goalkeeping.one_on_ones)
        } else {
            unit(d.attrs.technical.tackling)
        };
        t.challenge_base
            + t.w_goal_side * Self::goal_side_cos(state, frame, defender, carrier)
            + fresh
            + linger
            + t.w_tackling * skill
            + t.w_decisions * unit(d.attrs.mental.decisions)
            + t.w_aggression * unit(d.attrs.mental.aggression)
            + transition
            - box_penalty
    }

    /// The defender of `side` who commits to a challenge on `carrier` this
    /// tick, if any: among players in reach and recovered (keepers only in
    /// their own box), the highest
    /// challenge score above the threshold (lowest index on ties).
    #[must_use]
    pub fn choose_challenger(
        state: &MatchState,
        frame: &TickFrame,
        side: crate::phase::Side,
        carrier: usize,
    ) -> Option<usize> {
        let reach = state.tuning.duel.tackle_range;
        let threshold = state.tuning.defending.challenge_threshold;
        let c = frame.pos(carrier);
        let own_box = state.attacking(side).opposite();
        let keeper_may =
            state.tuning.defending.keeper_smother && pitch::in_penalty_area(c, own_box);
        let mut best = None;
        let mut best_score = threshold;
        for (i, p) in state.players.iter().enumerate() {
            if p.side != side || !p.active() {
                continue;
            }
            if p.role == Role::Goalkeeper && !keeper_may {
                continue;
            }
            if state.tick < p.tackle_ready_tick || frame.pos(i).distance(c) >= reach {
                continue;
            }
            let score = Self::challenge_score(state, frame, i, carrier);
            if score > best_score {
                best_score = score;
                best = Some(i);
            }
        }
        best
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
    use super::{dist_to_segment, Action, DecisionSystem};
    use crate::test_support::placed_state;
    use crate::tick_frame::TickFrame;
    use fm_core::{GoalEnd, Vec2};

    /// Home striker (10) on the ball `dist` m straight out from the right
    /// goal, `holding` ticks into possession, with extra placed players.
    fn striker_at(dist: f32, holding: u32, others: &[(usize, Vec2)]) -> Action {
        let c = GoalEnd::Right.goal_centre();
        let mut placed = vec![(10, Vec2::new(c.x - dist, c.y))];
        placed.extend_from_slice(others);
        let mut s = placed_state(&placed, 10);
        s.holder_ticks = holding;
        let f = TickFrame::capture(&s);
        DecisionSystem::choose_action(&s, &f, 10)
    }

    #[test]
    fn open_shot_is_taken_on_its_own_value() {
        assert_eq!(striker_at(11.0, 5, &[]), Action::Shoot);
    }

    #[test]
    fn covered_goal_is_not_shot_at() {
        let c = GoalEnd::Right.goal_centre();
        let blocker = (13, Vec2::new(c.x - 13.0, c.y));
        assert_ne!(striker_at(15.0, 5, &[blocker]), Action::Shoot);
    }

    #[test]
    fn forced_release_never_shoots() {
        // Long on the ball, goal covered, no team-mate within passing range
        // (they are all parked far away): the carrier clears, never shoots.
        let c = GoalEnd::Right.goal_centre();
        let blocker = (13, Vec2::new(c.x - 13.0, c.y));
        assert_eq!(striker_at(15.0, 500, &[blocker]), Action::Clear);
        // With a team-mate on, it is a pass.
        let mate = (9, Vec2::new(c.x - 25.0, c.y + 10.0));
        assert_eq!(
            striker_at(15.0, 500, &[blocker, mate]),
            Action::Pass { to: 9 }
        );
    }

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
