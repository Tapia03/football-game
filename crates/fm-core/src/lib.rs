//! Core primitives shared by every crate: math (libm), geometry, pitch and RNG.
//!
//! Every transcendental function in the engine must go through [`math`]
//! (pure-Rust `libm`, no platform libm) so that native `cargo test` and the
//! WASM build produce bit-identical results. `clippy.toml` enforces it.

#![forbid(unsafe_code)]

pub mod geometry;
pub mod libm_golden;
pub mod math;
pub mod pitch;
pub mod rng;

pub use geometry::{Vec2, Vec3};
pub use pitch::GoalEnd;
pub use rng::{rng_for_event, splitmix64, Rng};

/// Pi computed at runtime through `libm::atan`, not the `std` constant.
///
/// Using a libm call (instead of `core::f64::consts::PI`) makes the hello page
/// a real end-to-end proof that libm is linked and executes inside WASM.
#[must_use]
pub fn pi_libm() -> f64 {
    4.0 * libm::atan(1.0)
}

#[cfg(test)]
mod tests {
    use super::pi_libm;

    #[test]
    fn pi_libm_matches_std_constant_bit_for_bit() {
        assert_eq!(pi_libm().to_bits(), core::f64::consts::PI.to_bits());
    }
}
