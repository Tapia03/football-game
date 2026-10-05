//! Team instructions that shape where players stand (Phase 3 subset).
//! Discrete levels (not floats) keep tactics hashable and exactly comparable;
//! their numeric effect lives in `anchor::AnchorTuning`.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Mentality {
    Defensive,
    Cautious,
    #[default]
    Balanced,
    Positive,
    Attacking,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Width {
    Narrow,
    #[default]
    Normal,
    Wide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LineHeight {
    Deep,
    #[default]
    Normal,
    High,
}

/// How tightly the team closes down the ball carrier outside its own box
/// (spec Fase 6, 6C). Only `Medium` is calibrated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Pressing {
    Low,
    #[default]
    Medium,
    High,
    UltraHigh,
}

impl Pressing {
    /// Index into the per-level tuning tables, Low..=UltraHigh.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Low => 0,
            Self::Medium => 1,
            Self::High => 2,
            Self::UltraHigh => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Tactics {
    pub mentality: Mentality,
    pub width: Width,
    pub line_height: LineHeight,
    pub pressing: Pressing,
}
