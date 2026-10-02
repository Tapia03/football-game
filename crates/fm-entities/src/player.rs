//! Player identity, cold (`PlayerStatic`) and hot (`PlayerDynamic`) data.

use crate::attributes::PlayerAttributes;

/// The only key to a player. Index into the `PlayerDatabase` columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlayerId(pub u32);

impl PlayerId {
    #[inline]
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Club key (clubs live in `fm-world`; only the id is needed here).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClubId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Position {
    Goalkeeper,
    CentreBack,
    LeftBack,
    RightBack,
    DefensiveMidfielder,
    CentralMidfielder,
    LeftMidfielder,
    RightMidfielder,
    AttackingMidfielder,
    LeftWinger,
    RightWinger,
    Striker,
}

impl Position {
    pub const ALL: [Self; 12] = [
        Self::Goalkeeper,
        Self::CentreBack,
        Self::LeftBack,
        Self::RightBack,
        Self::DefensiveMidfielder,
        Self::CentralMidfielder,
        Self::LeftMidfielder,
        Self::RightMidfielder,
        Self::AttackingMidfielder,
        Self::LeftWinger,
        Self::RightWinger,
        Self::Striker,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Foot {
    Left,
    Right,
    Both,
}

/// Biographical data. Names are indices into name tables (no `String`), so the
/// struct stays POD and the database never allocates per player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlayerBio {
    pub first_name: u16,
    pub last_name: u16,
    pub nationality: u16,
    pub birth_year: u16,
    /// 0..52, week of the year the player was born.
    pub birth_week: u8,
    pub height_cm: u8,
    pub weight_kg: u8,
    pub foot: Foot,
}

impl PlayerBio {
    /// Age in whole years at (`year`, `week`).
    #[must_use]
    pub fn age_at(&self, year: u16, week: u8) -> u16 {
        let years = year.saturating_sub(self.birth_year);
        if week < self.birth_week {
            years.saturating_sub(1)
        } else {
            years
        }
    }
}

/// Cold data: read by matches and the market, rarely written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlayerStatic {
    pub attributes: PlayerAttributes,
    pub bio: PlayerBio,
    pub position: Position,
    /// Ceiling for development, 1..=100.
    pub potential: u8,
    pub club: Option<ClubId>,
}

/// Fixed-point scale for dynamic state: 0 ..= `BP_MAX` (basis points).
/// Integers instead of floats make `weekly_update` trivially bit-identical on
/// every target and keep the struct small.
pub const BP_MAX: u16 = 10_000;
/// Neutral value for form and morale.
pub const BP_NEUTRAL: u16 = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum InjuryKind {
    #[default]
    None,
    Knock,
    Strain,
    Tear,
    Major,
}

/// Hot data, touched every week for every player. Must stay ≤ 24 bytes
/// (spec 3.I): 500k × 24 B = 12 MB streams through cache in one pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlayerDynamic {
    /// Accumulated tiredness; 0 = fresh.
    pub fatigue: u16,
    /// Match fitness; `BP_MAX` = fully fit.
    pub condition: u16,
    pub form: u16,
    pub morale: u16,
    /// Minutes played since the last `weekly_update` (input, reset weekly).
    pub minutes_this_week: u16,
    /// Weeks left out injured; 0 = available.
    pub injury_weeks: u8,
    pub injury_kind: InjuryKind,
    /// Consecutive weeks fit but without minutes (drives morale).
    pub weeks_unused: u8,
}

const _: () = assert!(core::mem::size_of::<PlayerDynamic>() <= 24);

impl Default for PlayerDynamic {
    fn default() -> Self {
        Self {
            fatigue: 0,
            condition: BP_MAX,
            form: BP_NEUTRAL,
            morale: BP_NEUTRAL,
            minutes_this_week: 0,
            injury_weeks: 0,
            injury_kind: InjuryKind::None,
            weeks_unused: 0,
        }
    }
}

impl PlayerDynamic {
    #[inline]
    #[must_use]
    pub const fn is_injured(&self) -> bool {
        self.injury_weeks > 0
    }

    /// True when every field respects its documented range.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.fatigue <= BP_MAX
            && self.condition <= BP_MAX
            && self.form <= BP_MAX
            && self.morale <= BP_MAX
            && (self.injury_weeks == 0) == (self.injury_kind == InjuryKind::None)
    }
}

#[cfg(test)]
mod tests {
    use super::{Foot, PlayerBio, PlayerDynamic};

    #[test]
    fn dynamic_fits_budget() {
        assert!(core::mem::size_of::<PlayerDynamic>() <= 24);
        assert!(PlayerDynamic::default().is_valid());
    }

    #[test]
    fn age_respects_birth_week() {
        let bio = PlayerBio {
            first_name: 0,
            last_name: 0,
            nationality: 0,
            birth_year: 2000,
            birth_week: 20,
            height_cm: 180,
            weight_kg: 75,
            foot: Foot::Right,
        };
        assert_eq!(bio.age_at(2026, 19), 25);
        assert_eq!(bio.age_at(2026, 20), 26);
        assert_eq!(bio.age_at(1990, 0), 0);
    }
}
