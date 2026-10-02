//! Deterministic synthetic player generation (bootstrap, tests, benchmarks).

use fm_core::{rng_for_event, Rng};

use crate::attributes::{
    GoalkeepingAttributes, HiddenAttributes, MentalAttributes, PhysicalAttributes,
    PlayerAttributes, TechnicalAttributes,
};
use crate::database::PlayerDatabase;
use crate::player::{Foot, PlayerBio, PlayerStatic, Position};

const GENERATE_DOMAIN: u64 = 0x4745_4E45_5241_5445; // "GENERATE"

/// Relative frequency of each position in a squad (sums to 100).
const POSITION_WEIGHTS: [(Position, u32); 12] = [
    (Position::Goalkeeper, 9),
    (Position::CentreBack, 18),
    (Position::LeftBack, 8),
    (Position::RightBack, 8),
    (Position::DefensiveMidfielder, 8),
    (Position::CentralMidfielder, 12),
    (Position::LeftMidfielder, 5),
    (Position::RightMidfielder, 5),
    (Position::AttackingMidfielder, 7),
    (Position::LeftWinger, 6),
    (Position::RightWinger, 6),
    (Position::Striker, 8),
];

#[derive(Clone, Copy)]
enum Group {
    Keeper,
    Defender,
    Midfielder,
    Attacker,
}

impl Group {
    fn of(p: Position) -> Self {
        match p {
            Position::Goalkeeper => Self::Keeper,
            Position::CentreBack | Position::LeftBack | Position::RightBack => Self::Defender,
            Position::DefensiveMidfielder
            | Position::CentralMidfielder
            | Position::LeftMidfielder
            | Position::RightMidfielder
            | Position::AttackingMidfielder => Self::Midfielder,
            Position::LeftWinger | Position::RightWinger | Position::Striker => Self::Attacker,
        }
    }

    /// Offsets from the player's base quality, in `FIELDS` order per block.
    /// They shape role profiles (defenders tackle, attackers finish).
    const fn profile(self) -> (&'static [i8; 10], &'static [i8; 9], &'static [i8; 8]) {
        match self {
            Self::Keeper => (
                &[-10, -25, -25, -30, -10, -15, -25, -20, -15, -15],
                &[-5, 10, 5, 10, 5, 0, -20, 15, -5],
                &[-10, 10, 0, 5, 0, -15, -10, 0],
            ),
            Self::Defender => (
                &[-5, -5, -10, -20, -5, 10, -15, 15, -5, -10],
                &[5, 5, 0, 5, 0, 0, -10, 10, -5],
                &[-5, -5, 0, 10, 0, -5, 0, 10],
            ),
            Self::Midfielder => (
                &[10, 0, 5, -5, 5, -5, 5, 0, 5, 0],
                &[0, 0, 5, 0, 5, 0, 0, 0, 10],
                &[0, 0, 0, -5, 0, 0, 10, -5],
            ),
            Self::Attacker => (
                &[0, 0, 10, 15, 5, 5, 5, -20, 5, 0],
                &[0, 5, 5, -5, 0, 0, 15, -10, 5],
                &[10, 5, 0, 0, 0, 10, -5, 0],
            ),
        }
    }
}

fn pick_position(rng: &mut Rng) -> Position {
    let mut roll = rng.below(100);
    for (pos, w) in POSITION_WEIGHTS {
        if roll < w {
            return pos;
        }
        roll -= w;
    }
    Position::Striker
}

/// `base + offset ± 12`, clamped to 1..=100 by the attribute block.
fn attr(rng: &mut Rng, base: i32, offset: i8) -> u8 {
    #[allow(clippy::cast_possible_wrap)] // < 25
    let jitter = rng.below(25) as i32 - 12;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped
    let v = (base + i32::from(offset) + jitter).clamp(1, 100) as u8;
    v
}

#[allow(clippy::cast_possible_truncation)] // every draw below is < 256 (or < 65536 for u16)
fn generate_player(rng: &mut Rng, season_year: u16) -> PlayerStatic {
    let position = pick_position(rng);
    let group = Group::of(position);
    let group_is_keeper = matches!(group, Group::Keeper);
    // Triangular quality around 50 (20..=80): most players are average.
    let quality_u = 20 + rng.below(31) + rng.below(31);
    #[allow(clippy::cast_possible_wrap)] // <= 80
    let quality = quality_u as i32;
    let (t, m, p) = group.profile();

    let attributes = PlayerAttributes {
        technical: TechnicalAttributes::from_fn(|i| attr(rng, quality, t[i])),
        mental: MentalAttributes::from_fn(|i| attr(rng, quality, m[i])),
        physical: PhysicalAttributes::from_fn(|i| attr(rng, quality, p[i])),
        // Hidden traits are independent of ability.
        hidden: HiddenAttributes::from_fn(|_| 10 + rng.below(81) as u8),
        // Placeholder; drawn last (end of this fn) so adding the block did
        // not shift the draws of any pre-existing field.
        goalkeeping: GoalkeepingAttributes::uniform(1),
    };

    let age = 16 + rng.below(21) as u16; // 16..=36
                                         // Younger players have more headroom above current quality.
    let headroom = 30_u32.saturating_sub(u32::from(age - 16) * 2).max(1);
    let potential = (quality_u + rng.below(headroom)).min(100) as u8;

    let tall = matches!(position, Position::Goalkeeper | Position::CentreBack);
    let height_cm = 168 + rng.below(25) as u8 + if tall { 8 } else { 0 };
    let weight_kg = height_cm - 110 + rng.below(12) as u8;
    let left_sided = matches!(
        position,
        Position::LeftBack | Position::LeftMidfielder | Position::LeftWinger
    );
    let foot = match rng.below(100) {
        r if r < if left_sided { 30 } else { 75 } => Foot::Right,
        r if r < 95 => Foot::Left,
        _ => Foot::Both,
    };

    let mut player = PlayerStatic {
        attributes,
        bio: PlayerBio {
            first_name: rng.below(2_000) as u16,
            last_name: rng.below(5_000) as u16,
            nationality: rng.below(200) as u16,
            birth_year: season_year - age,
            birth_week: rng.below(52) as u8,
            height_cm,
            weight_kg,
            foot,
        },
        position,
        potential,
        club: None,
    };
    // Keepers get their quality in the GK block; outfielders are poor
    // emergency keepers (5..=30). Drawn after every other field.
    player.attributes.goalkeeping = if group_is_keeper {
        GoalkeepingAttributes::from_fn(|_| attr(rng, quality, 0))
    } else {
        GoalkeepingAttributes::from_fn(|_| 5 + rng.below(26) as u8)
    };
    player
}

/// Builds `count` players. Player `i` depends only on `(seed, i)`, so any
/// prefix of a larger generation is identical to a smaller one.
///
/// # Panics
/// Never for `count <= u32::MAX` (ids are `u32`).
#[must_use]
pub fn generate_database(count: u32, seed: u64, season_year: u16) -> PlayerDatabase {
    let mut db = PlayerDatabase::with_capacity(count as usize);
    for i in 0..count {
        let mut rng = rng_for_event(seed ^ GENERATE_DOMAIN, i, 0);
        db.create(generate_player(&mut rng, season_year));
    }
    db
}

#[cfg(test)]
mod tests {
    use super::generate_database;
    use crate::player::{PlayerId, Position};
    use std::collections::HashSet;

    #[test]
    fn generation_is_deterministic_and_prefix_stable() {
        let a = generate_database(1_000, 42, 2026);
        assert_eq!(a, generate_database(1_000, 42, 2026));
        assert_ne!(a, generate_database(1_000, 43, 2026));
        let small = generate_database(10, 42, 2026);
        assert_eq!(small.statics(), &a.statics()[..10]);
    }

    #[test]
    fn generated_players_are_valid_and_varied() {
        let db = generate_database(10_000, 1, 2026);
        let mut positions = HashSet::new();
        for s in db.statics() {
            assert!(s.attributes.is_valid());
            assert!((1..=100).contains(&s.potential));
            let age = s.bio.age_at(2026, 52);
            assert!((15..=36).contains(&age), "age {age}");
            positions.insert(s.position);
        }
        assert_eq!(positions.len(), Position::ALL.len());
    }

    #[test]
    fn profiles_shape_roles() {
        let db = generate_database(20_000, 2, 2026);
        let mean = |pos: Position, f: fn(&crate::PlayerStatic) -> u8| {
            let v: Vec<u32> = db
                .statics()
                .iter()
                .filter(|s| s.position == pos)
                .map(|s| u32::from(f(s)))
                .collect();
            v.iter().sum::<u32>() / u32::try_from(v.len()).unwrap()
        };
        let finishing = |s: &crate::PlayerStatic| s.attributes.technical.finishing;
        let tackling = |s: &crate::PlayerStatic| s.attributes.technical.tackling;
        assert!(mean(Position::Striker, finishing) > mean(Position::CentreBack, finishing) + 20);
        assert!(mean(Position::CentreBack, tackling) > mean(Position::Striker, tackling) + 20);
        let reflexes = |s: &crate::PlayerStatic| s.attributes.goalkeeping.reflexes;
        assert!(mean(Position::Goalkeeper, reflexes) > mean(Position::Striker, reflexes) + 25);
        let _ = db.static_of(PlayerId(0));
    }
}
