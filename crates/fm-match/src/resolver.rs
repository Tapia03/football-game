//! `ActionResolver` (spec 3.E, step 4): turns an `Action` into an outcome.
//! Every resolution draws from `rng_for_event(match_seed, player_id,
//! action_count)` and increments `action_count` — the only RNG consumer of
//! the match. No `dt`, no LOD in any signature (criterion 18).

use fm_core::{pitch, Rng, Vec2, Vec3};

use crate::ball::BallFlight;
use crate::decision::Action;
use crate::events::{CardKind, EventKind, MatchEvent, RestartKind};
use crate::formation::Role;
use crate::phase::Side;
use crate::state::{BallState, FlightIntent, MatchState, ShotOutcome, PLAYERS};
use crate::tick_frame::TickFrame;

#[inline]
fn unit(v: u8) -> f32 {
    f32::from(v) / 100.0
}

/// Triangular noise in `[-1, 1]`, peaked at 0 (sum of two uniforms).
fn tri(rng: &mut Rng) -> f32 {
    rng.next_f32() + rng.next_f32() - 1.0
}

fn rotate(v: Vec2, angle: f32) -> Vec2 {
    let (s, c) = (fm_core::math::sin(angle), fm_core::math::cos(angle));
    Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

fn idx_u8(i: usize) -> u8 {
    u8::try_from(i).expect("player index < 22")
}

pub struct ActionResolver;

/// Inputs of a shot's outcome draw.
#[derive(Clone, Copy)]
struct ShotContext {
    dist: f32,
    skill: f32,
    pressure: f32,
    /// Unblocked goal probability (geometry × finisher).
    xg: f32,
    penalty: bool,
}

impl ActionResolver {
    /// Applies `action` by player `actor` to the match state.
    pub fn resolve(state: &mut MatchState, frame: &TickFrame, actor: usize, action: Action) {
        match action {
            Action::Pass { to } => Self::resolve_pass(state, frame, actor, usize::from(to)),
            Action::ThroughPass { to, target } => {
                #[cfg(feature = "diagnostics")]
                {
                    let side = state.players[actor].side;
                    state.team_mut(side).through_passes += 1;
                    state.through_kick_ms = Some(state.now_ms());
                }
                let arrive = state.tuning.pass.through_arrival_speed;
                Self::resolve_pass_to(state, frame, actor, usize::from(to), target, arrive);
            }
            Action::Shoot => Self::resolve_shot(state, frame, actor, false),
            Action::Clear => Self::resolve_clear(state, frame, actor),
            Action::Tackle { on } => Self::resolve_tackle(state, frame, actor, usize::from(on)),
            // Movement-only actions: the carrier's trajectory is planned by
            // the engine; nothing random happens.
            Action::Dribble { .. } | Action::Hold => {}
        }
    }

    fn pressure_on(state: &MatchState, frame: &TickFrame, actor: usize) -> f32 {
        let me = &state.players[actor];
        let pos = frame.pos(actor);
        let radius = state.tuning.decision.pressure_radius;
        let close =
            state.players.iter().enumerate().any(|(j, o)| {
                o.side != me.side && o.active() && frame.pos(j).distance(pos) < radius
            });
        if close {
            1.0
        } else {
            0.0
        }
    }

    fn kick(
        state: &mut MatchState,
        frame: &TickFrame,
        actor: usize,
        v: Vec3,
        intent: FlightIntent,
    ) {
        let now = state.now_ms();
        let from = frame.ball(state);
        state.ball = BallState::Flight {
            flight: BallFlight::kick(now, from, v),
            intent,
        };
        state.last_touch = state.players[actor].side;
        state.players[actor].touch_ready_tick = state.tick + state.tuning.control.retouch_ticks;
        state.holder_ticks = 0;
    }

    /// Pass to teammate `to`. Accuracy from passing/technique/vision (keepers:
    /// distribution); error grows with pressure and distance.
    pub fn resolve_pass(state: &mut MatchState, frame: &TickFrame, actor: usize, to: usize) {
        let arrive = state.tuning.pass.arrival_speed;
        Self::resolve_pass_to(state, frame, actor, to, frame.pos(to), arrive);
    }

    /// Pass meant for `to`, aimed at `target` (their feet, or the space in
    /// front of a runner), arriving at `arrive` m/s if played on the ground.
    pub fn resolve_pass_to(
        state: &mut MatchState,
        frame: &TickFrame,
        actor: usize,
        to: usize,
        target: Vec2,
        arrive: f32,
    ) {
        let mut rng = state.next_rng(actor);
        let k = state.tuning.pass;
        let p = state.players[actor];
        let from = frame.ball(state).xy();
        let skill = if p.role == Role::Goalkeeper {
            unit(p.attrs.goalkeeping.distribution)
        } else {
            0.5 * unit(p.attrs.technical.passing)
                + 0.25 * unit(p.attrs.technical.technique)
                + 0.25 * unit(p.attrs.mental.vision)
        };
        let pressure = Self::pressure_on(state, frame, actor);
        let d = from.distance(target);
        // Rationale: elite passers miss by ~2-3°, poor ones by ~12°, more
        // when pressed; length error up to ±15% for the worst.
        let max_angle = k.angle_base - k.angle_skill * skill + k.angle_pressure * pressure;
        let angle_err = max_angle * tri(&mut rng);
        let len_err = 1.0 + (k.length_base - k.length_skill * skill) * tri(&mut rng);
        let dir = rotate((target - from).normalize(), angle_err);
        let aim_d = (d * len_err).max(1.0);

        let v = if d > k.lofted_dist {
            // Lofted: hang time grows with distance.
            BallFlight::lofted_velocity(dir, aim_d, k.lofted_base_s + aim_d / k.lofted_per_m)
        } else {
            // Arrives at ~8 m/s: firm enough to beat interceptors, soft
            // enough to control (a 20 m pass takes ~1.8 s).
            (dir * BallFlight::rolling_speed_arriving(aim_d, arrive)).extend(0.0)
        };
        state.team_mut(p.side).passes += 1;
        Self::kick(
            state,
            frame,
            actor,
            v,
            FlightIntent::Pass {
                receiver: idx_u8(to),
            },
        );
    }

    /// Shot at goal (or penalty). The outcome is drawn now and applied when the
    /// ball reaches the goal line (spec Fase 4 decisions).
    pub fn resolve_shot(state: &mut MatchState, frame: &TickFrame, actor: usize, penalty: bool) {
        let mut rng = state.next_rng(actor);
        let st = state.tuning.shot;
        let now = state.now_ms();
        let p = state.players[actor];
        let from = frame.ball(state);
        let end = state.attacking(p.side);
        let goal = end.goal_centre();
        let dist = from.xy().distance(goal);
        let shot_skill =
            crate::xg::shot_skill(&p.attrs, dist, state.tuning.decision.long_shot_dist);
        let pressure = if penalty {
            0.0
        } else {
            Self::pressure_on(state, frame, actor)
        };
        // Goal probability of this shot if nothing is in the way: blocking is
        // physical (bodies in the ball's path intercept it in flight), so it
        // is not discounted here — the decision already anticipated it.
        let xg = crate::xg::xg(from.xy(), end, &state.tuning.xg)
            * crate::xg::finisher(shot_skill, &state.tuning.xg);
        #[cfg(feature = "diagnostics")]
        {
            state.team_mut(p.side).xg += xg;
        }

        let (on_target, outcome) = Self::shot_outcome(
            state,
            &mut rng,
            p.side,
            ShotContext {
                dist,
                skill: shot_skill,
                pressure,
                xg,
                penalty,
            },
        );

        // Aim point on the goal line: inside the frame if on target.
        let half = pitch::GOAL_WIDTH / 2.0;
        let (dy, z) = if on_target {
            (
                tri(&mut rng) * (half - 0.4),
                0.2 + rng.next_f32() * (pitch::GOAL_HEIGHT - 0.5),
            )
        } else {
            let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
            (
                side * (half + 0.5 + rng.next_f32() * 4.0),
                0.2 + rng.next_f32() * 2.5,
            )
        };
        let target = Vec3::new(goal.x, goal.y + dy, z);
        let speed = st.speed_base + st.speed_skill * shot_skill;
        let flight_s = (dist / speed).max(0.15);
        let v = BallFlight::aimed_velocity(from, target, flight_s);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // < 5 s
        let arrive_ms = now + (flight_s * 1000.0) as u32;

        let team = state.team_mut(p.side);
        team.shots += 1;
        if on_target {
            team.shots_on_target += 1;
        }
        state.events.push(MatchEvent {
            tick: state.tick,
            kind: EventKind::Shot {
                side: p.side,
                player: p.id,
                on_target,
            },
        });
        Self::kick(
            state,
            frame,
            actor,
            v,
            FlightIntent::Shot {
                shooter: idx_u8(actor),
                outcome,
                arrive_ms,
            },
        );
    }

    /// Draws whether a shot is on target and, if so, whether the keeper saves
    /// it. Draw order is fixed (on-target, then save) — it is part of the
    /// replay contract. Open play: the goal probability is the shot's xG
    /// scaled by the keeper and pressure, and the save probability is what
    /// makes on-target × not-saved equal it.
    fn shot_outcome(
        state: &MatchState,
        rng: &mut Rng,
        side: Side,
        c: ShotContext,
    ) -> (bool, ShotOutcome) {
        let st = state.tuning.shot;
        // On target: better finishers and closer shots; pressure hurts.
        let p_on = if c.penalty {
            st.penalty_on_base + st.penalty_on_skill * c.skill
        } else {
            (st.on_target_base + st.on_target_skill * c.skill
                - c.dist / st.on_target_dist_div
                - st.on_target_pressure * c.pressure)
                .clamp(st.on_target_range.0, st.on_target_range.1)
        };
        let on_target = rng.chance(p_on);

        let defending = side.other();
        let keeper = state.keeper(defending);
        let outcome = if on_target {
            let p_save = keeper.map_or(0.0, |k| {
                let g = &state.players[k].attrs.goalkeeping;
                // One-on-ones matter more close in; reflexes always.
                let close = if c.dist < st.one_on_one_dist {
                    st.one_on_one_close
                } else {
                    st.one_on_one_far
                };
                let gk = 0.4 * unit(g.reflexes)
                    + 0.3 * unit(g.positioning_gk)
                    + 0.15 * unit(g.handling)
                    + close * unit(g.one_on_ones);
                if c.penalty {
                    (st.penalty_save_base + st.penalty_save_keeper * gk
                        - st.penalty_save_skill * c.skill)
                        .clamp(st.penalty_save_range.0, st.penalty_save_range.1)
                } else {
                    let p_goal = c.xg
                        * (st.keeper_base - st.keeper_skill * gk)
                        * (1.0 - st.goal_pressure * c.pressure);
                    (1.0 - p_goal / p_on).clamp(st.save_range.0, st.save_range.1)
                }
            });
            if rng.chance(p_save) {
                ShotOutcome::Saved
            } else {
                ShotOutcome::Goal
            }
        } else {
            ShotOutcome::OffTarget
        };

        (on_target, outcome)
    }

    /// Long, high clearance toward the opponents' half; nobody in particular
    /// is meant to get it.
    fn resolve_clear(state: &mut MatchState, frame: &TickFrame, actor: usize) {
        let mut rng = state.next_rng(actor);
        let k = state.tuning.pass;
        let p = state.players[actor];
        let forward = state.attacking(p.side).direction();
        let dir = rotate(Vec2::new(forward, 0.0), k.clear_angle * tri(&mut rng));
        let d = k.clear_dist * (1.0 + 0.2 * tri(&mut rng));
        let v = BallFlight::lofted_velocity(dir, d, k.lofted_base_s + d / k.lofted_per_m);
        Self::kick(state, frame, actor, v, FlightIntent::Loose);
    }

    /// Tackle on the carrier `on`: foul, clean win, ball knocked loose, or
    /// beaten.
    pub fn resolve_tackle(state: &mut MatchState, frame: &TickFrame, actor: usize, on: usize) {
        let mut rng = state.next_rng(actor);
        let du = state.tuning.duel;
        let di = state.tuning.discipline;
        let tackler = state.players[actor];
        let carrier = state.players[on];
        let t = &tackler.attrs;
        let c = &carrier.attrs;
        // Rationale: tackling vs dribbling decides most duels; base ~35-45%.
        // A keeper smothering at the carrier's feet uses one-on-ones/handling.
        let tackler_skill = if tackler.role == Role::Goalkeeper {
            0.6 * unit(t.goalkeeping.one_on_ones) + 0.4 * unit(t.goalkeeping.handling)
        } else {
            0.6 * unit(t.technical.tackling)
                + 0.2 * unit(t.mental.anticipation)
                + 0.2 * unit(t.physical.strength)
        };
        // Goal-side challenges win more and foul less than ones from behind.
        let goal_side = crate::decision::DecisionSystem::goal_side_cos(state, frame, actor, on);
        let p_win = (du.win_base + du.win_goal_side * goal_side + du.win_tackler * tackler_skill
            - du.win_carrier
                * (0.6 * unit(c.technical.dribbling)
                    + 0.2 * unit(c.physical.agility)
                    + 0.2 * unit(c.physical.balance)))
        .clamp(du.win_range.0, du.win_range.1);
        // Aggressive, clumsy tacklers foul more (~2-12% of challenges;
        // ~10-14 fouls per team per match overall).
        let p_foul = (di.foul_base + di.foul_aggression * unit(t.mental.aggression)
            - di.foul_tackling * unit(t.technical.tackling)
            + di.foul_from_behind * (-goal_side).max(0.0))
        .clamp(di.foul_range.0, di.foul_range.1);
        // Defenders are far more careful inside their own box (penalty risk;
        // ~0.3 penalties per match in real football).
        let own_box = state.attacking(tackler.side).opposite();
        let in_box = pitch::in_penalty_area(frame.ball(state).xy(), own_box);
        let p_foul = if in_box {
            p_foul * di.own_box_factor
        } else {
            p_foul
        };
        // A booked player goes in much more carefully (avoids a second yellow).
        let p_foul = if tackler.yellow_cards > 0 {
            p_foul * di.booked_factor
        } else {
            p_foul
        };
        let roll = rng.next_f32();
        state.players[actor].tackle_ready_tick = state.tick + du.cooldown_ticks;
        state.team_mut(tackler.side).tackles += 1;

        if roll < p_foul {
            Self::foul(state, frame, actor, on, &mut rng);
        } else if roll < p_foul + p_win {
            if rng.chance(du.clean_win) {
                // Clean: tackler comes away with it.
                state.ball = BallState::Held {
                    holder: idx_u8(actor),
                };
                state.holder_ticks = 0;
                state.last_touch = tackler.side;
            } else {
                // Poked loose in a random direction.
                let dir = Vec2::from_angle(rng.next_f32() * fm_core::math::TAU);
                let v = (dir * (3.0 + 4.0 * rng.next_f32())).extend(0.0);
                Self::kick(state, frame, actor, v, FlightIntent::Loose);
            }
        } else {
            state.players[actor].tackle_ready_tick = state.tick + du.beaten_cooldown_ticks;
        }
    }

    fn foul(state: &mut MatchState, frame: &TickFrame, actor: usize, on: usize, rng: &mut Rng) {
        let tackler = state.players[actor];
        let fouled = state.players[on];
        state.events.push(MatchEvent {
            tick: state.tick,
            kind: EventKind::Foul {
                by: tackler.id,
                on: fouled.id,
            },
        });
        // Booking odds rise with aggression: ~1 card per 6-7 fouls; direct
        // reds ~1 per 250 fouls.
        let di = state.tuning.discipline;
        let aggression = unit(tackler.attrs.mental.aggression);
        let card = if rng.chance(di.red_direct) {
            Some(CardKind::Red)
        } else if rng.chance(di.yellow_base + di.yellow_aggression * aggression) {
            Some(CardKind::Yellow)
        } else {
            None
        };
        if let Some(card) = card {
            let pl = &mut state.players[actor];
            let card = if card == CardKind::Yellow {
                pl.yellow_cards += 1;
                if pl.yellow_cards >= 2 {
                    CardKind::Red
                } else {
                    CardKind::Yellow
                }
            } else {
                card
            };
            state.events.push(MatchEvent {
                tick: state.tick,
                kind: EventKind::Card {
                    player: tackler.id,
                    card,
                },
            });
            if card == CardKind::Red {
                state.players[actor].sent_off = true;
            }
        }
        let spot = frame.ball(state).xy();
        let defending_goal = state.attacking(tackler.side).opposite();
        let kind = if pitch::in_penalty_area(spot, defending_goal) {
            RestartKind::Penalty
        } else {
            RestartKind::FreeKick
        };
        let spot = if kind == RestartKind::Penalty {
            defending_goal.penalty_spot()
        } else {
            spot
        };
        crate::engine::set_restart(state, frame, kind, fouled.side, spot);
    }

    /// Probability that `actor` brings the ball under control with one touch
    /// (spec Fase 5 (c1): professional first touch rarely fails on a normal
    /// pass). First touch & technique; fast, high, contested or intercepted
    /// balls are harder.
    #[must_use]
    pub fn control_probability(
        state: &MatchState,
        frame: &TickFrame,
        actor: usize,
        intent: FlightIntent,
        speed: f32,
    ) -> f32 {
        let k = state.tuning.control;
        let p = &state.players[actor];
        let height = frame.ball(state).z;
        let is_keeper = p.role == Role::Goalkeeper;
        let touch = if is_keeper {
            unit(p.attrs.goalkeeping.handling)
        } else {
            0.6 * unit(p.attrs.technical.first_touch) + 0.4 * unit(p.attrs.technical.technique)
        };
        // Cutting out an opponent's pass is a reaction, not a prepared touch.
        let intercepting = matches!(intent, FlightIntent::Pass { receiver }
            if state.players[usize::from(receiver)].side != p.side);
        (k.base + k.touch * touch
            - k.speed_penalty * (speed - k.easy_speed).max(0.0)
            - if height > 1.0 && !is_keeper {
                k.high_ball_penalty
            } else {
                0.0
            }
            - if intercepting {
                k.intercept_penalty
            } else {
                0.0
            }
            - k.pressure_penalty * Self::pressure_on(state, frame, actor))
        .clamp(k.range.0, k.range.1)
    }

    /// A player within reach of a loose ball tries to bring it under control.
    /// Returns true if they now hold it.
    pub fn try_receive(state: &mut MatchState, frame: &TickFrame, actor: usize) -> bool {
        let mut rng = state.next_rng(actor);
        let now = state.now_ms();
        let p = state.players[actor];
        let BallState::Flight { flight, intent } = state.ball else {
            return false;
        };
        let vel = flight.vel_at(now);
        let p_control = Self::control_probability(state, frame, actor, intent, vel.length());

        if rng.chance(p_control) {
            if let FlightIntent::Pass { receiver } = intent {
                if state.players[usize::from(receiver)].side == p.side {
                    state.team_mut(p.side).passes_completed += 1;
                    #[cfg(feature = "diagnostics")]
                    if state.through_kick_ms == Some(flight.kick_ms) {
                        state.team_mut(p.side).through_completed += 1;
                    }
                }
            }
            state.ball = BallState::Held {
                holder: idx_u8(actor),
            };
            state.holder_ticks = 0;
            state.last_touch = p.side;
            true
        } else {
            // Miscontrol: ball spills off at reduced speed in a new direction.
            let angle = tri(&mut rng) * 1.2;
            let spill = rotate(vel.xy(), angle) * 0.4;
            let v = spill.extend((vel.z * -0.3).max(0.0));
            Self::kick(state, frame, actor, v, FlightIntent::Loose);
            false
        }
    }

    /// Players able to play the ball this tick, nearest first (index breaks
    /// ties), written into `out`; returns how many.
    pub fn receive_candidates(
        state: &MatchState,
        frame: &TickFrame,
        out: &mut [u8; PLAYERS],
    ) -> usize {
        let BallState::Flight { intent, .. } = state.ball else {
            return 0;
        };
        let ball = frame.ball(state);
        let k = &state.tuning.control;
        let shot_in_flight = matches!(intent, FlightIntent::Shot { .. });
        let pass = match intent {
            FlightIntent::Pass { receiver } => Some((
                usize::from(receiver),
                state.players[usize::from(receiver)].side,
            )),
            _ => None,
        };
        let mut n = 0;
        let mut dists = [0.0_f32; PLAYERS];
        for (i, p) in state.players.iter().enumerate() {
            if !p.active() || state.tick < p.touch_ready_tick {
                continue;
            }
            let is_keeper = p.role == Role::Goalkeeper;
            // A shot's save/goal is already decided: keepers don't re-touch it.
            if shot_in_flight && is_keeper {
                continue;
            }
            let own_box = pitch::in_penalty_area(frame.pos(i), state.attacking(p.side).opposite());
            let (reach, max_h) = if is_keeper && own_box {
                (k.keeper_reach, k.keeper_max_height)
            } else {
                match pass {
                    Some((r, _)) if r == i => (k.receiver_radius, k.max_height),
                    Some((_, side)) if side != p.side => (k.intercept_radius, k.max_height),
                    _ => (k.control_radius, k.max_height),
                }
            };
            let d = frame.pos(i).distance(ball.xy());
            if d < reach && ball.z < max_h {
                out[n] = idx_u8(i);
                dists[n] = d;
                n += 1;
            }
        }
        // Insertion sort by (distance, index): n ≤ 22, no allocation.
        for i in 1..n {
            let mut j = i;
            while j > 0 && (dists[j] < dists[j - 1]) {
                dists.swap(j, j - 1);
                out.swap(j, j - 1);
                j -= 1;
            }
        }
        n
    }

    /// Resolves a penalty kick by the restart taker.
    pub fn take_penalty(state: &mut MatchState, frame: &TickFrame, taker: usize) {
        Self::resolve_shot(state, frame, taker, true);
    }

    /// Used by the engine for side bookkeeping.
    #[must_use]
    pub fn side_of(state: &MatchState, idx: usize) -> Side {
        state.players[idx].side
    }
}

#[cfg(test)]
mod tests {
    use super::ActionResolver;
    use crate::ball::BallFlight;
    use crate::state::{BallState, FlightIntent};
    use crate::test_support::placed_state;
    use crate::tick_frame::TickFrame;
    use fm_core::{Vec2, Vec3};

    /// Home player 7 receiving a ground pass from 9 at `speed` m/s, with an
    /// optional opponent (13) at `opp`.
    fn p_control(first_touch: u8, speed: f32, opp: Option<Vec2>, height: f32) -> f32 {
        let at = Vec2::new(50.0, 30.0);
        let mut placed = vec![(7, at), (9, Vec2::new(30.0, 30.0))];
        if let Some(o) = opp {
            placed.push((13, o));
        }
        let mut s = placed_state(&placed, 9);
        s.players[7].attrs.technical.first_touch = first_touch;
        s.players[7].attrs.technical.technique = first_touch;
        let now = s.now_ms();
        s.ball = BallState::Flight {
            flight: BallFlight::kick(now, at.extend(height), Vec3::new(speed, 0.0, 0.0)),
            intent: FlightIntent::Pass { receiver: 7 },
        };
        let f = TickFrame::capture(&s);
        ActionResolver::control_probability(&s, &f, 7, FlightIntent::Pass { receiver: 7 }, speed)
    }

    #[test]
    fn first_touch_is_professional_on_a_normal_pass() {
        let typical = p_control(50, 8.0, None, 0.0);
        let good = p_control(80, 8.0, None, 0.0);
        assert!((0.88..=0.92).contains(&typical), "typical {typical}");
        assert!((0.93..=0.98).contains(&good), "good {good}");
    }

    #[test]
    fn hard_balls_are_harder_to_control() {
        let base = p_control(50, 8.0, None, 0.0);
        assert!(p_control(50, 20.0, None, 0.0) < base, "fast");
        assert!(p_control(50, 8.0, None, 1.5) < base, "high");
        assert!(
            p_control(50, 8.0, Some(Vec2::new(51.0, 30.0)), 0.0) < base,
            "pressed"
        );
    }

    #[test]
    fn through_ball_rolls_into_the_space_for_the_runner() {
        use crate::decision::Action;
        let target = Vec2::new(80.0, 34.0);
        let mut s = placed_state(&[(5, Vec2::new(60.0, 34.0)), (9, Vec2::new(70.0, 30.0))], 5);
        let f = TickFrame::capture(&s);
        ActionResolver::resolve(&mut s, &f, 5, Action::ThroughPass { to: 9, target });
        let BallState::Flight { flight, intent } = s.ball else {
            panic!("ball should be in flight");
        };
        assert_eq!(intent, FlightIntent::Pass { receiver: 9 });
        // Where the ball comes to rest: near the space aimed at (passing
        // error included), not at the receiver's feet.
        let rest = flight.pos_at(f.now_ms + 20_000).xy();
        assert!(rest.distance(target) < 4.0, "rests at {rest:?}");
        assert!(rest.distance(Vec2::new(70.0, 30.0)) > 5.0);
    }
}
