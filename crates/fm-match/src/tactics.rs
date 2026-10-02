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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Tactics {
    pub mentality: Mentality,
    pub width: Width,
    pub line_height: LineHeight,
}
