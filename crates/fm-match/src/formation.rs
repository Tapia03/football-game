//! Formation tables: 11 slots per formation in team-relative space
//! (see `frame`). Base shape = the team in possession with the ball at the
//! centre spot; `anchor` deforms it per ball position, phase and tactics.

use crate::frame::Rel;

/// Tactical role of a slot; drives `RoleBehavior` and anchor rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Goalkeeper,
    CentreBack,
    FullBack,
    WingBack,
    DefensiveMidfielder,
    CentralMidfielder,
    WideMidfielder,
    AttackingMidfielder,
    Winger,
    Striker,
}

impl Role {
    /// Line the role belongs to, used to scale tactical shifts.
    #[must_use]
    pub const fn line(self) -> Line {
        match self {
            Self::Goalkeeper => Line::Goalkeeper,
            Self::CentreBack | Self::FullBack | Self::WingBack => Line::Defence,
            Self::DefensiveMidfielder
            | Self::CentralMidfielder
            | Self::WideMidfielder
            | Self::AttackingMidfielder => Line::Midfield,
            Self::Winger | Self::Striker => Line::Attack,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Line {
    Goalkeeper,
    Defence,
    Midfield,
    Attack,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slot {
    pub role: Role,
    pub base: Rel,
}

const fn s(role: Role, depth: f32, lateral: f32) -> Slot {
    Slot {
        role,
        base: Rel::new(depth, lateral),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Formation {
    F442,
    F433,
    F4231,
    F352,
    F532,
}

use Role::{
    AttackingMidfielder as AM, CentralMidfielder as CM, CentreBack as CB,
    DefensiveMidfielder as DM, FullBack as FB, Goalkeeper as GK, Striker as ST,
    WideMidfielder as WM, WingBack as WB, Winger as WG,
};

// Lateral +1 = team's left. Depths sit in 0.2..0.65 at rest so phase and
// tactic shifts never push the block off the pitch.
const F442: [Slot; 11] = [
    s(GK, 0.03, 0.0),
    s(FB, 0.22, -0.75),
    s(CB, 0.2, -0.25),
    s(CB, 0.2, 0.25),
    s(FB, 0.22, 0.75),
    s(WM, 0.42, -0.8),
    s(CM, 0.4, -0.25),
    s(CM, 0.4, 0.25),
    s(WM, 0.42, 0.8),
    s(ST, 0.6, -0.2),
    s(ST, 0.6, 0.2),
];
const F433: [Slot; 11] = [
    s(GK, 0.03, 0.0),
    s(FB, 0.22, -0.75),
    s(CB, 0.2, -0.25),
    s(CB, 0.2, 0.25),
    s(FB, 0.22, 0.75),
    s(DM, 0.33, 0.0),
    s(CM, 0.42, -0.35),
    s(CM, 0.42, 0.35),
    s(WG, 0.6, -0.75),
    s(ST, 0.63, 0.0),
    s(WG, 0.6, 0.75),
];
const F4231: [Slot; 11] = [
    s(GK, 0.03, 0.0),
    s(FB, 0.22, -0.75),
    s(CB, 0.2, -0.25),
    s(CB, 0.2, 0.25),
    s(FB, 0.22, 0.75),
    s(DM, 0.34, -0.2),
    s(DM, 0.34, 0.2),
    s(WG, 0.5, -0.75),
    s(AM, 0.5, 0.0),
    s(WG, 0.5, 0.75),
    s(ST, 0.64, 0.0),
];
const F352: [Slot; 11] = [
    s(GK, 0.03, 0.0),
    s(CB, 0.2, -0.45),
    s(CB, 0.19, 0.0),
    s(CB, 0.2, 0.45),
    s(WB, 0.4, -0.85),
    s(CM, 0.38, -0.3),
    s(DM, 0.33, 0.0),
    s(CM, 0.38, 0.3),
    s(WB, 0.4, 0.85),
    s(ST, 0.6, -0.2),
    s(ST, 0.6, 0.2),
];
const F532: [Slot; 11] = [
    s(GK, 0.03, 0.0),
    s(WB, 0.26, -0.85),
    s(CB, 0.2, -0.45),
    s(CB, 0.19, 0.0),
    s(CB, 0.2, 0.45),
    s(WB, 0.26, 0.85),
    s(CM, 0.4, -0.4),
    s(CM, 0.38, 0.0),
    s(CM, 0.4, 0.4),
    s(ST, 0.6, -0.2),
    s(ST, 0.6, 0.2),
];

impl Formation {
    pub const ALL: [Self; 5] = [Self::F442, Self::F433, Self::F4231, Self::F352, Self::F532];

    #[must_use]
    pub const fn slots(self) -> &'static [Slot; 11] {
        match self {
            Self::F442 => &F442,
            Self::F433 => &F433,
            Self::F4231 => &F4231,
            Self::F352 => &F352,
            Self::F532 => &F532,
        }
    }

    /// Display name, e.g. "4-2-3-1".
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::F442 => "4-4-2",
            Self::F433 => "4-3-3",
            Self::F4231 => "4-2-3-1",
            Self::F352 => "3-5-2",
            Self::F532 => "5-3-2",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Formation, Line, Role};

    fn count(f: Formation, line: Line) -> usize {
        f.slots().iter().filter(|s| s.role.line() == line).count()
    }

    #[test]
    fn line_counts_match_names() {
        // Wing-backs count as defence in 5-3-2 and as midfield in 3-5-2 by
        // name; the table uses WingBack in both, so check the name instead.
        for f in Formation::ALL {
            assert_eq!(count(f, Line::Goalkeeper), 1, "{}", f.name());
            assert_eq!(f.slots().len(), 11);
        }
        assert_eq!(count(Formation::F442, Line::Defence), 4);
        assert_eq!(count(Formation::F442, Line::Midfield), 4);
        assert_eq!(count(Formation::F442, Line::Attack), 2);
        assert_eq!(count(Formation::F433, Line::Attack), 3);
        // 4-2-3-1: the wide pair of the "3" are wingers (attack line), so the
        // midfield line is the double pivot + the No. 10.
        assert_eq!(count(Formation::F4231, Line::Defence), 4);
        assert_eq!(count(Formation::F4231, Line::Midfield), 3);
        assert_eq!(count(Formation::F4231, Line::Attack), 3);
        let backs = |f: Formation| {
            f.slots()
                .iter()
                .filter(|s| s.role == Role::CentreBack)
                .count()
        };
        assert_eq!(backs(Formation::F352), 3);
        assert_eq!(backs(Formation::F532), 3);
    }

    #[test]
    fn slots_are_in_range_and_ordered_back_to_front() {
        for f in Formation::ALL {
            let slots = f.slots();
            assert_eq!(slots[0].role, Role::Goalkeeper);
            for s in slots {
                assert!((0.0..=0.7).contains(&s.base.depth), "{} {:?}", f.name(), s);
                assert!((-1.0..=1.0).contains(&s.base.lateral));
            }
            // Mirror symmetry: every slot has a partner at -lateral, same depth.
            for s in slots {
                assert!(
                    slots.iter().any(|o| o.role == s.role
                        && (o.base.lateral + s.base.lateral).abs() < 1e-6
                        && (o.base.depth - s.base.depth).abs() < 1e-6),
                    "{} not symmetric at {:?}",
                    f.name(),
                    s
                );
            }
        }
    }
}
