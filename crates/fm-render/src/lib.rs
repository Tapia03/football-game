//! 2D rendering (spec Section 3.H). Phase 3 only lands the FFI boundary and a
//! WebGL2 smoke check; the `Renderer2D` trait and the glow backend arrive in
//! Phase 6.
//!
//! This is the only crate allowed to contain `unsafe`, and only under
//! [`ffi`]. Everything else here is safe code.

pub mod ffi;
