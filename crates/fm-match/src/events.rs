//! Match events. The log is pre-allocated at kick-off, so recording an event
//! inside `tick_logic` never allocates (criterion 17).

use fm_entities::PlayerId;

use crate::phase::Side;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CardKind {
    Yellow,
    /// Direct red, or the second yellow.
    Red,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RestartKind {
    KickOff,
    ThrowIn,
    GoalKick,
    Corner,
    FreeKick,
    Penalty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventKind {
    Goal {
        side: Side,
        scorer: PlayerId,
    },
    Shot {
        side: Side,
        player: PlayerId,
        on_target: bool,
    },
    Save {
        keeper: PlayerId,
    },
    Foul {
        by: PlayerId,
        on: PlayerId,
    },
    Card {
        player: PlayerId,
        card: CardKind,
    },
    Restart {
        side: Side,
        kind: RestartKind,
    },
    HalfTime,
    FullTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MatchEvent {
    /// Logical tick at which the event happened (×100 ms = match time).
    pub tick: u32,
    pub kind: EventKind,
}

/// Fixed-capacity event log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventLog {
    events: Vec<MatchEvent>,
    dropped: u32,
}

/// Comfortably above any real match (~300-500 events).
pub const EVENT_CAPACITY: usize = 4_096;

impl EventLog {
    #[must_use]
    pub fn new() -> Self {
        Self {
            events: Vec::with_capacity(EVENT_CAPACITY),
            dropped: 0,
        }
    }

    /// Records an event; past capacity it is counted instead of growing.
    pub fn push(&mut self, e: MatchEvent) {
        if self.events.len() < EVENT_CAPACITY {
            self.events.push(e);
        } else {
            self.dropped = self.dropped.saturating_add(1);
        }
    }

    #[must_use]
    pub fn as_slice(&self) -> &[MatchEvent] {
        &self.events
    }

    #[must_use]
    pub const fn dropped(&self) -> u32 {
        self.dropped
    }
}

impl Default for EventLog {
    fn default() -> Self {
        Self::new()
    }
}
