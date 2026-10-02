//! Match engine: logical tick, analytic kinematics, decisions, action resolver, snapshots. Implemented in Phases 3-6.
//!
//! Phase 0 only wires the crate into the workspace so the dependency graph
//! and CI are in place before any logic lands.

/// Crate name, used by the workspace smoke test to prove the crate links.
pub const CRATE_NAME: &str = "fm-match";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_links_against_core() {
        assert_eq!(super::CRATE_NAME, "fm-match");
        assert!(fm_core::pi_libm() > 3.0);
    }
}
