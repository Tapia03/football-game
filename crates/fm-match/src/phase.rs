//! Game phase per team and the state machine that derives it each logical
//! tick (spec 3.F). Time is counted in logical ticks, never wall-clock, so
//! every LOD sees identical phases.

/// Logical tick length (spec 3.A): 10 Hz.
pub const LOGICAL_DT_MS: u32 = 100;

/// A transition lasts 3 s after possession changes hands.
pub const TRANSITION_TICKS: u32 = 3_000 / LOGICAL_DT_MS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    InPossession,
    OutOfPossession,
    /// Won the ball within the last 3 s.
    TransitionAttack,
    /// Lost the ball within the last 3 s.
    TransitionDefense,
    SetPiece,
}

impl Phase {
    /// True for the two phases of the team that has the ball.
    #[must_use]
    pub const fn has_ball(self) -> bool {
        matches!(self, Self::InPossession | Self::TransitionAttack)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Home,
    Away,
}

impl Side {
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Home => Self::Away,
            Self::Away => Self::Home,
        }
    }
}

/// Who controls the ball this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Possession {
    Team(Side),
    /// Nobody: ball in flight or contested. Teams keep their last phase.
    Loose,
}

/// Derives both teams' phases from the possession history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PhaseStateMachine {
    holder: Side,
    /// Ticks since `holder` gained the ball; saturates.
    ticks_since_change: u32,
}

impl PhaseStateMachine {
    /// Kick-off: `kicking_off` starts in possession, no transition pending.
    #[must_use]
    pub const fn new(kicking_off: Side) -> Self {
        Self {
            holder: kicking_off,
            ticks_since_change: TRANSITION_TICKS,
        }
    }

    /// Engine entry point: advances one logical tick from the frame's
    /// possession / set-piece status; returns `(home_phase, away_phase)`.
    pub fn update(&mut self, frame: &crate::tick_frame::TickFrame) -> (Phase, Phase) {
        self.advance(frame.possession(), frame.set_piece())
    }

    /// The pure transition behind `update`.
    pub fn advance(&mut self, possession: Possession, set_piece: bool) -> (Phase, Phase) {
        match possession {
            Possession::Team(side) if side != self.holder => {
                self.holder = side;
                self.ticks_since_change = 0;
            }
            // Same holder or loose ball: the transition clock keeps running.
            _ => self.ticks_since_change = self.ticks_since_change.saturating_add(1),
        }
        self.phases(set_piece)
    }

    /// Phases for the current state without advancing time.
    #[must_use]
    pub fn phases(&self, set_piece: bool) -> (Phase, Phase) {
        let (with, without) = if set_piece {
            (Phase::SetPiece, Phase::SetPiece)
        } else if self.ticks_since_change < TRANSITION_TICKS {
            (Phase::TransitionAttack, Phase::TransitionDefense)
        } else {
            (Phase::InPossession, Phase::OutOfPossession)
        };
        match self.holder {
            Side::Home => (with, without),
            Side::Away => (without, with),
        }
    }

    #[must_use]
    pub const fn holder(&self) -> Side {
        self.holder
    }
}

#[cfg(test)]
mod tests {
    use super::{Phase, PhaseStateMachine, Possession, Side, TRANSITION_TICKS};

    const HOME: Possession = Possession::Team(Side::Home);
    const AWAY: Possession = Possession::Team(Side::Away);

    #[test]
    fn kick_off_is_settled_possession() {
        let mut m = PhaseStateMachine::new(Side::Home);
        assert_eq!(
            m.advance(HOME, false),
            (Phase::InPossession, Phase::OutOfPossession)
        );
    }

    #[test]
    fn turnover_opens_a_three_second_transition() {
        assert_eq!(TRANSITION_TICKS, 30);
        let mut m = PhaseStateMachine::new(Side::Home);
        // Tick of the steal counts as tick 0 of the transition.
        assert_eq!(
            m.advance(AWAY, false),
            (Phase::TransitionDefense, Phase::TransitionAttack)
        );
        for _ in 1..TRANSITION_TICKS {
            assert_eq!(m.advance(AWAY, false).1, Phase::TransitionAttack);
        }
        // Exactly 30 ticks (3.0 s) after the steal the transition is over.
        assert_eq!(
            m.advance(AWAY, false),
            (Phase::OutOfPossession, Phase::InPossession)
        );
    }

    #[test]
    fn loose_ball_keeps_phase_and_clock() {
        let mut m = PhaseStateMachine::new(Side::Home);
        m.advance(AWAY, false);
        for _ in 0..10 {
            assert_eq!(
                m.advance(Possession::Loose, false).1,
                Phase::TransitionAttack
            );
        }
        for _ in 0..TRANSITION_TICKS {
            m.advance(Possession::Loose, false);
        }
        assert_eq!(m.advance(Possession::Loose, false).1, Phase::InPossession);
        assert_eq!(m.holder(), Side::Away);
    }

    #[test]
    fn regain_during_transition_restarts_clock() {
        let mut m = PhaseStateMachine::new(Side::Home);
        m.advance(AWAY, false);
        for _ in 0..20 {
            m.advance(AWAY, false);
        }
        assert_eq!(m.advance(HOME, false).0, Phase::TransitionAttack);
        for _ in 1..TRANSITION_TICKS {
            assert_eq!(m.advance(HOME, false).0, Phase::TransitionAttack);
        }
        assert_eq!(m.advance(HOME, false).0, Phase::InPossession);
    }

    #[test]
    fn set_piece_overrides_both_teams() {
        let mut m = PhaseStateMachine::new(Side::Home);
        assert_eq!(m.advance(AWAY, true), (Phase::SetPiece, Phase::SetPiece));
        assert!(Phase::TransitionAttack.has_ball());
        assert!(!Phase::OutOfPossession.has_ball());
        assert_eq!(Side::Home.other(), Side::Away);
    }
}
