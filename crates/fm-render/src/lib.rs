//! 2D rendering (spec Section 3.H). Phase 3 only lands the FFI boundary and a
//! WebGL2 smoke check; the `Renderer2D` trait and the glow backend arrive in
//! Phase 6.
//!
//! All production `unsafe` of the workspace lives under [`ffi`]
//! (`fm-render/src/ffi/`); everything else in this crate is safe code. The
//! only other `unsafe` is test-only, in `fm-test-utils` (spec §0.13).

pub mod ffi;

pub mod shapes;
