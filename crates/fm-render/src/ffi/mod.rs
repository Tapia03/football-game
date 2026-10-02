//! All production `unsafe` of the workspace lives under this module (spec
//! Section 0.13). Children: `sab` (`SharedArrayBuffer`, Phase 6),
//! `glow_backend` (WebGL2 via glow) and `wasm_shims` (JS glue). The test-only
//! allocation counter lives in the `fm-test-utils` crate.
#![allow(unsafe_code)]

#[cfg(target_arch = "wasm32")]
pub mod glow_backend;
pub mod sab;
pub mod wasm_shims;
