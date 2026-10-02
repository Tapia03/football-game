//! Analytic ball flight (spec 3.G, v2.1 model).
//!
//! At kick time the whole path — flight, bounces, roll, rest — is solved once
//! into a fixed array of closed-form segments. `pos_at`/`vel_at` pick the
//! segment for the query time: pure, allocation-free, and identical however
//! often it is sampled.
//!
//! Model: horizontal motion with linear drag (air) or linear damping (roll);
//! vertical motion is a drag-free parabola so the bounce instant is the root
//! of a quadratic (no iteration). Magnus is deferred (spec 3.G).

use fm_core::{math, Vec2, Vec3};

/// Gravity, m/s².
pub const GRAVITY: f32 = 9.81;
/// Horizontal air drag coefficient (1/s). Linear stand-in for quadratic
/// drag: a 25 m/s shot loses ~10% of its speed over 0.5 s.
pub const AIR_DRAG: f32 = 0.2;
/// Rolling damping (1/s): a 10 m/s pass decelerates ~3.5 m/s² at first and
/// covers ~28 m before stopping.
pub const ROLL_DAMPING: f32 = 0.35;
/// Vertical coefficient of restitution on grass.
pub const RESTITUTION: f32 = 0.55;
/// Horizontal speed kept through a bounce (grass friction at impact).
pub const BOUNCE_FRICTION: f32 = 0.8;
/// Below this vertical speed after a bounce the ball just rolls (m/s).
pub const MIN_BOUNCE_VZ: f32 = 1.0;
/// Below this ground speed the ball is considered at rest (m/s).
pub const STOP_SPEED: f32 = 0.3;

const MAX_SEGMENTS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Segment {
    /// Airborne from `p0` with horizontal velocity `vh` and vertical `vz`.
    Air { p0: Vec3, vh: Vec2, vz: f32 },
    /// Rolling on the ground from `p0` with velocity `v`.
    Roll { p0: Vec2, v: Vec2 },
    /// At rest.
    Rest { p: Vec2 },
}

/// A complete ball path from one kick, times in seconds after the kick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BallFlight {
    pub kick_ms: u32,
    segs: [Segment; MAX_SEGMENTS],
    /// Start time (s after kick) of each segment; the last used one is Rest.
    starts: [f32; MAX_SEGMENTS],
    len: usize,
}

/// `(1 - e^(-c·τ)) / c`: displacement factor of linear damping.
fn damped(c: f32, tau: f32) -> f32 {
    (1.0 - math::exp(-c * tau)) / c
}

impl BallFlight {
    /// Solves the full path of a ball kicked at `kick_ms` from `p0` with
    /// velocity `v0`. A ball below ground level is lifted to `z = 0`.
    #[must_use]
    pub fn kick(kick_ms: u32, p0: Vec3, v0: Vec3) -> Self {
        let mut segs = [Segment::Rest { p: p0.xy() }; MAX_SEGMENTS];
        let mut starts = [0.0_f32; MAX_SEGMENTS];
        let mut len = 0;
        let mut t = 0.0_f32;
        let mut p = Vec3::new(p0.x, p0.y, p0.z.max(0.0));
        let mut vh = v0.xy();
        let mut vz = v0.z;

        while len < MAX_SEGMENTS - 1 {
            starts[len] = t;
            if p.z > 0.0 || vz > MIN_BOUNCE_VZ {
                segs[len] = Segment::Air { p0: p, vh, vz };
                len += 1;
                // z(τ) = z0 + vz·τ − g·τ²/2 = 0, positive root.
                let tau = (vz + math::sqrt(vz * vz + 2.0 * GRAVITY * p.z)) / GRAVITY;
                let xy = p.xy() + vh * damped(AIR_DRAG, tau);
                let horizontal_at_impact = vh * math::exp(-AIR_DRAG * tau);
                let vertical_at_impact = vz - GRAVITY * tau;
                t += tau;
                p = Vec3::new(xy.x, xy.y, 0.0);
                vh = horizontal_at_impact * BOUNCE_FRICTION;
                vz = -vertical_at_impact * RESTITUTION;
            } else {
                let speed = vh.length();
                if speed <= STOP_SPEED {
                    break;
                }
                segs[len] = Segment::Roll { p0: p.xy(), v: vh };
                len += 1;
                // speed·e^(−μτ) = STOP_SPEED.
                let tau = math::ln(speed / STOP_SPEED) / ROLL_DAMPING;
                let xy = p.xy() + vh * damped(ROLL_DAMPING, tau);
                t += tau;
                p = Vec3::new(xy.x, xy.y, 0.0);
                vh = Vec2::ZERO;
                vz = 0.0;
            }
        }
        starts[len] = t;
        segs[len] = Segment::Rest { p: p.xy() };
        len += 1;
        Self {
            kick_ms,
            segs,
            starts,
            len,
        }
    }

    fn locate(&self, t_ms: u32) -> (usize, f32) {
        #[allow(clippy::cast_precision_loss)] // ms since kick < 2^24
        let tau = t_ms.saturating_sub(self.kick_ms) as f32 / 1000.0;
        let mut i = 0;
        while i + 1 < self.len && tau >= self.starts[i + 1] {
            i += 1;
        }
        (i, tau - self.starts[i])
    }

    #[must_use]
    pub fn pos_at(&self, t_ms: u32) -> Vec3 {
        let (i, tau) = self.locate(t_ms);
        match self.segs[i] {
            Segment::Air { p0, vh, vz } => {
                let xy = p0.xy() + vh * damped(AIR_DRAG, tau);
                Vec3::new(
                    xy.x,
                    xy.y,
                    (p0.z + vz * tau - 0.5 * GRAVITY * tau * tau).max(0.0),
                )
            }
            Segment::Roll { p0, v } => (p0 + v * damped(ROLL_DAMPING, tau)).extend(0.0),
            Segment::Rest { p } => p.extend(0.0),
        }
    }

    #[must_use]
    pub fn vel_at(&self, t_ms: u32) -> Vec3 {
        let (i, tau) = self.locate(t_ms);
        match self.segs[i] {
            Segment::Air { vh, vz, .. } => {
                (vh * math::exp(-AIR_DRAG * tau)).extend(vz - GRAVITY * tau)
            }
            Segment::Roll { v, .. } => (v * math::exp(-ROLL_DAMPING * tau)).extend(0.0),
            Segment::Rest { .. } => Vec3::ZERO,
        }
    }

    /// True once the ball has come to rest.
    #[must_use]
    pub fn at_rest(&self, t_ms: u32) -> bool {
        matches!(self.segs[self.locate(t_ms).0], Segment::Rest { .. })
    }

    /// Launch velocity that lands a lofted ball `d` metres away along `dir`
    /// (unit) after `flight_s` seconds, from ground level.
    #[must_use]
    pub fn lofted_velocity(dir: Vec2, d: f32, flight_s: f32) -> Vec3 {
        let vh = d / damped(AIR_DRAG, flight_s);
        (dir * vh).extend(0.5 * GRAVITY * flight_s)
    }

    /// Launch velocity reaching `target` (with height) exactly `flight_s`
    /// seconds after leaving `from`, assuming no bounce on the way.
    #[must_use]
    pub fn aimed_velocity(from: Vec3, target: Vec3, flight_s: f32) -> Vec3 {
        let dxy = target.xy() - from.xy();
        let vh = dxy / damped(AIR_DRAG, flight_s);
        let vz = (target.z - from.z + 0.5 * GRAVITY * flight_s * flight_s) / flight_s;
        vh.extend(vz)
    }

    /// Ground speed that makes a rolling pass stop `d` metres away.
    #[must_use]
    pub fn rolling_speed_for(d: f32) -> f32 {
        // Total roll distance ≈ v0/μ (minus the tiny STOP_SPEED tail).
        d * ROLL_DAMPING + STOP_SPEED
    }

    /// Ground speed that makes a rolling ball reach `d` metres away still
    /// moving at `arrive` m/s (linear damping: v(x) = v0 − μ·x).
    #[must_use]
    pub fn rolling_speed_arriving(d: f32, arrive: f32) -> f32 {
        arrive + d * ROLL_DAMPING
    }
}

#[cfg(test)]
mod tests {
    use super::{BallFlight, GRAVITY};
    use fm_core::{Vec2, Vec3};

    #[test]
    fn rolling_pass_stops_near_requested_distance() {
        for d in [5.0_f32, 15.0, 30.0] {
            let v = BallFlight::rolling_speed_for(d);
            let f = BallFlight::kick(0, Vec3::ZERO, Vec3::new(v, 0.0, 0.0));
            let end = f.pos_at(60_000);
            assert!((end.x - d).abs() < 1.0, "d={d} end={end:?}");
            assert!(f.at_rest(60_000));
            assert_eq!(end.z.to_bits(), 0.0_f32.to_bits());
        }
    }

    #[test]
    fn lofted_ball_lands_on_target_then_bounces_and_rolls() {
        let dir = Vec2::new(0.0, 1.0);
        let v = BallFlight::lofted_velocity(dir, 40.0, 2.0);
        let f = BallFlight::kick(1_000, Vec3::ZERO, v);
        let land = f.pos_at(3_000);
        assert!(land.z < 1e-3 && (land.y - 40.0).abs() < 0.05, "{land:?}");
        let apex = f.pos_at(2_000);
        assert!((apex.z - GRAVITY * 0.5).abs() < 0.05, "apex {apex:?}");
        // After landing it keeps going (bounce + roll) and eventually stops.
        let rest = f.pos_at(120_000);
        assert!(rest.y > 45.0 && f.at_rest(120_000));
        // Never under the ground, never faster than at launch.
        for ms in (1_000..30_000).step_by(37) {
            let p = f.pos_at(ms);
            assert!(p.z >= 0.0);
            assert!(f.vel_at(ms).xy().length() <= v.xy().length() + 1e-3);
        }
    }

    #[test]
    fn aimed_shot_passes_through_target() {
        let from = Vec3::new(80.0, 30.0, 0.0);
        let target = Vec3::new(105.0, 35.0, 1.5);
        let v = BallFlight::aimed_velocity(from, target, 0.9);
        let f = BallFlight::kick(500, from, v);
        let at = f.pos_at(1_400);
        assert!(
            at.xy().distance(target.xy()) < 0.05 && (at.z - 1.5).abs() < 0.05,
            "{at:?}"
        );
    }

    #[test]
    fn pass_arrives_with_requested_speed() {
        let d = 20.0;
        let v0 = BallFlight::rolling_speed_arriving(d, 8.0);
        let f = BallFlight::kick(0, Vec3::ZERO, Vec3::new(v0, 0.0, 0.0));
        // Find the first ms the ball passes d and check speed there.
        let ms = (0..10_000)
            .find(|&ms| f.pos_at(ms).x >= d)
            .expect("reaches target");
        let v = f.vel_at(ms).x;
        assert!((v - 8.0).abs() < 0.1, "arrival speed {v}");
        assert!(ms < 2_000, "a 20 m pass takes {ms} ms");
    }

    #[test]
    fn continuous_across_segments() {
        let f = BallFlight::kick(0, Vec3::new(0.0, 0.0, 1.0), Vec3::new(12.0, 3.0, 6.0));
        let mut prev = f.pos_at(0);
        for ms in 1..20_000 {
            let p = f.pos_at(ms);
            assert!(
                (p - prev).length() < 0.05,
                "jump at {ms}: {prev:?} -> {p:?}"
            );
            prev = p;
        }
    }

    #[test]
    fn pure_function_of_time() {
        let f = BallFlight::kick(0, Vec3::ZERO, Vec3::new(20.0, -4.0, 8.0));
        let a: Vec<u32> = (0..500).map(|i| f.pos_at(i * 20).x.to_bits()).collect();
        let b: Vec<u32> = (0..500)
            .rev()
            .map(|i| f.pos_at(i * 20).x.to_bits())
            .collect();
        assert_eq!(a, b.into_iter().rev().collect::<Vec<_>>());
        assert_eq!(f.pos_at(0), Vec3::ZERO);
    }
}
