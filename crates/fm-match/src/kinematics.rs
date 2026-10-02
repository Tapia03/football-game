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
    /// Braking deceleration on arrival (m/s²).
    pub brake: f32,
}

impl Lead {
    /// Distance covered after `t` seconds of cruise over a leg of length
    /// `len` at `v`, braking at `a` to stop at its end; and the speed then.
    #[inline]
    fn cruise(len: f32, v: f32, a: f32, t: f32) -> (f32, f32) {
        // Brake harder when the leg is too short to brake at `a`.
        let brake_len = v * v / (2.0 * a);
        let (cruise_len, a) = if brake_len >= len {
            (0.0, v * v / (2.0 * len))
        } else {
            (len - brake_len, a)
        };
        let t1 = cruise_len / v;
        if t <= t1 {
            return (v * t, v);
        }
        let tau = (t - t1).min(v / a);
        (
            cruise_len + v * tau - 0.5 * a * tau * tau,
            (v - a * tau).max(0.0),
        )
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
        let (s, _) = Self::cruise(len, traj.speed, self.brake, e - self.cruise_s);
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
        let (_, v) = Self::cruise(len, traj.speed, self.brake, e - self.cruise_s);
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
        #[allow(clippy::cast_precision_loss)] // ms intervals ≪ 2^24
        let elapsed_s = t_ms.saturating_sub(traj.t_start_ms) as f32 / 1000.0;
        let current_react = lead.map_or(0.0, |l| l.react_s);
        #[allow(clippy::cast_precision_loss)] // < 2^24
        let react_s = if k.reaction_ms == 0 {
            0.0
        } else if current_react > elapsed_s {
            current_react - elapsed_s
        } else if traj.target.distance(target) > k.reaction_threshold {
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
        (
            cruise,
            Lead {
                origin: pos,
                react_v: v0,
                react_s,
                accel,
                cruise_s: react_s + accel_s,
                brake: if k.max_accel.is_finite() {
                    k.max_accel
                } else {
                    f32::MAX
                },
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
}
