//! 2D rendering (spec Section 3.H): the FFI boundary with the WebGL2 smoke
//! check and the mesh renderer, and the safe draw list (`shapes`). The
//! `Renderer2D` trait is finalised later in Phase 6.
//!
//! All production `unsafe` of the workspace lives under [`ffi`]
//! (`fm-render/src/ffi/`); everything else in this crate is safe code. The
//! only other `unsafe` is test-only, in `fm-test-utils` (spec §0.13).

pub mod ffi;

pub mod shapes;
