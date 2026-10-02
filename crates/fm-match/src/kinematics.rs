//! Analytic player movement (spec 3.A.2): straight line at constant speed,
//! evaluated in closed form. `pos_at` is a pure function of the trajectory
//! and the query time, so sampling at 60 Hz, 10 Hz or never gives identical
//! logical state.

use fm_core::Vec2;

/// Below this distance a trajectory is "already there".
const ARRIVED_EPS: f32 = 1e-4;

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

    /// Position at `t_ms`. Times before the start clamp to the start.
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

    /// Match time at which the player reaches the target (rounded up to ms).
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
