//! Snapshots and level of detail (spec 3.A.3, 3.D).
//!
//! LOD only decides how often the consumer samples; it never reaches
//! `tick_logic`, `choose_action` or `resolve`. Sampling is `&self`: it cannot
//! change the match.

use fm_core::{Vec2, Vec3};

use crate::phase::{Phase, Side};
use crate::state::PLAYERS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LodLevel {
    /// 60 snapshots per second, rendered.
    Full,
    /// 10 snapshots per second (one per logical tick), not rendered.
    Reduced,
    /// No snapshots: events only.
    Abstract,
}

impl LodLevel {
    /// Sample offsets (ms) inside each 100 ms logical tick. Full = 60 Hz,
    /// rounded to whole milliseconds.
    #[must_use]
    pub const fn sample_offsets_ms(self) -> &'static [u32] {
        match self {
            Self::Full => &[0, 17, 33, 50, 67, 83],
            Self::Reduced => &[0],
            Self::Abstract => &[],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerSnapshot {
    pub id: u32,
    pub pos: Vec2,
    pub side: Side,
    pub sent_off: bool,
}

/// Plain-old-data view of the match at one instant (no heap).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatchSnapshot {
    pub t_ms: u32,
    pub tick: u32,
    pub players: [PlayerSnapshot; PLAYERS],
    pub ball: Vec3,
    pub score: [u8; 2],
    pub phases: [Phase; 2],
}
