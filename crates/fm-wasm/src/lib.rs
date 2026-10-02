//! wasm-bindgen surface of the engine. Phase 0 exposes `hello()` and the libm
//! parity check so the browser can prove the WASM build matches native.

#![forbid(unsafe_code)]

use wasm_bindgen::prelude::wasm_bindgen;

/// Greeting shown by the bootstrap page; pi comes from libm inside WASM.
#[wasm_bindgen]
#[must_use]
pub fn hello() -> String {
    format!("Hello from Rust (libm: {})", fm_core::pi_libm())
}

/// Number of libm golden lines that differ in this build (0 = parity).
#[wasm_bindgen]
#[must_use]
pub fn libm_golden_mismatches() -> u32 {
    let actual = fm_core::libm_golden::generate();
    let golden = fm_core::libm_golden::GOLDEN;
    let differing = actual
        .lines()
        .zip(golden.lines())
        .filter(|(a, g)| a != g)
        .count();
    let missing = actual.lines().count().abs_diff(golden.lines().count());
    u32::try_from(differing + missing).unwrap_or(u32::MAX)
}

/// WebGL2 smoke test through glow: RGBA of the drawn pixel, or an error
/// message. The page compares it with `webgl2_smoke_expected()`.
///
/// # Errors
/// When WebGL2 is unavailable or the smoke shader fails.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn webgl2_smoke() -> Result<Vec<u8>, String> {
    fm_render::ffi::glow_backend::webgl2_smoke().map(|px| px.to_vec())
}

/// Pixel the smoke shader must produce.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
#[must_use]
pub fn webgl2_smoke_expected() -> Vec<u8> {
    fm_render::ffi::glow_backend::SMOKE_EXPECTED_RGBA.to_vec()
}

#[cfg(test)]
mod tests {
    #[test]
    fn hello_reports_libm_pi() {
        assert_eq!(super::hello(), "Hello from Rust (libm: 3.141592653589793)");
    }
}
