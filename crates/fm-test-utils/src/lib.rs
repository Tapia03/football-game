//! Test-only utilities, used exclusively as a `dev-dependency`.
//!
//! The only `unsafe` here is the `GlobalAlloc` impl in [`alloc_counter`],
//! compiled only with the `alloc-counter` feature. Everything else is safe.

#[cfg(feature = "alloc-counter")]
#[allow(unsafe_code)] // GlobalAlloc is an unsafe trait; isolated to this module.
pub mod alloc_counter;
