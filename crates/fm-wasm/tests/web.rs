//! Runs inside a real browser via `wasm-pack test --headless --chrome`.
#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn hello_in_wasm() {
    assert_eq!(
        fm_wasm::hello(),
        "Hello from Rust (libm: 3.141592653589793)"
    );
}

#[wasm_bindgen_test]
fn libm_parity_with_native_golden() {
    if let Some((line, expected, actual)) = fm_core::libm_golden::first_mismatch() {
        panic!("WASM libm diverges from native golden at line {line}: expected `{expected}`, got `{actual}`");
    }
}

#[wasm_bindgen_test]
fn rng_for_event_matches_native_golden() {
    // Same constants as fm-core `rng::tests::GOLDEN_3_7` (computed natively).
    let mut r = fm_core::rng_for_event(0x1234_5678_9ABC_DEF0, 3, 7);
    let got: Vec<u64> = (0..4).map(|_| r.next_u64()).collect();
    assert_eq!(
        got,
        [
            0xE278_19E3_B95B_C0C3,
            0xAE2B_B331_92F8_3AF7,
            0xC8AC_F881_11D2_6C24,
            0x8AA5_2FF8_3857_FDA4,
        ]
    );
}

#[wasm_bindgen_test]
fn geometry_bit_identical_to_native() {
    // angle_to/from_angle go through libm; pinned bits come from native.
    let a = fm_core::Vec2::new(3.0, 4.0);
    let b = fm_core::Vec2::from_angle(1.0);
    assert_eq!(a.angle_to(b).to_bits(), ANGLE_BITS);
    assert_eq!(a.normalize().x.to_bits(), 0.6_f32.to_bits());
}

/// `Vec2::new(3, 4).angle_to(Vec2::from_angle(1.0))` computed natively.
const ANGLE_BITS: u32 = 0x3D94_E640;

#[wasm_bindgen_test]
fn weekly_update_matches_native_digest() {
    // Same scenario as the native digest test in fm-entities.
    let mut db = fm_entities::generate_database(5_000, 12, 2026);
    for w in 0..30 {
        for i in (0..5_000).step_by(2) {
            db.record_minutes(fm_entities::PlayerId(i), 90);
        }
        db.weekly_update(99, w);
    }
    assert_eq!(fm_entities::dynamics_digest(&db), 0x71E5_90E3_714D_2DD4);
}

#[wasm_bindgen_test]
fn webgl2_smoke_draws_expected_pixel() {
    let px = fm_wasm::webgl2_smoke().expect("WebGL2 smoke failed");
    assert_eq!(px, fm_wasm::webgl2_smoke_expected());
}
