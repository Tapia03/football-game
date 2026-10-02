//! Market value and transfer AI. Implemented in Phase 11.
//!
//! Phase 0 only wires the crate into the workspace so the dependency graph
//! and CI are in place before any logic lands.

#![forbid(unsafe_code)]

/// Crate name, used by the workspace smoke test to prove the crate links.
pub const CRATE_NAME: &str = "fm-economy";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_links_against_core() {
        assert_eq!(super::CRATE_NAME, "fm-economy");
        assert!(fm_core::pi_libm() > 3.0);
    }
}
