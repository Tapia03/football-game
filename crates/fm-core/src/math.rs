//! The only allowed entry point for transcendental math in the engine.
//!
//! `std`'s `f32::sin`, `f32::powf`, ... lower to the platform libm (glibc on
//! native, a JS/compiler-builtins shim on WASM) and differ by ulps between
//! targets. `libm` is pure Rust, so the same bits come out everywhere.
//! `clippy.toml` forbids the `std` versions workspace-wide.

/// Pi as f32. A constant is exact on every target, so no libm call is needed.
pub const PI: f32 = core::f32::consts::PI;
/// 2·Pi as f32.
pub const TAU: f32 = core::f32::consts::TAU;

#[inline]
#[must_use]
pub fn sin(x: f32) -> f32 {
    libm::sinf(x)
}

#[inline]
#[must_use]
pub fn cos(x: f32) -> f32 {
    libm::cosf(x)
}

#[inline]
#[must_use]
pub fn powf(base: f32, exp: f32) -> f32 {
    libm::powf(base, exp)
}

#[inline]
#[must_use]
pub fn sqrt(x: f32) -> f32 {
    libm::sqrtf(x)
}

#[inline]
#[must_use]
pub fn exp(x: f32) -> f32 {
    libm::expf(x)
}

/// Natural logarithm.
#[inline]
#[must_use]
pub fn ln(x: f32) -> f32 {
    libm::logf(x)
}

/// Angle of `(x, y)` in radians, range `[-PI, PI]`.
#[inline]
#[must_use]
pub fn atan2(y: f32, x: f32) -> f32 {
    libm::atan2f(y, x)
}

#[cfg(test)]
mod tests {
    use super::{atan2, cos, exp, ln, powf, sin, sqrt, PI};

    #[test]
    fn wrappers_delegate_to_libm_bit_for_bit() {
        for i in -50_i16..=50 {
            let x = f32::from(i) * 0.37;
            let p = x.abs() + 0.5;
            assert_eq!(sin(x).to_bits(), libm::sinf(x).to_bits());
            assert_eq!(cos(x).to_bits(), libm::cosf(x).to_bits());
            assert_eq!(exp(x * 0.1).to_bits(), libm::expf(x * 0.1).to_bits());
            assert_eq!(sqrt(p).to_bits(), libm::sqrtf(p).to_bits());
            assert_eq!(ln(p).to_bits(), libm::logf(p).to_bits());
            assert_eq!(powf(p, x * 0.1).to_bits(), libm::powf(p, x * 0.1).to_bits());
            assert_eq!(atan2(x, p).to_bits(), libm::atan2f(x, p).to_bits());
        }
    }

    #[test]
    fn sanity_values() {
        assert!(sin(0.0).abs() < 1e-7);
        assert!((cos(PI) + 1.0).abs() < 1e-6);
        assert!((sqrt(2.0) * sqrt(2.0) - 2.0).abs() < 1e-6);
        assert!((ln(exp(1.5)) - 1.5).abs() < 1e-6);
        assert!((powf(2.0, 10.0) - 1024.0).abs() < 1e-3);
    }
}
