//! Team instructions that shape where players stand (Phase 3 subset).
//! Discrete levels (not floats) keep tactics hashable and exactly comparable.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Mentality {
    Defensive,
    Cautious,
    #[default]
    Balanced,
    Positive,
    Attacking,
}

impl Mentality {
    /// Whole-block depth shift, as a fraction of pitch length.
    /// ±0.08 ≈ ±8.4 m: Defensive→Attacking moves the block ~17 m (criterion 19
    /// needs ≥ 5 m), leaving room for the other modifiers.
    #[must_use]
    pub const fn depth_shift(self) -> f32 {
        match self {
            Self::Defensive => -0.08,
            Self::Cautious => -0.04,
            Self::Balanced => 0.0,
            Self::Positive => 0.04,
            Self::Attacking => 0.08,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Width {
    Narrow,
    #[default]
    Normal,
    Wide,
}

impl Width {
    /// Lateral stretch applied in possession (fraction of the formation's width).
    #[must_use]
    pub const fn factor(self) -> f32 {
        match self {
            Self::Narrow => 0.75,
            Self::Normal => 0.9,
            Self::Wide => 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LineHeight {
    Deep,
    #[default]
    Normal,
    High,
}

impl LineHeight {
    /// Extra depth for the defensive line (≈ ±5 m); midfield follows at half.
    #[must_use]
    pub const fn depth_shift(self) -> f32 {
        match self {
            Self::Deep => -0.05,
            Self::Normal => 0.0,
            Self::High => 0.05,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Tactics {
    pub mentality: Mentality,
    pub width: Width,
    pub line_height: LineHeight,
}
