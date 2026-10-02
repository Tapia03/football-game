//! `ActionResolver` (spec 3.E, step 4): turns an `Action` into an outcome.
//! Every resolution draws from `rng_for_event(match_seed, player_id,
//! action_count)` and increments `action_count` — the only RNG consumer of
//! the match. No `dt`, no LOD in any signature (criterion 18).

use fm_core::{pitch, Rng, Vec2, Vec3};

use crate::ball::BallFlight;
use crate::decision::{Action, PRESSURE_RADIUS};
use crate::events::{CardKind, EventKind, MatchEvent, RestartKind};
use crate::formation::Role;
use crate::phase::Side;
use crate::state::{BallState, FlightIntent, MatchState, ShotOutcome, PLAYERS};
use crate::tick_frame::TickFrame;

/// Radius within which a player can play a loose ball at foot height (m).
pub const CONTROL_RADIUS: f32 = 1.2;
/// The intended receiver of a pass is set for it and reaches further.
pub const RECEIVER_RADIUS: f32 = 1.5;
/// A player cutting across someone else's pass has to react: shorter reach.
pub const INTERCEPT_RADIUS: f32 = 0.9;
/// Keepers reach further and higher inside their own box.
pub const KEEPER_REACH: f32 = 2.2;
/// Speed a ground pass should still have when it reaches the receiver (m/s).
pub const PASS_ARRIVAL_SPEED: f32 = 8.0;
/// After any challenge, the whole team waits this long before the next one
/// (~7 s): real teams make ~30-40 tackles a match, not a continuous scrum.
pub const TEAM_TACKLE_GAP: u32 = 70;
/// Ticks a kicker must wait before touching the ball again.
const RETOUCH_TICKS: u32 = 5;
/// Ticks a tackler waits after any attempt (recovery, ~2.5 s); longer if
/// beaten (~4 s). Real teams attempt ~20-40 tackles per match.
const TACKLE_COOLDOWN: u32 = 25;
const BEATEN_COOLDOWN: u32 = 40;

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

impl ActionResolver {
    /// Applies `action` by player `actor` to the match state.
    pub fn resolve(state: &mut MatchState, frame: &TickFrame, actor: usize, action: Action) {
        match action {
            Action::Pass { to } => Self::resolve_pass(state, frame, actor, usize::from(to)),
            Action::Shoot => Self::resolve_shot(state, frame, actor, false),
            Action::Tackle { on } => Self::resolve_tackle(state, frame, actor, usize::from(on)),
            // Movement-only actions: the carrier's trajectory is planned by
            // the engine; nothing random happens.
            Action::Dribble { .. } | Action::Hold => {}
        }
    }

    fn pressure_on(state: &MatchState, frame: &TickFrame, actor: usize) -> f32 {
        let me = &state.players[actor];
        let pos = frame.pos(actor);
        let close = state.players.iter().enumerate().any(|(j, o)| {
            o.side != me.side && o.active() && frame.pos(j).distance(pos) < PRESSURE_RADIUS
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
        state.players[actor].touch_ready_tick = state.tick + RETOUCH_TICKS;
        state.holder_ticks = 0;
    }

    /// Pass to teammate `to`. Accuracy from passing/technique/vision (keepers:
    /// distribution); error grows with pressure and distance.
    pub fn resolve_pass(state: &mut MatchState, frame: &TickFrame, actor: usize, to: usize) {
        let mut rng = state.next_rng(actor);
        let p = state.players[actor];
        let from = frame.ball(state).xy();
        let target = frame.pos(to);
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
        let max_angle = 0.22 - 0.18 * skill + 0.08 * pressure;
        let angle_err = max_angle * tri(&mut rng);
        let len_err = 1.0 + (0.15 - 0.12 * skill) * tri(&mut rng);
        let dir = rotate((target - from).normalize(), angle_err);
        let aim_d = (d * len_err).max(1.0);

        let v = if d > 28.0 {
            // Lofted: hang time grows with distance.
            BallFlight::lofted_velocity(dir, aim_d, 0.9 + aim_d / 30.0)
        } else {
            // Arrives at ~8 m/s: firm enough to beat interceptors, soft
            // enough to control (a 20 m pass takes ~1.8 s).
            (dir * BallFlight::rolling_speed_arriving(aim_d, PASS_ARRIVAL_SPEED)).extend(0.0)
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
        let now = state.now_ms();
        let p = state.players[actor];
        let from = frame.ball(state);
        let end = state.attacking(p.side);
        let goal = end.goal_centre();
        let dist = from.xy().distance(goal);
        let shot_skill = if dist > 20.0 {
            0.6 * unit(p.attrs.technical.long_shots)
        } else {
            0.6 * unit(p.attrs.technical.finishing)
        } + 0.2 * unit(p.attrs.mental.composure)
            + 0.2 * unit(p.attrs.technical.technique);
        let pressure = if penalty {
            0.0
        } else {
            Self::pressure_on(state, frame, actor)
        };

        // On target: better finishers and closer shots; pressure hurts.
        let p_on = if penalty {
            0.8 + 0.12 * shot_skill
        } else {
            (0.25 + 0.5 * shot_skill - dist / 60.0 - 0.1 * pressure).clamp(0.05, 0.85)
        };
        let on_target = rng.chance(p_on);

        let defending = p.side.other();
        let keeper = state.keeper(defending);
        let outcome = if on_target {
            let p_save = keeper.map_or(0.0, |k| {
                let g = &state.players[k].attrs.goalkeeping;
                // One-on-ones matter more close in; reflexes always.
                let close = if dist < 12.0 { 0.25 } else { 0.1 };
                let gk = 0.4 * unit(g.reflexes)
                    + 0.3 * unit(g.positioning_gk)
                    + 0.15 * unit(g.handling)
                    + close * unit(g.one_on_ones);
                if penalty {
                    (0.12 + 0.2 * gk - 0.1 * shot_skill).clamp(0.05, 0.35)
                } else {
                    // ~70% of on-target shots are saved in real football.
                    (0.5 + 0.4 * gk - 0.3 * shot_skill + dist / 80.0).clamp(0.15, 0.92)
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
        let speed = 20.0 + 8.0 * shot_skill;
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

    /// Tackle on the carrier `on`: foul, clean win, ball knocked loose, or
    /// beaten.
    pub fn resolve_tackle(state: &mut MatchState, frame: &TickFrame, actor: usize, on: usize) {
        let mut rng = state.next_rng(actor);
        let tackler = state.players[actor];
        let carrier = state.players[on];
        let t = &tackler.attrs;
        let c = &carrier.attrs;
        // Rationale: tackling vs dribbling decides most duels; base ~35-45%.
        let p_win = (0.35
            + 0.35
                * (0.6 * unit(t.technical.tackling)
                    + 0.2 * unit(t.mental.anticipation)
                    + 0.2 * unit(t.physical.strength))
            - 0.3
                * (0.6 * unit(c.technical.dribbling)
                    + 0.2 * unit(c.physical.agility)
                    + 0.2 * unit(c.physical.balance)))
        .clamp(0.1, 0.75);
        // Aggressive, clumsy tacklers foul more (~2-12% of challenges;
        // ~10-14 fouls per team per match overall).
        let p_foul = (0.03 + 0.08 * unit(t.mental.aggression) - 0.03 * unit(t.technical.tackling))
            .clamp(0.02, 0.15);
        // Defenders are far more careful inside their own box (penalty risk;
        // ~0.3 penalties per match in real football).
        let own_box = state.attacking(tackler.side).opposite();
        let in_box = pitch::in_penalty_area(frame.ball(state).xy(), own_box);
        let p_foul = if in_box { p_foul * 0.05 } else { p_foul };
        // A booked player goes in much more carefully (avoids a second yellow).
        let p_foul = if tackler.yellow_cards > 0 {
            p_foul * 0.3
        } else {
            p_foul
        };
        let roll = rng.next_f32();
        state.players[actor].tackle_ready_tick = state.tick + TACKLE_COOLDOWN;
        let tick = state.tick;
        let team = state.team_mut(tackler.side);
        team.tackles += 1;
        team.next_tackle_tick = tick + TEAM_TACKLE_GAP;

        if roll < p_foul {
            Self::foul(state, frame, actor, on, &mut rng);
        } else if roll < p_foul + p_win {
            if rng.chance(0.6) {
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
            state.players[actor].tackle_ready_tick = state.tick + BEATEN_COOLDOWN;
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
        let aggression = unit(tackler.attrs.mental.aggression);
        let card = if rng.chance(0.004) {
            Some(CardKind::Red)
        } else if rng.chance(0.07 + 0.13 * aggression) {
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
        let speed = vel.length();
        let height = frame.ball(state).z;
        let is_keeper = p.role == Role::Goalkeeper;
        // Control odds: first touch & technique; fast or high balls are harder.
        let touch = if is_keeper {
            unit(p.attrs.goalkeeping.handling)
        } else {
            0.6 * unit(p.attrs.technical.first_touch) + 0.4 * unit(p.attrs.technical.technique)
        };
        // Cutting out an opponent's pass is a reaction, not a prepared touch.
        let intercepting = matches!(intent, FlightIntent::Pass { receiver }
            if state.players[usize::from(receiver)].side != p.side);
        let p_control = (0.6 + 0.37 * touch
            - 0.015 * (speed - 12.0).max(0.0)
            - if height > 1.0 && !is_keeper { 0.2 } else { 0.0 }
            - if intercepting { 0.25 } else { 0.0 })
        .clamp(0.15, 0.97);

        if rng.chance(p_control) {
            if let FlightIntent::Pass { receiver } = intent {
                if state.players[usize::from(receiver)].side == p.side {
                    state.team_mut(p.side).passes_completed += 1;
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
                (KEEPER_REACH, 2.6)
            } else {
                match pass {
                    Some((r, _)) if r == i => (RECEIVER_RADIUS, 1.8),
                    Some((_, side)) if side != p.side => (INTERCEPT_RADIUS, 1.8),
                    _ => (CONTROL_RADIUS, 1.8),
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
