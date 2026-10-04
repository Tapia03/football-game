//! Analytic player movement (spec 3.A.2, Fase 5 "Física de movimento"):
//! each planned move has up to three phases, all in closed form —
//! reaction (keep the previous velocity), constant acceleration toward the
//! wanted velocity, and a straight cruise to the target at constant speed.
//! `pos_at` is a pure function of the trajectory and the query time, so
//! sampling at 60 Hz, 10 Hz or never gives identical logical state.
//!
//! With `max_accel = ∞`, `reaction_ms = 0` and `turn_rate = ∞` the first two
//! phases last zero and the cruise is exactly the original model.

use fm_core::Vec2;

use crate::tuning::KinematicsTuning;

/// Below this distance a trajectory is "already there".
const ARRIVED_EPS: f32 = 1e-4;
/// A new target closer than this (squared, m²) to the planned one does not
/// trigger a re-plan.
const REPLAN_EPS2: f32 = 0.25;
/// Below this speed a player is standing and can face any way (m/s).
const MOVING_SPEED: f32 = 0.5;

/// Parameters of one planned movement. Match time is in integer
/// milliseconds so trajectories never accumulate float error over 90 min.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trajectory {
    pub start: Vec2,
    pub target: Vec2,
    /// Metres per second, > 0.
    pub speed: f32,
    pub t_start_ms: u32,
}

/// Movement physics of one planned move (spec Fase 5, "Física de
/// movimento"), kept apart from [`Trajectory`] so that the instant model
/// (no `Lead`) stays exactly the original code. Phases, from `t_start_ms`:
/// keep `react_v` for `react_s`; accelerate at `accel` until `cruise_s`;
/// cruise along the trajectory and brake at `brake` to stop on the target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lead {
    pub origin: Vec2,
    pub react_v: Vec2,
    pub react_s: f32,
    pub accel: Vec2,
    pub cruise_s: f32,
    /// Braking on arrival, precomputed at planning: the cruise runs at
    /// full speed for `cruise_full_s`, then decelerates at `brake` (m/s²)
    /// to stop on the target.
    pub brake: f32,
    pub cruise_full_s: f32,
    /// The intent this move was planned for: re-planning is skipped while
    /// it does not change (spec Fase 5, "Física de movimento").
    pub intent: Vec2,
    pub intent_speed: f32,
}

impl Lead {
    /// Distance covered and speed after `t` seconds of cruise at `v`.
    #[inline]
    fn cruise(&self, v: f32, t: f32) -> (f32, f32) {
        if t <= self.cruise_full_s {
            return (v * t, v);
        }
        let tau = (t - self.cruise_full_s).min(v / self.brake);
        (
            v * self.cruise_full_s + v * tau - 0.5 * self.brake * tau * tau,
            (v - self.brake * tau).max(0.0),
        )
    }

    /// Braking plan for a cruise leg of length `len` at `v`, braking at
    /// `a` (harder when the leg is too short): `(brake, full-speed time)`.
    fn braking(len: f32, v: f32, a: f32) -> (f32, f32) {
        let brake_len = v * v / (2.0 * a);
        if brake_len >= len {
            (v * v / (2.0 * len.max(1e-4)), 0.0)
        } else {
            (a, (len - brake_len) / v)
        }
    }

    #[inline]
    #[must_use]
    pub fn pos(&self, traj: &Trajectory, t_ms: u32) -> Vec2 {
        #[allow(clippy::cast_precision_loss)] // < 2^24 ms
        let e = t_ms.saturating_sub(traj.t_start_ms) as f32 / 1000.0;
        if e < self.react_s {
            return self.origin + self.react_v * e;
        }
        if e < self.cruise_s {
            let t = e - self.react_s;
            let accel_origin = self.origin + self.react_v * self.react_s;
            return accel_origin + self.react_v * t + self.accel * (0.5 * t * t);
        }
        let len = traj.start.distance(traj.target);
        if len < ARRIVED_EPS {
            return traj.target;
        }
        let (s, _) = self.cruise(traj.speed, e - self.cruise_s);
        traj.start + (traj.target - traj.start) * (s.min(len) / len)
    }

    #[must_use]
    pub fn vel(&self, traj: &Trajectory, t_ms: u32) -> Vec2 {
        #[allow(clippy::cast_precision_loss)] // < 2^24 ms
        let e = t_ms.saturating_sub(traj.t_start_ms) as f32 / 1000.0;
        if e < self.react_s {
            return self.react_v;
        }
        if e < self.cruise_s {
            return self.react_v + self.accel * (e - self.react_s);
        }
        let len = traj.start.distance(traj.target);
        if len < ARRIVED_EPS {
            return Vec2::ZERO;
        }
        let (_, v) = self.cruise(traj.speed, e - self.cruise_s);
        (traj.target - traj.start) * (v / len)
    }
}

/// Turn limit for one re-plan: cos and sin of the largest heading change.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurnLimit {
    pub cos: f32,
    pub sin: f32,
}

impl TurnLimit {
    /// Largest heading change for `turn_rate` (rad/s) over `dt_s`; `None`
    /// when unlimited.
    #[must_use]
    pub fn new(turn_rate: f32, dt_s: f32) -> Option<Self> {
        let max = turn_rate * dt_s;
        (max.is_finite() && max < core::f32::consts::PI).then(|| Self {
            cos: fm_core::math::cos(max),
            sin: fm_core::math::sin(max),
        })
    }
}

pub struct PlayerKinematics;

impl PlayerKinematics {
    /// Top running speed for a pace attribute (1..=100): 5.5 m/s (jog-level)
    /// to 9.0 m/s (elite sprint, ≈ 32 km/h).
    #[must_use]
    pub fn top_speed(pace: u8) -> f32 {
        5.5 + 0.035 * f32::from(pace.clamp(1, 100))
    }

    /// Plans a move from `from` at time `t_ms` toward `target`. A non-positive
    /// speed is treated as "stand still".
    #[must_use]
    pub fn plan_trajectory(from: Vec2, target: Vec2, speed: f32, t_ms: u32) -> Trajectory {
        let (target, speed) = if speed > 0.0 {
            (target, speed)
        } else {
            (from, 1.0)
        };
        Trajectory {
            start: from,
            target,
            speed,
            t_start_ms: t_ms,
        }
    }

    /// Re-plans from wherever `current` puts the player at `t_ms`, so a new
    /// decision never teleports anyone.
    #[must_use]
    pub fn replan(current: &Trajectory, target: Vec2, speed: f32, t_ms: u32) -> Trajectory {
        Self::plan_trajectory(Self::pos_at(current, t_ms), target, speed, t_ms)
    }

    /// True when the physics reduce to instant velocity changes (the
    /// original model): no reaction, unlimited acceleration and turning.
    #[inline]
    #[must_use]
    pub fn instant(k: &KinematicsTuning, turn: Option<TurnLimit>) -> bool {
        k.max_accel.is_infinite() && k.reaction_ms == 0 && turn.is_none()
    }

    /// Re-plans a moving player (spec Fase 5, "Física de movimento"): from
    /// `pos` toward `target` at `speed`, given its current move (`current`,
    /// `lead`), with reaction, limited acceleration and turning, and braking
    /// to stop on the target. `turn` is the per-re-plan heading limit.
    #[must_use]
    pub fn steer(
        current: (&Trajectory, Option<&Lead>),
        pos: Vec2,
        target: Vec2,
        speed: f32,
        t_ms: u32,
        k: &KinematicsTuning,
        turn: Option<TurnLimit>,
    ) -> (Trajectory, Lead) {
        let (traj, lead) = current;
        // Same intent as the move under way: keep it (it already brakes onto
        // that target).
        if let Some(l) = lead {
            if (l.intent_speed - speed).abs() < 1e-3
                && (l.intent - target).length_squared() < REPLAN_EPS2
                && (traj.target - l.intent).length_squared() < REPLAN_EPS2
            {
                return (*traj, *l);
            }
        }
        let v0 = lead.map_or_else(|| Self::vel_at(traj, t_ms), |l| l.vel(traj, t_ms));
        let (target, speed) = if speed > 0.0 {
            (target, speed)
        } else {
            (pos, 0.0)
        };
        let to = target - pos;
        let dist = to.length();
        let mut dir = if dist > ARRIVED_EPS {
            to / dist
        } else {
            Vec2::ZERO
        };
        // Limited turning from the current heading; standing: any way.
        let v0_len = v0.length();
        if let Some(lim) = turn {
            if v0_len > MOVING_SPEED && dist > ARRIVED_EPS {
                let heading = v0 / v0_len;
                if heading.dot(dir) < lim.cos {
                    let left = heading.x * dir.y - heading.y * dir.x >= 0.0;
                    let side = if left { lim.sin } else { -lim.sin };
                    let normal = Vec2::new(-heading.y, heading.x);
                    dir = heading * lim.cos + normal * side;
                }
            }
        }
        // Reaction: a new intent (target moved more than the threshold)
        // keeps the old velocity for `reaction_ms`; one in progress goes on.
        // The previous intent is where the player was sent (`Lead::intent`),
        // not the end of the cruise leg: with limited turning that leg ends
        // on the target's projection — next to the player on a sharp turn —
        // and comparing against it restarted the reaction on every re-plan.
        #[allow(clippy::cast_precision_loss)] // ms intervals ≪ 2^24
        let elapsed_s = t_ms.saturating_sub(traj.t_start_ms) as f32 / 1000.0;
        let current_react = lead.map_or(0.0, |l| l.react_s);
        #[allow(clippy::cast_precision_loss)] // < 2^24
        let react_s = if k.reaction_ms == 0 {
            0.0
        } else if current_react > elapsed_s {
            current_react - elapsed_s
        } else if lead.map_or(traj.target, |l| l.intent).distance(target) > k.reaction_threshold {
            k.reaction_ms as f32 / 1000.0
        } else {
            0.0
        };
        // Accelerate toward the wanted velocity (capped by what is left to
        // run: no point sprinting past a target one stride away).
        let accel_origin = pos + v0 * react_s;
        let wanted = speed.min(fm_core::math::sqrt(2.0 * k.max_accel * dist));
        let dv = dir * wanted - v0;
        let dv_len = dv.length();
        let (accel, accel_s) = if k.max_accel.is_finite() && dv_len > ARRIVED_EPS {
            (dv * (k.max_accel / dv_len), dv_len / k.max_accel)
        } else {
            (Vec2::ZERO, 0.0)
        };
        let start = accel_origin + v0 * accel_s + accel * (0.5 * accel_s * accel_s);
        // Cruise along the wanted direction up to the target's projection on
        // it, braking to stop there; a stand request stops where it ends up.
        let cruise = if wanted > MOVING_SPEED * 0.1 && dist > ARRIVED_EPS {
            let along = (target - start).dot(dir).max(0.0);
            Self::plan_trajectory(start, start + dir * along, wanted, t_ms)
        } else {
            Self::plan_trajectory(start, start, 0.0, t_ms)
        };
        let len = cruise.start.distance(cruise.target);
        let (brake, cruise_full_s) = if k.max_accel.is_finite() {
            Lead::braking(len, cruise.speed, k.max_accel)
        } else {
            (f32::MAX, len / cruise.speed)
        };
        (
            cruise,
            Lead {
                origin: pos,
                react_v: v0,
                react_s,
                accel,
                cruise_s: react_s + accel_s,
                brake,
                cruise_full_s,
                intent: target,
                intent_speed: speed,
            },
        )
    }

    /// Position at `t_ms` on an instant-model move. Times before the start
    /// clamp to the start.
    #[inline]
    #[must_use]
    pub fn pos_at(traj: &Trajectory, t_ms: u32) -> Vec2 {
        let dist = traj.start.distance(traj.target);
        if dist < ARRIVED_EPS {
            return traj.target;
        }
        // Integer subtraction first: exact elapsed time, then one conversion.
        #[allow(clippy::cast_precision_loss)] // < 2^24 ms ≈ 4.6 h: exact in f32
        let elapsed_s = t_ms.saturating_sub(traj.t_start_ms) as f32 / 1000.0;
        let frac = (elapsed_s * traj.speed / dist).clamp(0.0, 1.0);
        traj.start.lerp(traj.target, frac)
    }

    /// Velocity at `t_ms` on an instant-model move (zero once arrived).
    #[must_use]
    pub fn vel_at(traj: &Trajectory, t_ms: u32) -> Vec2 {
        let dist = traj.start.distance(traj.target);
        #[allow(clippy::cast_precision_loss)] // < 2^24 ms
        let elapsed_s = t_ms.saturating_sub(traj.t_start_ms) as f32 / 1000.0;
        if dist < ARRIVED_EPS || elapsed_s * traj.speed >= dist {
            return Vec2::ZERO;
        }
        (traj.target - traj.start) * (traj.speed / dist)
    }

    /// Match time at which the player reaches the target (rounded up to ms),
    /// instant model.
    #[must_use]
    pub fn arrival_ms(traj: &Trajectory) -> u32 {
        let dist = traj.start.distance(traj.target);
        let ms = (dist / traj.speed * 1000.0).ceil();
        // Distances are pitch-bounded (< 130 m) so this fits easily.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let ms = ms as u32;
        traj.t_start_ms.saturating_add(ms)
    }
}

#[cfg(test)]
mod tests {
    use super::{PlayerKinematics as K, Trajectory};
    use fm_core::Vec2;

    fn traj() -> Trajectory {
        K::plan_trajectory(Vec2::new(10.0, 10.0), Vec2::new(40.0, 50.0), 8.0, 1_000)
    }

    #[test]
    fn endpoints() {
        let t = traj();
        assert_eq!(K::pos_at(&t, 0), t.start, "before start clamps to start");
        assert_eq!(K::pos_at(&t, 1_000), t.start);
        // 50 m at 8 m/s = 6.25 s.
        assert_eq!(K::arrival_ms(&t), 7_250);
        assert_eq!(K::pos_at(&t, 7_250), t.target);
        assert_eq!(K::pos_at(&t, 60_000), t.target);
    }

    #[test]
    fn never_exceeds_speed_and_approaches_monotonically() {
        let t = traj();
        let mut prev = K::pos_at(&t, 1_000);
        for ms in (1_016..8_000).step_by(16) {
            let p = K::pos_at(&t, ms);
            assert!(prev.distance(p) <= 8.0 * 0.016 + 1e-3);
            assert!(p.distance(t.target) <= prev.distance(t.target) + 1e-5);
            prev = p;
        }
    }

    #[test]
    fn pure_function_of_time_regardless_of_sampling() {
        // Same query time, evaluated after different sampling histories
        // (60 Hz, 10 Hz, none): bit-identical results.
        let t = traj();
        let direct: Vec<u32> = (0..80)
            .map(|i| K::pos_at(&t, i * 100).x.to_bits())
            .collect();
        for step in [16_u32, 100, 7] {
            let mut sampled = Vec::new();
            let mut ms = 0;
            while ms < 8_000 {
                let p = K::pos_at(&t, ms);
                if ms % 100 == 0 {
                    sampled.push(p.x.to_bits());
                }
                ms += step;
            }
            let expect: Vec<u32> = (0_u32..)
                .zip(direct.iter().copied())
                .filter(|(i, _)| (i * 100) % step == 0)
                .map(|(_, b)| b)
                .collect();
            assert_eq!(sampled, expect, "step {step}");
        }
    }

    #[test]
    fn replan_is_continuous() {
        let t = traj();
        let r = K::replan(&t, Vec2::new(0.0, 0.0), 6.0, 3_000);
        assert_eq!(K::pos_at(&r, 3_000), K::pos_at(&t, 3_000));
        assert_eq!(r.t_start_ms, 3_000);
    }

    #[test]
    fn zero_length_and_zero_speed_stand_still() {
        let p = Vec2::new(5.0, 5.0);
        let same = K::plan_trajectory(p, p, 7.0, 0);
        assert_eq!(K::pos_at(&same, 10_000), p);
        let stopped = K::plan_trajectory(p, Vec2::new(50.0, 5.0), 0.0, 0);
        assert_eq!(K::pos_at(&stopped, 10_000), p);
        assert_eq!(K::arrival_ms(&stopped), 0);
    }

    #[test]
    fn top_speed_range() {
        assert!((K::top_speed(1) - 5.535).abs() < 1e-4);
        assert!((K::top_speed(100) - 9.0).abs() < 1e-4);
        assert!(K::top_speed(80) > K::top_speed(40));
    }

    fn physics() -> crate::tuning::KinematicsTuning {
        crate::tuning::KinematicsTuning {
            max_accel: 15.0,
            reaction_ms: 80,
            reaction_threshold: 2.0,
            turn_rate: 15.0,
        }
    }

    fn turn() -> Option<super::TurnLimit> {
        super::TurnLimit::new(physics().turn_rate, 0.1)
    }

    #[test]
    fn test_player_entering_play_does_not_jump() {
        // An off-the-play (arcade) player running east at 6 m/s enters the
        // play with a new target to the north: position and velocity carry
        // over exactly, and speed then changes at most max_accel per second.
        let arcade = K::plan_trajectory(Vec2::new(10.0, 30.0), Vec2::new(60.0, 30.0), 6.0, 0);
        let t = 2_000;
        let pos = K::pos_at(&arcade, t);
        let vel = K::vel_at(&arcade, t);
        let (traj, lead) = K::steer(
            (&arcade, None),
            pos,
            Vec2::new(pos.x, 60.0),
            8.0,
            t,
            &physics(),
            turn(),
        );
        assert_eq!(lead.pos(&traj, t), pos, "no position jump");
        assert_eq!(lead.vel(&traj, t), vel, "no velocity jump");
        let mut prev = lead.vel(&traj, t);
        for ms in (t + 10..t + 1_000).step_by(10) {
            let v = lead.vel(&traj, ms);
            assert!(
                (v - prev).length() <= 15.0 * 0.010 + 1e-3,
                "acceleration bounded at {ms} ms"
            );
            prev = v;
        }
    }

    #[test]
    fn brakes_to_stop_on_the_target() {
        let start = Vec2::new(0.0, 0.0);
        let target = Vec2::new(20.0, 0.0);
        let (traj, lead) = K::steer(
            (&K::plan_trajectory(start, start, 0.0, 0), None),
            start,
            target,
            8.0,
            0,
            &physics(),
            turn(),
        );
        let end = lead.pos(&traj, 10_000);
        assert!(end.distance(target) < 1e-3, "stops on the target: {end:?}");
        assert_eq!(lead.vel(&traj, 10_000), Vec2::ZERO);
    }

    #[test]
    fn turning_is_limited_while_moving_free_when_standing() {
        let k = physics();
        // Running east at full speed, asked to go west: it cannot turn on
        // the spot — it brakes, still moving east a moment later.
        let east = K::plan_trajectory(Vec2::new(50.0, 30.0), Vec2::new(90.0, 30.0), 8.0, 0);
        let pos = K::pos_at(&east, 1_000);
        let (traj, lead) = K::steer(
            (&east, None),
            pos,
            Vec2::new(10.0, 30.0),
            8.0,
            1_000,
            &k,
            turn(),
        );
        let v = lead.vel(&traj, 1_100);
        assert!(
            v.x > 0.0 && v.length() < 8.0,
            "braking, not reversed: {v:?}"
        );
        // Standing still, it can face the target at once.
        let stand = K::plan_trajectory(pos, pos, 0.0, 0);
        let (traj, _) = K::steer(
            (&stand, None),
            pos,
            Vec2::new(10.0, 30.0),
            8.0,
            1_000,
            &k,
            turn(),
        );
        assert!((traj.target - traj.start).normalize().x < -0.99);
    }
    /// A player sprinting one way and sent back the other way stops within a
    /// second, re-planning every tick as the engine does. (The reaction used
    /// to restart on every re-plan of a sharp turn, so he barely slowed.)
    #[test]
    fn a_sprinting_player_sent_back_stops_within_a_second() {
        let k = physics();
        let east = K::plan_trajectory(Vec2::new(50.0, 30.0), Vec2::new(90.0, 30.0), 7.0, 0);
        let back = Vec2::new(40.0, 30.0);
        let mut t = 1_000;
        let from = K::pos_at(&east, t);
        let (mut traj, mut lead) = K::steer((&east, None), from, back, 7.0, t, &k, turn());
        let mut turned_at = None;
        for tick in 1..=10 {
            t += 100;
            let pos = lead.pos(&traj, t);
            if turned_at.is_none() && lead.vel(&traj, t).x <= 0.0 {
                turned_at = Some(tick);
            }
            (traj, lead) = K::steer((&traj, Some(&lead)), pos, back, 7.0, t, &k, turn());
        }
        let pos = lead.pos(&traj, t);
        let v = lead.vel(&traj, t);
        assert!(
            turned_at.is_some_and(|tick| tick <= 10),
            "still running east after 1 s: {v:?}"
        );
        assert!(v.x < -3.0, "heading back by then: {v:?}");
        assert!(
            pos.x - from.x < 4.0,
            "overshoot while turning: {} m",
            pos.x - from.x
        );
    }
}
