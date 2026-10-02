//! `FormationAnchor::compute`: where a slot should stand given the ball,
//! the team's phase and its tactics (spec 3.E, step 1). Pure function of its
//! inputs; evaluated once per logical tick.

use fm_core::Vec2;

use crate::formation::{Line, Slot};
use crate::frame::{Rel, TeamFrame};
use crate::phase::Phase;
use crate::tactics::{LineHeight, Mentality, Tactics, Width};

/// How the block deforms in one phase.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhaseShape {
    /// Vertical spread of the lines around the block centre (1 = base shape).
    pub spread: f32,
    /// How far the block centre follows the ball's depth (0..1).
    pub follow_depth: f32,
    /// How far each player slides toward the ball's side (0..1).
    pub follow_lateral: f32,
    /// Extra depth for the whole block (fraction of pitch length).
    pub push: f32,
    /// Lateral spread when the formation's width instruction does not apply.
    pub compact_width: f32,
}

/// Every tunable constant of `FormationAnchor`, in one place. `Default` is
/// the Phase 3 calibration; Phase 5 retunes here without touching code.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnchorTuning {
    /// Indexed by `phase_index`: `InPossession`, `OutOfPossession`,
    /// `TransitionAttack`, `TransitionDefense`, `SetPiece`.
    pub phase: [PhaseShape; 5],
    /// Rest depth of the block centre (team-relative).
    pub block_centre: f32,
    /// Whole-block depth shift per `Mentality`, Defensive..=Attacking.
    pub mentality_shift: [f32; 5],
    /// Lateral stretch in possession per `Width`, Narrow..=Wide.
    pub width_factor: [f32; 3],
    /// Defensive-line depth shift per `LineHeight`, Deep..=High.
    pub line_shift: [f32; 3],
    /// Share of `line_shift` applied to defence / midfield / attack.
    pub line_weight: [f32; 3],
    /// Keeper depth: `base + per_ball_depth * ball_depth`, clamped.
    pub gk_depth_base: f32,
    pub gk_depth_per_ball: f32,
    pub gk_depth_range: (f32, f32),
    /// Keeper lateral shading toward the ball, clamped.
    pub gk_lateral_follow: f32,
    pub gk_lateral_limit: f32,
    /// Outfield clamps that keep anchors inside the pitch.
    pub depth_range: (f32, f32),
    pub lateral_range: (f32, f32),
}

impl Default for AnchorTuning {
    fn default() -> Self {
        // In possession the team stretches to create space; out of possession
        // it compresses and slides to the ball; transitions exaggerate each.
        Self {
            phase: [
                PhaseShape {
                    spread: 1.1,
                    follow_depth: 0.5,
                    follow_lateral: 0.15,
                    push: 0.0,
                    compact_width: 1.0,
                },
                PhaseShape {
                    spread: 0.8,
                    follow_depth: 0.6,
                    follow_lateral: 0.3,
                    push: 0.0,
                    compact_width: 0.7,
                },
                PhaseShape {
                    spread: 1.15,
                    follow_depth: 0.5,
                    follow_lateral: 0.1,
                    push: 0.05,
                    compact_width: 1.0,
                },
                PhaseShape {
                    spread: 0.85,
                    follow_depth: 0.5,
                    follow_lateral: 0.25,
                    push: -0.05,
                    compact_width: 0.75,
                },
                PhaseShape {
                    spread: 1.0,
                    follow_depth: 0.4,
                    follow_lateral: 0.1,
                    push: 0.0,
                    compact_width: 0.85,
                },
            ],
            block_centre: 0.4,
            // ±0.08 ≈ ±8.4 m: Defensive→Attacking moves the block ~17 m
            // (criterion 19 needs ≥ 5 m), leaving room for other modifiers.
            mentality_shift: [-0.08, -0.04, 0.0, 0.04, 0.08],
            width_factor: [0.75, 0.9, 1.0],
            // ≈ ±5 m for the back line.
            line_shift: [-0.05, 0.0, 0.05],
            // Defenders carry the full instruction, midfield half, attackers
            // none (free to stay high).
            line_weight: [1.0, 0.5, 0.0],
            gk_depth_base: 0.02,
            gk_depth_per_ball: 0.08,
            gk_depth_range: (0.01, 0.12),
            gk_lateral_follow: 0.15,
            gk_lateral_limit: 0.1,
            depth_range: (0.04, 0.96),
            lateral_range: (-0.95, 0.95),
        }
    }
}

const fn phase_index(phase: Phase) -> usize {
    match phase {
        Phase::InPossession => 0,
        Phase::OutOfPossession => 1,
        Phase::TransitionAttack => 2,
        Phase::TransitionDefense => 3,
        Phase::SetPiece => 4,
    }
}

const fn mentality_index(m: Mentality) -> usize {
    match m {
        Mentality::Defensive => 0,
        Mentality::Cautious => 1,
        Mentality::Balanced => 2,
        Mentality::Positive => 3,
        Mentality::Attacking => 4,
    }
}

const fn width_index(w: Width) -> usize {
    match w {
        Width::Narrow => 0,
        Width::Normal => 1,
        Width::Wide => 2,
    }
}

const fn line_index(l: LineHeight) -> usize {
    match l {
        LineHeight::Deep => 0,
        LineHeight::Normal => 1,
        LineHeight::High => 2,
    }
}

pub struct FormationAnchor;

impl FormationAnchor {
    /// Anchor position on the pitch for `slot`, given the ball position on the
    /// pitch, the team's `phase`, its `tactics` and attacking direction.
    #[must_use]
    pub fn compute(
        slot: &Slot,
        ball: Vec2,
        phase: Phase,
        tactics: Tactics,
        frame: TeamFrame,
        tuning: &AnchorTuning,
    ) -> Vec2 {
        let b = frame.to_rel(ball);
        let rel = if slot.role.line() == Line::Goalkeeper {
            // Keeper edges off the line as the ball goes upfield, and shades
            // toward the ball's side to cover the near post.
            let lim = tuning.gk_lateral_limit;
            Rel::new(
                (tuning.gk_depth_base + tuning.gk_depth_per_ball * b.depth)
                    .clamp(tuning.gk_depth_range.0, tuning.gk_depth_range.1),
                (tuning.gk_lateral_follow * b.lateral).clamp(-lim, lim),
            )
        } else {
            outfield(slot, b, phase, tactics, tuning)
        };
        frame.to_pitch(rel)
    }
}

fn outfield(slot: &Slot, ball: Rel, phase: Phase, tactics: Tactics, t: &AnchorTuning) -> Rel {
    let sh = &t.phase[phase_index(phase)];
    let line_weight = match slot.role.line() {
        Line::Defence => t.line_weight[0],
        Line::Midfield => t.line_weight[1],
        Line::Attack => t.line_weight[2],
        Line::Goalkeeper => 0.0,
    };
    let centre = t.block_centre + (ball.depth - 0.5) * sh.follow_depth;
    let depth = centre
        + (slot.base.depth - t.block_centre) * sh.spread
        + sh.push
        + t.mentality_shift[mentality_index(tactics.mentality)]
        + t.line_shift[line_index(tactics.line_height)] * line_weight;

    let width = if phase.has_ball() {
        t.width_factor[width_index(tactics.width)]
    } else {
        sh.compact_width
    };
    let lateral = slot.base.lateral * width + ball.lateral * sh.follow_lateral;

    Rel::new(
        depth.clamp(t.depth_range.0, t.depth_range.1),
        lateral.clamp(t.lateral_range.0, t.lateral_range.1),
    )
}

#[cfg(test)]
mod tests {
    use super::{AnchorTuning, FormationAnchor};
    use crate::formation::{Formation, Line};
    use crate::frame::TeamFrame;
    use crate::phase::Phase;
    use crate::tactics::{LineHeight, Mentality, Tactics, Width};
    use fm_core::pitch::{self, CENTRE};
    use fm_core::{GoalEnd, Vec2};

    const PHASES: [Phase; 5] = [
        Phase::InPossession,
        Phase::OutOfPossession,
        Phase::TransitionAttack,
        Phase::TransitionDefense,
        Phase::SetPiece,
    ];

    fn anchors(f: Formation, ball: Vec2, phase: Phase, t: Tactics, end: GoalEnd) -> Vec<Vec2> {
        f.slots()
            .iter()
            .map(|s| {
                FormationAnchor::compute(
                    s,
                    ball,
                    phase,
                    t,
                    TeamFrame::new(end),
                    &AnchorTuning::default(),
                )
            })
            .collect()
    }

    /// Centre of mass of the 10 outfield players.
    fn com(f: Formation, ball: Vec2, phase: Phase, t: Tactics, end: GoalEnd) -> Vec2 {
        let a = anchors(f, ball, phase, t, end);
        let sum = a[1..].iter().fold(Vec2::ZERO, |acc, p| acc + *p);
        sum / 10.0
    }

    fn ball_grid() -> Vec<Vec2> {
        let mut v = Vec::new();
        for i in 0..=10_u8 {
            for j in 0..=4_u8 {
                v.push(Vec2::new(f32::from(i) * 10.5, f32::from(j) * 17.0));
            }
        }
        v
    }

    #[test]
    fn anchors_always_inside_pitch() {
        let tactics = [
            Tactics::default(),
            Tactics {
                mentality: Mentality::Attacking,
                width: Width::Wide,
                line_height: LineHeight::High,
            },
            Tactics {
                mentality: Mentality::Defensive,
                width: Width::Narrow,
                line_height: LineHeight::Deep,
            },
        ];
        for f in Formation::ALL {
            for phase in PHASES {
                for &t in &tactics {
                    for end in [GoalEnd::Left, GoalEnd::Right] {
                        for ball in ball_grid() {
                            for p in anchors(f, ball, phase, t, end) {
                                assert!(pitch::in_pitch(p), "{} {phase:?} {p:?}", f.name());
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn centre_of_mass_follows_the_ball() {
        let t = Tactics::default();
        for f in Formation::ALL {
            for phase in PHASES {
                let back = com(f, Vec2::new(20.0, 34.0), phase, t, GoalEnd::Right);
                let mid = com(f, CENTRE, phase, t, GoalEnd::Right);
                let front = com(f, Vec2::new(90.0, 34.0), phase, t, GoalEnd::Right);
                assert!(back.x < mid.x && mid.x < front.x, "{} {phase:?}", f.name());
                let left = com(f, Vec2::new(52.5, 60.0), phase, t, GoalEnd::Right);
                let right = com(f, Vec2::new(52.5, 8.0), phase, t, GoalEnd::Right);
                assert!(left.y > right.y, "{} {phase:?} lateral shift", f.name());
            }
        }
    }

    #[test]
    fn mentality_moves_block_at_least_five_metres() {
        // Criterion 19 is about the running match (Phase 5/7); here we check
        // the anchor model leaves that much room.
        let def = Tactics {
            mentality: Mentality::Defensive,
            ..Tactics::default()
        };
        let att = Tactics {
            mentality: Mentality::Attacking,
            ..Tactics::default()
        };
        for f in Formation::ALL {
            for phase in PHASES {
                let d = com(f, CENTRE, phase, def, GoalEnd::Right);
                let a = com(f, CENTRE, phase, att, GoalEnd::Right);
                assert!(a.x - d.x >= 5.0, "{} {phase:?}: {}", f.name(), a.x - d.x);
            }
        }
    }

    #[test]
    fn out_of_possession_is_more_compact() {
        let t = Tactics::default();
        let spread = |phase| {
            let a = anchors(Formation::F442, CENTRE, phase, t, GoalEnd::Right);
            let xs = a[1..].iter().map(|p| p.x);
            let ys = a[1..].iter().map(|p| p.y);
            let span = |it: &mut dyn Iterator<Item = f32>| {
                let v: Vec<f32> = it.collect();
                v.iter().copied().fold(f32::MIN, f32::max)
                    - v.iter().copied().fold(f32::MAX, f32::min)
            };
            (span(&mut xs.into_iter()), span(&mut ys.into_iter()))
        };
        let (in_x, in_y) = spread(Phase::InPossession);
        let (out_x, out_y) = spread(Phase::OutOfPossession);
        assert!(out_x < in_x && out_y < in_y);
    }

    #[test]
    fn high_line_lifts_defenders_more_than_attackers() {
        let deep = Tactics {
            line_height: LineHeight::Deep,
            ..Tactics::default()
        };
        let high = Tactics {
            line_height: LineHeight::High,
            ..Tactics::default()
        };
        let f = Formation::F442;
        let lo = anchors(f, CENTRE, Phase::OutOfPossession, deep, GoalEnd::Right);
        let hi = anchors(f, CENTRE, Phase::OutOfPossession, high, GoalEnd::Right);
        for (i, s) in f.slots().iter().enumerate() {
            let lift = hi[i].x - lo[i].x;
            match s.role.line() {
                Line::Defence => assert!(lift > 9.0, "defender lift {lift}"),
                Line::Attack => assert!(lift.abs() < 1e-4, "attacker lift {lift}"),
                _ => {}
            }
        }
    }

    #[test]
    fn both_directions_are_mirror_images() {
        let t = Tactics::default();
        for f in Formation::ALL {
            for phase in PHASES {
                for ball in ball_grid() {
                    let mirrored_ball = Vec2::new(105.0 - ball.x, 68.0 - ball.y);
                    let r = anchors(f, ball, phase, t, GoalEnd::Right);
                    let l = anchors(f, mirrored_ball, phase, t, GoalEnd::Left);
                    for (a, b) in r.iter().zip(&l) {
                        let m = Vec2::new(105.0 - b.x, 68.0 - b.y);
                        assert!(a.distance(m) < 1e-3, "{} {phase:?}", f.name());
                    }
                }
            }
        }
    }

    #[test]
    #[allow(clippy::float_cmp)] // exact on purpose: these are pinned literals
    fn default_tuning_is_the_phase3_calibration() {
        // Pinned so an accidental edit of the defaults shows up in review.
        let t = AnchorTuning::default();
        assert_eq!(t.mentality_shift, [-0.08, -0.04, 0.0, 0.04, 0.08]);
        assert_eq!(t.width_factor, [0.75, 0.9, 1.0]);
        assert_eq!(t.line_shift, [-0.05, 0.0, 0.05]);
        assert_eq!(t.line_weight, [1.0, 0.5, 0.0]);
        assert!((t.block_centre - 0.4).abs() < f32::EPSILON);
        assert!((t.phase[1].spread - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn tuning_changes_anchor_output() {
        let mut t = AnchorTuning::default();
        t.mentality_shift[2] = 0.1;
        let slot = &Formation::F442.slots()[5];
        let frame = TeamFrame::new(GoalEnd::Right);
        let base = FormationAnchor::compute(
            slot,
            CENTRE,
            Phase::InPossession,
            Tactics::default(),
            frame,
            &AnchorTuning::default(),
        );
        let tuned = FormationAnchor::compute(
            slot,
            CENTRE,
            Phase::InPossession,
            Tactics::default(),
            frame,
            &t,
        );
        assert!((tuned.x - base.x - 0.1 * 105.0).abs() < 1e-3);
    }

    #[test]
    fn goalkeeper_stays_near_own_goal() {
        let t = Tactics::default();
        for ball in ball_grid() {
            let gk = anchors(
                Formation::F433,
                ball,
                Phase::InPossession,
                t,
                GoalEnd::Right,
            )[0];
            assert!(
                gk.x <= 12.7 && pitch::in_penalty_area(gk, GoalEnd::Left),
                "{gk:?}"
            );
        }
    }
}
