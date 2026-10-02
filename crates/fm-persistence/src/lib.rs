//! Save serialization and versioning. Implemented in Phase 9.
//!
//! Phase 0 only wires the crate into the workspace so the dependency graph
//! and CI are in place before any logic lands.

#![forbid(unsafe_code)]

/// Crate name, used by the workspace smoke test to prove the crate links.
pub const CRATE_NAME: &str = "fm-persistence";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_links_against_core() {
        assert_eq!(super::CRATE_NAME, "fm-persistence");
        assert!(fm_core::pi_libm() > 3.0);
    }
}
