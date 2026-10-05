//! Squads: how good a player is for the screens (`overall`), which
//! positions can play a role, and the automatic line-up (spec Fase 7B).
//! None of this reaches the match engine except the eleven ids it is given.

use fm_entities::{PlayerDatabase, PlayerId, PlayerStatic, Position};
use fm_match::{Formation, Role};

/// Weighted mean of `(value, weight)` pairs, rounded to nearest.
fn weighted(pairs: &[(u8, u32)]) -> u8 {
    let (sum, weight) = pairs.iter().fold((0_u32, 0_u32), |(s, w), &(v, k)| {
        (s + u32::from(v) * k, w + k)
    });
    u8::try_from((sum + weight / 2) / weight.max(1)).unwrap_or(u8::MAX)
}

/// One number for the screens: the attributes that matter for the player's
/// position, weighted. The match engine never reads it.
#[must_use]
pub fn overall(player: &PlayerStatic) -> u8 {
    use Position as P;
    let (t, m, p, g) = (
        &player.attributes.technical,
        &player.attributes.mental,
        &player.attributes.physical,
        &player.attributes.goalkeeping,
    );
    match player.position {
        P::Goalkeeper => weighted(&[
            (g.reflexes, 3),
            (g.handling, 3),
            (g.positioning_gk, 3),
            (g.aerial_reach, 2),
            (g.one_on_ones, 2),
            (g.distribution, 1),
            (m.concentration, 1),
        ]),
        P::CentreBack | P::LeftBack | P::RightBack => weighted(&[
            (t.tackling, 3),
            (m.positioning, 3),
            (t.heading, 2),
            (p.strength, 2),
            (m.anticipation, 2),
            (m.concentration, 2),
            (p.pace, 1),
            (t.passing, 1),
        ]),
        P::DefensiveMidfielder
        | P::CentralMidfielder
        | P::LeftMidfielder
        | P::RightMidfielder
        | P::AttackingMidfielder => weighted(&[
            (t.passing, 3),
            (m.vision, 3),
            (t.first_touch, 2),
            (m.decisions, 2),
            (t.technique, 2),
            (p.stamina, 2),
            (t.tackling, 1),
            (m.off_the_ball, 1),
        ]),
        P::LeftWinger | P::RightWinger | P::Striker => weighted(&[
            (t.finishing, 3),
            (m.off_the_ball, 3),
            (t.dribbling, 2),
            (p.pace, 2),
            (m.composure, 2),
            (t.first_touch, 2),
            (t.technique, 1),
            (t.heading, 1),
        ]),
    }
}

/// Whether a player of position `pos` is at home in `role` (the same table
/// the engine's demo line-ups use).
#[must_use]
pub fn fits(role: Role, pos: Position) -> bool {
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

/// The best eleven of `squad` for `formation`: for each slot, the best
/// player (by `overall`, lowest id on ties) not yet used who fits the role
/// and is `available`; failing that, anyone available of the right kind
/// (keeper or outfielder); failing that, anyone of the right kind at all —
/// a team always fields eleven.
///
/// # Panics
/// When the squad has no keeper or fewer than ten outfielders.
#[must_use]
pub fn pick_lineup(
    players: &PlayerDatabase,
    squad: &[PlayerId],
    formation: Formation,
    available: impl Fn(PlayerId) -> bool,
) -> [PlayerId; 11] {
    let mut used: Vec<PlayerId> = Vec::with_capacity(11);
    let mut out = [PlayerId(0); 11];
    for (slot, s) in formation.slots().iter().enumerate() {
        let keeper = s.role == Role::Goalkeeper;
        let right_kind =
            |id: PlayerId| (players.static_of(id).position == Position::Goalkeeper) == keeper;
        let best = |accept: &dyn Fn(PlayerId) -> bool| {
            squad
                .iter()
                .copied()
                .filter(|&id| !used.contains(&id) && accept(id))
                // Best overall first; the lowest id breaks ties.
                .min_by_key(|&id| (u8::MAX - overall(players.static_of(id)), id))
        };
        let id = best(&|id| available(id) && fits(s.role, players.static_of(id).position))
            .or_else(|| best(&|id| available(id) && right_kind(id)))
            .or_else(|| best(&right_kind))
            .expect("a squad has a keeper and ten outfielders");
        used.push(id);
        out[slot] = id;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_entities::generate_database;

    #[test]
    fn overall_is_a_valid_rating_that_follows_the_attributes() {
        let db = generate_database(500, 3, 2026);
        for s in db.statics() {
            assert!((1..=100).contains(&overall(s)), "{}", overall(s));
        }
        // Better at what the position needs: a better rating.
        let mut striker = *db.static_of(PlayerId(0));
        striker.position = Position::Striker;
        let before = overall(&striker);
        striker.attributes.technical.finishing = 100;
        striker.attributes.mental.off_the_ball = 100;
        assert!(overall(&striker) > before);
        // What the position does not use does not move it.
        let rated = overall(&striker);
        striker.attributes.goalkeeping.reflexes = 1;
        assert_eq!(overall(&striker), rated);
    }

    #[test]
    fn every_role_of_every_formation_has_a_position_that_fits() {
        for formation in Formation::ALL {
            for slot in formation.slots() {
                assert!(
                    Position::ALL.iter().any(|&p| fits(slot.role, p)),
                    "{:?}",
                    slot.role
                );
            }
        }
    }
}
