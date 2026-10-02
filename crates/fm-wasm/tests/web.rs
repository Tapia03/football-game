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
