//! Core primitives shared by every crate: math (libm), geometry and RNG.
//!
//! Phase 0 ships only the libm golden-vector machinery. Every transcendental
//! function in the engine must go through `libm` (pure Rust, no platform libm)
//! so that native `cargo test` and the WASM build produce bit-identical results.

pub mod libm_golden;

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
