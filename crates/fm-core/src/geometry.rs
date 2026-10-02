//! 2D/3D vectors in metres. All length/angle math goes through `crate::math`.

use core::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub, SubAssign};

use crate::math;

/// Below this length a vector has no meaningful direction.
const DIRECTION_EPS: f32 = 1e-6;

/// Point or displacement on the pitch plane (metres).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

/// Point or displacement in 3D (metres); `z` is height above the pitch.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self::new(0.0, 0.0);
    pub const X: Self = Self::new(1.0, 0.0);
    pub const Y: Self = Self::new(0.0, 1.0);

    #[inline]
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Unit vector pointing at `angle` radians (counter-clockwise from +x).
    #[inline]
    #[must_use]
    pub fn from_angle(angle: f32) -> Self {
        Self::new(math::cos(angle), math::sin(angle))
    }

    #[inline]
    #[must_use]
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y
    }

    /// z component of the 3D cross product; > 0 when `rhs` is counter-clockwise.
    #[inline]
    #[must_use]
    pub fn cross(self, rhs: Self) -> f32 {
        self.x * rhs.y - self.y * rhs.x
    }

    #[inline]
    #[must_use]
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    #[inline]
    #[must_use]
    pub fn length(self) -> f32 {
        math::sqrt(self.length_squared())
    }

    #[inline]
    #[must_use]
    pub fn distance(self, rhs: Self) -> f32 {
        (rhs - self).length()
    }

    /// Unit vector in the same direction, or `ZERO` for a (near-)zero vector,
    /// so callers never propagate NaN from a degenerate direction.
    #[inline]
    #[must_use]
    pub fn normalize(self) -> Self {
        let len = self.length();
        if len < DIRECTION_EPS {
            Self::ZERO
        } else {
            self / len
        }
    }

    /// Component-wise clamp into the box `[min, max]`.
    #[inline]
    #[must_use]
    pub fn clamp(self, min: Self, max: Self) -> Self {
        Self::new(self.x.clamp(min.x, max.x), self.y.clamp(min.y, max.y))
    }

    /// Same direction, length capped at `max_len`.
    #[inline]
    #[must_use]
    pub fn clamp_length(self, max_len: f32) -> Self {
        let len = self.length();
        if len > max_len && len >= DIRECTION_EPS {
            self * (max_len / len)
        } else {
            self
        }
    }

    /// `self + (rhs - self) * t`; `t` is not clamped.
    #[inline]
    #[must_use]
    pub fn lerp(self, rhs: Self, t: f32) -> Self {
        self + (rhs - self) * t
    }

    /// Angle of this vector in radians, `[-PI, PI]`.
    #[inline]
    #[must_use]
    pub fn angle(self) -> f32 {
        math::atan2(self.y, self.x)
    }

    /// Signed angle in radians rotating `self` onto `rhs`, `[-PI, PI]`
    /// (positive = counter-clockwise).
    #[inline]
    #[must_use]
    pub fn angle_to(self, rhs: Self) -> f32 {
        math::atan2(self.cross(rhs), self.dot(rhs))
    }

    /// Lifts to 3D at height `z`.
    #[inline]
    #[must_use]
    pub const fn extend(self, z: f32) -> Vec3 {
        Vec3::new(self.x, self.y, z)
    }
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);

    #[inline]
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    #[inline]
    #[must_use]
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    #[inline]
    #[must_use]
    pub fn cross(self, rhs: Self) -> Self {
        Self::new(
            self.y * rhs.z - self.z * rhs.y,
            self.z * rhs.x - self.x * rhs.z,
            self.x * rhs.y - self.y * rhs.x,
        )
    }

    #[inline]
    #[must_use]
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    #[inline]
    #[must_use]
    pub fn length(self) -> f32 {
        math::sqrt(self.length_squared())
    }

    /// Unit vector, or `ZERO` for a (near-)zero vector.
    #[inline]
    #[must_use]
    pub fn normalize(self) -> Self {
        let len = self.length();
        if len < DIRECTION_EPS {
            Self::ZERO
        } else {
            self / len
        }
    }

    /// Component-wise clamp into the box `[min, max]`.
    #[inline]
    #[must_use]
    pub fn clamp(self, min: Self, max: Self) -> Self {
        Self::new(
            self.x.clamp(min.x, max.x),
            self.y.clamp(min.y, max.y),
            self.z.clamp(min.z, max.z),
        )
    }

    /// Same direction, length capped at `max_len`.
    #[inline]
    #[must_use]
    pub fn clamp_length(self, max_len: f32) -> Self {
        let len = self.length();
        if len > max_len && len >= DIRECTION_EPS {
            self * (max_len / len)
        } else {
            self
        }
    }

    #[inline]
    #[must_use]
    pub fn lerp(self, rhs: Self, t: f32) -> Self {
        self + (rhs - self) * t
    }

    /// Unsigned angle between the two vectors in radians, `[0, PI]`.
    #[inline]
    #[must_use]
    pub fn angle_to(self, rhs: Self) -> f32 {
        math::atan2(self.cross(rhs).length(), self.dot(rhs))
    }

    /// Projection on the pitch plane (drops height).
    #[inline]
    #[must_use]
    pub const fn xy(self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }
}

macro_rules! impl_ops {
    ($t:ident { $($f:ident),+ }) => {
        impl Add for $t {
            type Output = Self;
            #[inline]
            fn add(self, rhs: Self) -> Self { Self { $($f: self.$f + rhs.$f),+ } }
        }
        impl Sub for $t {
            type Output = Self;
            #[inline]
            fn sub(self, rhs: Self) -> Self { Self { $($f: self.$f - rhs.$f),+ } }
        }
        impl Mul<f32> for $t {
            type Output = Self;
            #[inline]
            fn mul(self, rhs: f32) -> Self { Self { $($f: self.$f * rhs),+ } }
        }
        impl Mul<$t> for f32 {
            type Output = $t;
            #[inline]
            fn mul(self, rhs: $t) -> $t { rhs * self }
        }
        impl Div<f32> for $t {
            type Output = Self;
            #[inline]
            fn div(self, rhs: f32) -> Self { Self { $($f: self.$f / rhs),+ } }
        }
        impl Neg for $t {
            type Output = Self;
            #[inline]
            fn neg(self) -> Self { Self { $($f: -self.$f),+ } }
        }
        impl AddAssign for $t {
            #[inline]
            fn add_assign(&mut self, rhs: Self) { *self = *self + rhs; }
        }
        impl SubAssign for $t {
            #[inline]
            fn sub_assign(&mut self, rhs: Self) { *self = *self - rhs; }
        }
        impl MulAssign<f32> for $t {
            #[inline]
            fn mul_assign(&mut self, rhs: f32) { *self = *self * rhs; }
        }
    };
}

impl_ops!(Vec2 { x, y });
impl_ops!(Vec3 { x, y, z });

#[cfg(test)]
mod tests {
    use super::{Vec2, Vec3};
    use crate::math::PI;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    fn close2(a: Vec2, b: Vec2) -> bool {
        close(a.x, b.x) && close(a.y, b.y)
    }

    #[test]
    fn vec2_operators() {
        let a = Vec2::new(1.0, 2.0);
        let b = Vec2::new(3.0, -4.0);
        assert_eq!(a + b, Vec2::new(4.0, -2.0));
        assert_eq!(a - b, Vec2::new(-2.0, 6.0));
        assert_eq!(a * 2.0, Vec2::new(2.0, 4.0));
        assert_eq!(2.0 * a, Vec2::new(2.0, 4.0));
        assert_eq!(b / 2.0, Vec2::new(1.5, -2.0));
        assert_eq!(-a, Vec2::new(-1.0, -2.0));
        let mut c = a;
        c += b;
        c -= a;
        c *= 3.0;
        assert_eq!(c, Vec2::new(9.0, -12.0));
    }

    #[test]
    fn vec2_products_and_length() {
        let a = Vec2::new(3.0, 4.0);
        assert!(close(a.length(), 5.0));
        assert!(close(a.dot(Vec2::new(1.0, 0.0)), 3.0));
        assert!(close(Vec2::X.cross(Vec2::Y), 1.0));
        assert!(close(Vec2::Y.cross(Vec2::X), -1.0));
        assert!(close(Vec2::ZERO.distance(a), 5.0));
    }

    #[test]
    fn vec2_normalize_handles_zero() {
        assert!(close(Vec2::new(3.0, 4.0).normalize().length(), 1.0));
        assert_eq!(Vec2::ZERO.normalize(), Vec2::ZERO);
        assert_eq!(Vec2::new(1e-9, 0.0).normalize(), Vec2::ZERO);
    }

    #[test]
    fn vec2_clamp_and_lerp() {
        let p = Vec2::new(-5.0, 80.0).clamp(Vec2::ZERO, Vec2::new(105.0, 68.0));
        assert_eq!(p, Vec2::new(0.0, 68.0));
        assert!(close(Vec2::new(30.0, 40.0).clamp_length(5.0).length(), 5.0));
        assert_eq!(Vec2::new(1.0, 1.0).clamp_length(5.0), Vec2::new(1.0, 1.0));
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(10.0, -10.0);
        assert_eq!(a.lerp(b, 0.0), a);
        assert_eq!(a.lerp(b, 1.0), b);
        assert_eq!(a.lerp(b, 0.5), Vec2::new(5.0, -5.0));
    }

    #[test]
    fn vec2_angles() {
        assert!(close(Vec2::X.angle_to(Vec2::Y), PI / 2.0));
        assert!(close(Vec2::Y.angle_to(Vec2::X), -PI / 2.0));
        assert!(close(Vec2::X.angle_to(-Vec2::X).abs(), PI));
        assert!(close2(Vec2::from_angle(0.0), Vec2::X));
        assert!(close2(Vec2::from_angle(PI / 2.0), Vec2::Y));
        for i in -30_i8..=30 {
            let ang = f32::from(i) * 0.1;
            assert!(close(Vec2::from_angle(ang).length(), 1.0));
            assert!(close(Vec2::from_angle(ang).angle(), ang));
        }
    }

    #[test]
    fn vec3_operations() {
        let a = Vec3::new(1.0, 0.0, 0.0);
        let b = Vec3::new(0.0, 1.0, 0.0);
        assert_eq!(a.cross(b), Vec3::Z);
        assert_eq!(b.cross(a), -Vec3::Z);
        assert!(close(a.dot(b), 0.0));
        assert!(close(Vec3::new(2.0, 3.0, 6.0).length(), 7.0));
        assert!(close(Vec3::new(2.0, 3.0, 6.0).normalize().length(), 1.0));
        assert_eq!(Vec3::ZERO.normalize(), Vec3::ZERO);
        assert!(close(a.angle_to(b), PI / 2.0));
        assert_eq!(a.lerp(b, 0.5), Vec3::new(0.5, 0.5, 0.0));
        assert_eq!(
            Vec3::new(5.0, -1.0, 9.0).clamp(Vec3::ZERO, Vec3::new(4.0, 4.0, 4.0)),
            Vec3::new(4.0, 0.0, 4.0)
        );
        assert!(close(
            Vec3::new(0.0, 30.0, 40.0).clamp_length(5.0).length(),
            5.0
        ));
        assert_eq!(Vec3::new(1.0, 2.0, 3.0).xy(), Vec2::new(1.0, 2.0));
        assert_eq!(Vec2::new(1.0, 2.0).extend(3.0), Vec3::new(1.0, 2.0, 3.0));
        let mut c = Vec3::new(1.0, 1.0, 1.0);
        c += Vec3::Z;
        c -= Vec3::new(1.0, 0.0, 0.0);
        c *= 2.0;
        assert_eq!(c, Vec3::new(0.0, 2.0, 4.0));
        assert_eq!(c / 2.0 + Vec3::Z - Vec3::Z, Vec3::new(0.0, 1.0, 2.0));
    }
}
