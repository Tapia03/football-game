//! `DecisionSystem::choose_action` (spec 3.E, step 3) — Phase 4 minimum.
//!
//! Deterministic scoring, no RNG: randomness belongs to `ActionResolver`.
//! Takes only the match state and the actor — no `dt`, no LOD (criterion 18).
//! Phase 5 replaces the scoring with full role behaviours.

use fm_core::{pitch, GoalEnd, Vec2};

use crate::formation::Role;
use crate::state::{MatchState, PLAYERS};
use crate::tick_frame::TickFrame;
use crate::value;

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

impl DecisionSystem {
    /// What the carrier `me` does this tick. Call only for the ball holder.
    ///
    /// Every option is valued in one currency — the probability that this
    /// possession ends in a goal (spec Fase 5 (c1), item 2): shots by their
    /// estimated xG, everything else by Expected Threat after the action,
    /// minus the opponents' threat where the ball would be lost. The carrier
    /// keeps the ball unless something beats keeping it by `act_margin`.
    #[must_use]
    pub fn choose_action(state: &MatchState, frame: &TickFrame, me: usize) -> Action {
        let t = &state.tuning.decision;
        let v = &state.tuning.value;
        let p = &state.players[me];
        let my_pos = frame.pos(me);
        let end = state.attacking(p.side);
        let is_keeper = p.role == Role::Goalkeeper;

        let nearest_opp = state
            .players
            .iter()
            .enumerate()
            .filter(|(_, o)| o.side != p.side && o.active())
            .map(|(j, _)| frame.pos(j).distance(my_pos))
            .fold(f32::MAX, f32::min);
        let pressed = nearest_opp < t.pressure_radius;
        let min_hold = if is_keeper {
            t.keeper_hold_ticks
        } else {
            t.min_hold_ticks
        };
        if state.holder_ticks < min_hold && !pressed {
            return Action::Hold;
        }

        // Forced release (deadlock guard): best pass whatever its value, or
        // clear. Never a forced shot: a shot is only taken on its value.
        if state.holder_ticks >= v.forced_release_ticks {
            let (_, best_to) = Self::best_pass(state, frame, me, f32::MIN);
            return best_to.map_or(Action::Clear, |to| Action::Pass { to });
        }

        let hold_keep = if pressed { v.hold_keep_pressed } else { 1.0 };
        #[allow(clippy::cast_precision_loss)] // ticks on the ball ≪ 2^23
        let waited = state.holder_ticks as f32;
        let hold_ev = Self::keep_or_lose(state, my_pos, my_pos, end, hold_keep)
            * (1.0 - v.hold_erosion * waited).max(0.0);

        // Candidates in a fixed order; strict `>` keeps the earliest on ties.
        let mut best = Action::Hold;
        let mut best_ev = hold_ev + v.act_margin;
        // Coverage only lowers a shot's value: skip it when even the
        // unblocked shot cannot win (same result, less work).
        let open_shot = Self::open_shot_xg(state, frame, me);
        if open_shot > best_ev {
            let shot_ev = open_shot * (1.0 - crate::xg::coverage(state, frame, me, end));
            if shot_ev > best_ev {
                best = Action::Shoot;
                best_ev = shot_ev;
            }
        }
        let (best_pass_ev, best_to) = Self::best_pass(state, frame, me, best_ev);
        #[cfg(debug_assertions)]
        {
            // The pruned search must agree with the full one whenever its
            // result can be chosen.
            let (full_ev, full_to) = Self::best_pass(state, frame, me, f32::MIN);
            if full_ev > best_ev {
                debug_assert_eq!(
                    (best_pass_ev.to_bits(), best_to),
                    (full_ev.to_bits(), full_to)
                );
            } else {
                debug_assert!(best_to.is_none() || best_pass_ev <= best_ev);
            }
        }
        if let Some(to) = best_to {
            if best_pass_ev > best_ev {
                best = Action::Pass { to };
                best_ev = best_pass_ev;
            }
        }
        if !is_keeper {
            let (dribble_ev, target) = Self::dribble(state, frame, me);
            if dribble_ev > best_ev {
                best = Action::Dribble { target };
            }
        }
        best
    }

    /// `keep · xT(at_keep) − (1 − keep) · opponents' xT(at_lose)`.
    fn keep_or_lose(
        state: &MatchState,
        at_keep: Vec2,
        at_lose: Vec2,
        end: GoalEnd,
        keep: f32,
    ) -> f32 {
        let grid = &state.tuning.value.xt;
        keep * value::xt(grid, at_keep, end)
            - (1.0 - keep) * value::xt(grid, at_lose, end.opposite())
    }

    /// The carrier's estimated xG for a shot now: geometry × finisher ×
    /// uncovered share of the goal mouth. Zero for keepers and out of range.
    #[must_use]
    pub fn shot_xg(state: &MatchState, frame: &TickFrame, me: usize) -> f32 {
        let end = state.attacking(state.players[me].side);
        Self::open_shot_xg(state, frame, me) * (1.0 - crate::xg::coverage(state, frame, me, end))
    }

    /// Shot xG before blocking: geometry × finisher.
    fn open_shot_xg(state: &MatchState, frame: &TickFrame, me: usize) -> f32 {
        let t = &state.tuning.decision;
        let p = &state.players[me];
        let end = state.attacking(p.side);
        let pos = frame.pos(me);
        let dist_goal = pos.distance(end.goal_centre());
        if dist_goal >= t.shoot_range || p.role == Role::Goalkeeper {
            return 0.0;
        }
        let skill = crate::xg::shot_skill(&p.attrs, dist_goal, t.long_shot_dist);
        crate::xg::xg(pos, end, &state.tuning.xg) * crate::xg::finisher(skill, &state.tuning.xg)
    }

    /// Estimated chance that a pass from `me` to `to` arrives and is
    /// controlled: not cut out in the lane × on target × first touch.
    #[must_use]
    pub fn pass_success(state: &MatchState, frame: &TickFrame, me: usize, to: usize) -> f32 {
        let dt = &state.tuning.decision;
        let v = &state.tuning.value;
        let ct = &state.tuning.control;
        let p = &state.players[me];
        let r = &state.players[to];
        let from = frame.pos(me);
        let target = frame.pos(to);
        let d = from.distance(target);
        let dir = (target - from) / d.max(1e-3);
        let pressure_r2 = dt.pressure_radius * dt.pressure_radius;
        // Probability the pass survives every opponent along its lane.
        let mut survive = 1.0;
        let mut receiver_pressed = false;
        for (j, o) in state.players.iter().enumerate() {
            if o.side == p.side || !o.active() {
                continue;
            }
            let oj = frame.pos(j);
            receiver_pressed |= (oj - target).length_squared() < pressure_r2;
            let rel = oj - from;
            let along = rel.dot(dir).clamp(0.0, d);
            let reach = v.body_reach + o.top_speed * (along / v.pass_speed - v.react_s).max(0.0);
            // Squared distances first: the square root only for the few
            // opponents actually within reach of the lane.
            let off2 = (rel - dir * along).length_squared();
            if off2 < reach * reach {
                survive *= 1.0 - v.pass_intercept_max * (1.0 - fm_core::math::sqrt(off2) / reach);
            }
        }
        let skill = if p.role == Role::Goalkeeper {
            f32::from(p.attrs.goalkeeping.distribution) / 100.0
        } else {
            (0.5 * f32::from(p.attrs.technical.passing)
                + 0.25 * f32::from(p.attrs.technical.technique)
                + 0.25 * f32::from(p.attrs.mental.vision))
                / 100.0
        };
        let accuracy = (1.0 - d * v.pass_error_per_m * (1.5 - skill)).max(v.pass_accuracy_min);
        let touch = (0.6 * f32::from(r.attrs.technical.first_touch)
            + 0.4 * f32::from(r.attrs.technical.technique))
            / 100.0;
        let control = (ct.base + ct.touch * touch
            - if d > state.tuning.pass.lofted_dist {
                ct.high_ball_penalty
            } else {
                0.0
            }
            - if receiver_pressed {
                ct.pressure_penalty
            } else {
                0.0
            })
        .clamp(ct.range.0, ct.range.1);
        survive * accuracy * control
    }

    /// Best pass target and its expected value: success × xT at the
    /// receiver − failure × the opponents' xT there. Targets whose threat
    /// cannot beat `floor` are skipped before the costly success estimate
    /// (a pass is never worth more than the threat where it arrives); the
    /// result is then the same whenever it beats `floor`.
    fn best_pass(
        state: &MatchState,
        frame: &TickFrame,
        me: usize,
        floor: f32,
    ) -> (f32, Option<u8>) {
        let k = &state.tuning.decision;
        let p = &state.players[me];
        let my_pos = frame.pos(me);
        let end = state.attacking(p.side);
        let mut best_ev = f32::MIN;
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
            let grid = &state.tuning.value.xt;
            let gain = value::xt(grid, tp, end);
            if gain <= floor || gain <= best_ev {
                continue;
            }
            let success = Self::pass_success(state, frame, me, i);
            let ev = success * gain - (1.0 - success) * value::xt(grid, tp, end.opposite());
            // Strict `>` keeps the lowest index on ties: deterministic.
            if ev > best_ev {
                best_ev = ev;
                best_to = u8::try_from(i).ok();
            }
        }
        (best_ev, best_to)
    }

    /// Best dribble step (straight on or 45° either way): expected value
    /// and target. Into space the ball is nearly always kept; into a marker
    /// it depends on dribbling (item 7 replaces this with the 1v1 duel).
    fn dribble(state: &MatchState, frame: &TickFrame, me: usize) -> (f32, Vec2) {
        let t = &state.tuning.decision;
        let v = &state.tuning.value;
        let p = &state.players[me];
        let my_pos = frame.pos(me);
        let end = state.attacking(p.side);
        let forward = end.direction();
        let cramped_keep = v.dribble_keep_cramped_base
            + v.dribble_keep_cramped_skill * f32::from(p.attrs.technical.dribbling) / 100.0;
        let mut best = (f32::MIN, my_pos);
        // Straight first so it wins ties; diagonals are unit vectors too.
        for dy in [0.0, 1.0, -1.0] {
            let dir = Vec2::new(forward, dy).normalize();
            let ahead = my_pos + dir * t.dribble_probe_ahead;
            let space = state
                .players
                .iter()
                .enumerate()
                .filter(|(_, o)| o.side != p.side && o.active())
                .map(|(j, _)| frame.pos(j).distance(ahead))
                .fold(f32::MAX, f32::min);
            let keep = if space > t.dribble_space_min {
                v.dribble_keep_open
            } else {
                cramped_keep
            };
            let step = my_pos + dir * t.dribble_step;
            let target = Vec2::new(
                step.x.clamp(1.0, pitch::LENGTH - 1.0),
                step.y.clamp(1.0, pitch::WIDTH - 1.0),
            );
            let ev = Self::keep_or_lose(state, target, my_pos, end, keep);
            if ev > best.0 {
                best = (ev, target);
            }
        }
        best
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
    use super::{Action, DecisionSystem};
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
    fn shoots_when_the_shot_beats_carrying_on() {
        // Unmarked 7 m out: xG ≈ 0.35 beats the threat of carrying closer.
        assert_eq!(striker_at(7.0, 5, &[]), Action::Shoot);
    }

    #[test]
    fn carries_in_when_unmarked_far_out() {
        // Unmarked 20 m out (xG ≈ 0.05): carrying into the box is worth more.
        assert!(matches!(striker_at(20.0, 5, &[]), Action::Dribble { .. }));
    }

    #[test]
    fn passes_into_threat_through_an_open_lane_only() {
        let c = GoalEnd::Right.goal_centre();
        // Carrier 45 m out with a marker in front of them, off to one side
        // (dribbling is cramped); a team-mate 20 m further forward in space.
        let marker = (13, Vec2::new(c.x - 42.0, c.y - 3.0));
        let mate = (9, Vec2::new(c.x - 25.0, c.y + 12.0));
        assert_eq!(striker_at(45.0, 5, &[marker, mate]), Action::Pass { to: 9 });
        // A defender standing in that lane: the pass is no longer worth it.
        let in_lane = (14, Vec2::new(c.x - 35.0, c.y + 6.0));
        assert_ne!(
            striker_at(45.0, 5, &[marker, mate, in_lane]),
            Action::Pass { to: 9 }
        );
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
