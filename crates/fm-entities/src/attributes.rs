//! Player attributes: four POD blocks, every value on a 1..=100 scale.
//!
//! `u8` fields keep a full attribute set at 41 bytes, so 500k players fit in
//! ~20 MB of cold data and copy without allocation.

/// Lowest and highest legal attribute value.
pub const ATTR_MIN: u8 = 1;
pub const ATTR_MAX: u8 = 100;

macro_rules! attr_block {
    ($(#[$m:meta])* $name:ident { $($field:ident),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name {
            $(pub $field: u8),+
        }

        impl $name {
            /// Field names in declaration order (stable, used by generators/UI).
            pub const FIELDS: &'static [&'static str] = &[$(stringify!($field)),+];

            /// Every field set to `v` (clamped to the legal range).
            #[must_use]
            pub const fn uniform(v: u8) -> Self {
                let v = if v < ATTR_MIN { ATTR_MIN } else if v > ATTR_MAX { ATTR_MAX } else { v };
                Self { $($field: v),+ }
            }

            /// Builds the block from values in `FIELDS` order.
            #[must_use]
            pub fn from_fn(mut f: impl FnMut(usize) -> u8) -> Self {
                let values: [u8; Self::FIELDS.len()] =
                    core::array::from_fn(|i| f(i).clamp(ATTR_MIN, ATTR_MAX));
                let [$($field),+] = values;
                Self { $($field),+ }
            }

            /// Values in `FIELDS` order.
            #[must_use]
            pub const fn to_array(&self) -> [u8; Self::FIELDS.len()] {
                [$(self.$field),+]
            }

            /// True when every value is in `ATTR_MIN..=ATTR_MAX`.
            #[must_use]
            pub fn is_valid(&self) -> bool {
                self.to_array().iter().all(|v| (ATTR_MIN..=ATTR_MAX).contains(v))
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::uniform(50)
            }
        }
    };
}

attr_block!(
    /// Ball skills (10).
    TechnicalAttributes {
        passing, crossing, dribbling, finishing, first_touch,
        heading, long_shots, tackling, technique, set_pieces,
    }
);

attr_block!(
    /// Decision making and personality on the pitch (9).
    MentalAttributes {
        aggression, anticipation, composure, concentration, decisions,
        determination, off_the_ball, positioning, vision,
    }
);

attr_block!(
    /// Body (8).
    PhysicalAttributes {
        acceleration, agility, balance, jumping, natural_fitness,
        pace, stamina, strength,
    }
);

attr_block!(
    /// Never shown to the user; drive variance, injuries and development (8).
    HiddenAttributes {
        consistency, important_matches, injury_proneness, versatility,
        adaptability, ambition, professionalism, temperament,
    }
);

attr_block!(
    /// Goalkeeping (6). Always present so `PlayerStatic` keeps a fixed size,
    /// but only consulted when the player's role is goalkeeper.
    GoalkeepingAttributes {
        reflexes, handling, positioning_gk, aerial_reach, one_on_ones, distribution,
    }
);

const _: () = assert!(core::mem::size_of::<GoalkeepingAttributes>() == 6);

/// The full attribute set of a player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PlayerAttributes {
    pub technical: TechnicalAttributes,
    pub mental: MentalAttributes,
    pub physical: PhysicalAttributes,
    pub hidden: HiddenAttributes,
    pub goalkeeping: GoalkeepingAttributes,
}

impl PlayerAttributes {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.technical.is_valid()
            && self.mental.is_valid()
            && self.physical.is_valid()
            && self.hidden.is_valid()
            && self.goalkeeping.is_valid()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        GoalkeepingAttributes, HiddenAttributes, MentalAttributes, PhysicalAttributes,
        PlayerAttributes, TechnicalAttributes,
    };
    use core::mem::size_of;

    #[test]
    fn block_sizes_match_spec() {
        assert_eq!(TechnicalAttributes::FIELDS.len(), 10);
        assert_eq!(MentalAttributes::FIELDS.len(), 9);
        assert_eq!(PhysicalAttributes::FIELDS.len(), 8);
        assert_eq!(HiddenAttributes::FIELDS.len(), 8);
        assert_eq!(size_of::<TechnicalAttributes>(), 10);
        assert_eq!(size_of::<MentalAttributes>(), 9);
        assert_eq!(size_of::<PhysicalAttributes>(), 8);
        assert_eq!(size_of::<HiddenAttributes>(), 8);
        assert_eq!(GoalkeepingAttributes::FIELDS.len(), 6);
        assert_eq!(size_of::<GoalkeepingAttributes>(), 6);
        assert_eq!(size_of::<PlayerAttributes>(), 41);
    }

    #[test]
    fn values_are_clamped() {
        assert!(TechnicalAttributes::uniform(0).is_valid());
        assert_eq!(TechnicalAttributes::uniform(255).passing, 100);
        let m = MentalAttributes::from_fn(|i| if i == 0 { 0 } else { 200 });
        assert_eq!(m.aggression, 1);
        assert_eq!(m.vision, 100);
        assert!(PlayerAttributes::default().is_valid());
    }

    #[test]
    fn from_fn_follows_field_order() {
        let p = PhysicalAttributes::from_fn(|i| u8::try_from(i + 1).unwrap());
        assert_eq!(p.to_array(), [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(p.stamina, 7);
    }
}
