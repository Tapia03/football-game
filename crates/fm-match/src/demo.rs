//! Deterministic demo line-ups for tests, benchmarks, the parity golden and
//! (later) the browser demo.

use fm_entities::{generate_database, PlayerDatabase, PlayerId, Position};

use crate::engine::{MatchSetup, TeamSheet};
use crate::formation::{Formation, Role};
use crate::tactics::Tactics;
use crate::tuning::TuningParams;

fn fits(role: Role, pos: Position) -> bool {
    use Position as P;
    match role {
        Role::Goalkeeper => pos == P::Goalkeeper,
        Role::CentreBack => pos == P::CentreBack,
        Role::FullBack | Role::WingBack => matches!(pos, P::LeftBack | P::RightBack),
        Role::DefensiveMidfielder => matches!(pos, P::DefensiveMidfielder | P::CentralMidfielder),
        Role::CentralMidfielder => matches!(pos, P::CentralMidfielder | P::AttackingMidfielder),
        Role::WideMidfielder => matches!(pos, P::LeftMidfielder | P::RightMidfielder),
        Role::AttackingMidfielder => pos == P::AttackingMidfielder,
        Role::Winger => matches!(pos, P::LeftWinger | P::RightWinger),
        Role::Striker => pos == P::Striker,
    }
}

fn pick(db: &PlayerDatabase, formation: Formation, used: &mut Vec<PlayerId>) -> [PlayerId; 11] {
    let mut out = [PlayerId(0); 11];
    for (slot, s) in formation.slots().iter().enumerate() {
        let want_gk = s.role == Role::Goalkeeper;
        let id = db
            .ids()
            .find(|id| !used.contains(id) && fits(s.role, db.static_of(*id).position))
            .or_else(|| {
                // Fallback: any unused player of the right kind (GK / outfield).
                db.ids().find(|id| {
                    !used.contains(id)
                        && (db.static_of(*id).position == Position::Goalkeeper) == want_gk
                })
            })
            .expect("demo database has enough players");
        used.push(id);
        out[slot] = id;
    }
    out
}

/// A 4-4-2 vs 4-3-3 match between two squads drawn from a generated pool.
#[must_use]
pub fn demo_match(seed: u64) -> (PlayerDatabase, MatchSetup) {
    let db = generate_database(300, seed, 2026);
    let mut used = Vec::new();
    let home = pick(&db, Formation::F442, &mut used);
    let away = pick(&db, Formation::F433, &mut used);
    let setup = MatchSetup {
        match_seed: seed,
        home: TeamSheet {
            formation: Formation::F442,
            tactics: Tactics::default(),
            players: home,
        },
        away: TeamSheet {
            formation: Formation::F433,
            tactics: Tactics::default(),
            players: away,
        },
        tuning: TuningParams::default(),
    };
    (db, setup)
}
