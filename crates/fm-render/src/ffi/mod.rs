//! Every `unsafe` block of the workspace lives under this module (spec
//! Section 6). Children: `sab` (`SharedArrayBuffer`, Phase 6),
//! `glow_backend` (WebGL2 via glow), `wasm_shims` (JS glue), and
//! `alloc_counter` (allocation-counting global allocator used by tests).
#![allow(unsafe_code)]

pub mod alloc_counter;
#[cfg(target_arch = "wasm32")]
pub mod glow_backend;
pub mod sab;
pub mod wasm_shims;
