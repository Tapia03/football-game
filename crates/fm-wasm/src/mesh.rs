//! Frame mesh (spec Fase 6, 6A): the pitch, the 22 players and the ball of
//! one decoded snapshot as a triangle list in clip space. Pure functions of
//! their arguments — no engine and no state: the main thread reads the
//! snapshot ring, interpolates, and hands the frame in.

use fm_core::pitch::{self, GoalEnd};
use fm_core::Vec2;
use fm_match::Side;
use fm_render::shapes::{tessellate, Mesh, Rgba, Shape, View};

use crate::sab::Frame;

/// Clear colour behind the pitch.
pub const BACKGROUND: [f32; 4] = [0.04, 0.24, 0.12, 1.0];

/// Players per side: indices `0..11` are the home side.
const SIDE: usize = 11;

const LINE: f32 = 0.15;

fn pitch_shapes(out: &mut Vec<Shape>) {
    // Mown stripes: 10 bands across the length.
    let (a, b) = (Rgba::hex(0x2E_8B47), Rgba::hex(0x29_7F40));
    for i in 0..10_u8 {
        let x0 = f32::from(i) * pitch::LENGTH / 10.0;
        out.push(Shape::Rect {
            min: Vec2::new(x0, 0.0),
            max: Vec2::new(x0 + pitch::LENGTH / 10.0, pitch::WIDTH),
            color: if i % 2 == 0 { a } else { b },
        });
    }
    let white = Rgba(1.0, 1.0, 1.0, 0.9);
    let line = |out: &mut Vec<Shape>, a: Vec2, b: Vec2| {
        out.push(Shape::Line {
            a,
            b,
            width: LINE,
            color: white,
        });
    };
    let rect = |out: &mut Vec<Shape>, x0: f32, y0: f32, x1: f32, y1: f32| {
        line(out, Vec2::new(x0, y0), Vec2::new(x1, y0));
        line(out, Vec2::new(x1, y0), Vec2::new(x1, y1));
        line(out, Vec2::new(x1, y1), Vec2::new(x0, y1));
        line(out, Vec2::new(x0, y1), Vec2::new(x0, y0));
    };
    rect(out, 0.0, 0.0, pitch::LENGTH, pitch::WIDTH);
    line(
        out,
        Vec2::new(pitch::HALF_LENGTH, 0.0),
        Vec2::new(pitch::HALF_LENGTH, pitch::WIDTH),
    );
    out.push(Shape::Ring {
        centre: pitch::CENTRE,
        radius: pitch::CENTRE_CIRCLE_RADIUS,
        width: LINE,
        color: white,
    });
    out.push(Shape::Disc {
        centre: pitch::CENTRE,
        radius: 0.3,
        color: white,
    });
    for end in [GoalEnd::Left, GoalEnd::Right] {
        let x = end.goal_line_x();
        let dir = -end.direction(); // into the pitch
        let box_rect = |out: &mut Vec<Shape>, depth: f32, width: f32| {
            let (y0, y1) = (
                pitch::HALF_WIDTH - width / 2.0,
                pitch::HALF_WIDTH + width / 2.0,
            );
            let x1 = x + dir * depth;
            line(out, Vec2::new(x, y0), Vec2::new(x1, y0));
            line(out, Vec2::new(x1, y0), Vec2::new(x1, y1));
            line(out, Vec2::new(x1, y1), Vec2::new(x, y1));
        };
        box_rect(out, pitch::PENALTY_AREA_DEPTH, pitch::PENALTY_AREA_WIDTH);
        box_rect(out, pitch::GOAL_AREA_DEPTH, pitch::GOAL_AREA_WIDTH);
        out.push(Shape::Disc {
            centre: end.penalty_spot(),
            radius: 0.25,
            color: white,
        });
        // Goal frame behind the line.
        let back = x - dir * 2.0;
        let (g0, g1) = (
            pitch::HALF_WIDTH - pitch::GOAL_WIDTH / 2.0,
            pitch::HALF_WIDTH + pitch::GOAL_WIDTH / 2.0,
        );
        line(out, Vec2::new(x, g0), Vec2::new(back, g0));
        line(out, Vec2::new(back, g0), Vec2::new(back, g1));
        line(out, Vec2::new(back, g1), Vec2::new(x, g1));
    }
}

/// How the pitch is fitted into a `width_px` × `height_px` canvas: the one
/// view both the mesh and anything laid over the canvas use.
#[must_use]
pub fn pitch_view(width_px: u32, height_px: u32) -> View {
    #[allow(clippy::cast_precision_loss)] // canvas sizes ≪ 2^24
    let (w, h) = (width_px.max(1) as f32, height_px.max(1) as f32);
    View::fit(
        pitch::CENTRE,
        Vec2::new(pitch::LENGTH + 8.0, pitch::WIDTH + 8.0),
        w,
        h,
    )
}

/// A velocity arrow is this many seconds of travel long (a sprint at 8 m/s
/// draws ~5 m), and players slower than `ARROW_MIN_SPEED` get none.
const ARROW_SECONDS: f32 = 0.6;
const ARROW_MIN_SPEED: f32 = 0.5;

/// What is laid over the plain frame (the toggles of the page).
#[derive(Debug, Clone, Copy, Default)]
pub struct Overlays<'a> {
    /// F2: velocities in m/s, one per player; each moving player gets an
    /// arrow along its velocity.
    pub velocities: Option<&'a [Vec2; 22]>,
    /// F3: the offside line (`offside_line`).
    pub offside: bool,
    /// F4: the formation lines, from the role code of each player
    /// (`role_code`, engine order).
    pub formation: Option<&'a [u8; 22]>,
}

/// Phase codes of a team that has the ball (`sab` layout): in possession,
/// or in attacking transition.
const fn has_ball(phase: u8) -> bool {
    matches!(phase, 0 | 2)
}

const fn first_of(side: Side) -> usize {
    match side {
        Side::Home => 0,
        Side::Away => SIDE,
    }
}

/// Goal `side` defends in the frame's half: home attacks to the right in
/// the first half and the sides swap at half time.
const fn defended_goal(frame: &Frame, side: Side) -> GoalEnd {
    match (side, frame.half == 0) {
        (Side::Home, true) | (Side::Away, false) => GoalEnd::Left,
        (Side::Away, true) | (Side::Home, false) => GoalEnd::Right,
    }
}

/// The side attacking in `frame`, from the phase codes; `None` when they do
/// not tell (set piece: both teams carry the same code).
#[must_use]
pub const fn attacking_side(frame: &Frame) -> Option<Side> {
    match (has_ball(frame.phases[0]), has_ball(frame.phases[1])) {
        (true, false) => Some(Side::Home),
        (false, true) => Some(Side::Away),
        _ => None,
    }
}

/// `x` of the second-last player of `defending`, counted from its own goal
/// line: the keeper counts, players sent off do not; with fewer than two
/// left, the goal line. The engine's own line (`TickFrame::offside_line`),
/// read from a snapshot: the same values, bit for bit.
#[must_use]
pub fn second_last_defender_x(frame: &Frame, defending: Side) -> f32 {
    let goal = defended_goal(frame, defending);
    let goal_x = goal.goal_line_x();
    let away_from_goal = -goal.direction();
    let start = first_of(defending);
    let mut depth = [f32::INFINITY; SIDE];
    for (i, d) in depth.iter_mut().enumerate() {
        if frame.sent_off >> (start + i) & 1 == 0 {
            *d = (frame.players[start + i].x - goal_x) * away_from_goal;
        }
    }
    // Deepest player, then the deepest of the others (first index on ties).
    let deepest = (1..SIDE).fold(0, |m, i| if depth[i] < depth[m] { i } else { m });
    let second = (0..SIDE)
        .filter(|&i| i != deepest)
        .fold(None, |m: Option<usize>, i| match m {
            Some(j) if depth[j] <= depth[i] => Some(j),
            _ => Some(i),
        });
    match second {
        Some(i) if depth[i].is_finite() => frame.players[start + i].x,
        _ => goal_x,
    }
}

/// The offside line of the Laws for the team attacking in `frame`, as
/// `(defending side, x)`: an attacker beyond it is in an offside position.
/// It is the nearest to the defended goal of the second-last defender, the
/// ball and the halfway line — so never past halfway nor behind the ball.
/// (The engine's line is the second-last defender alone: it whistles
/// nothing and only aims its runs with it.) `None` when the snapshot does
/// not tell who attacks.
#[must_use]
pub fn offside_line(frame: &Frame) -> Option<(Side, f32)> {
    let defending = match attacking_side(frame)? {
        Side::Home => Side::Away,
        Side::Away => Side::Home,
    };
    let goal = defended_goal(frame, defending);
    let depth = |x: f32| (x - goal.goal_line_x()) * -goal.direction();
    let mut x = second_last_defender_x(frame, defending);
    for other in [frame.ball.x, pitch::HALF_LENGTH] {
        if depth(other) < depth(x) {
            x = other;
        }
    }
    Some((defending, x))
}

/// Dashes of the offside line: 17 across the 68 m.
const DASH: f32 = 2.5;
const DASH_PERIOD: f32 = 4.0;

fn offside_shapes(frame: &Frame, out: &mut Vec<Shape>) {
    let Some((defending, x)) = offside_line(frame) else {
        return;
    };
    let color = match defending {
        Side::Home => Rgba(1.0, 0.45, 0.42, 0.95),
        Side::Away => Rgba(0.35, 0.75, 1.0, 0.95),
    };
    let mut y = (DASH_PERIOD - DASH) / 2.0;
    while y < pitch::WIDTH {
        out.push(Shape::Line {
            a: Vec2::new(x, y),
            b: Vec2::new(x, (y + DASH).min(pitch::WIDTH)),
            width: 0.35,
            color,
        });
        y += DASH_PERIOD;
    }
}

/// Sector of a role code (`role_code`): 0 defence, 1 midfield, 2 attack —
/// `Role::line`. The keeper belongs to none.
const fn sector(code: u8) -> Option<u8> {
    match code {
        1..=3 => Some(0),
        4..=7 => Some(1),
        8 | 9 => Some(2),
        _ => None,
    }
}

/// One broken line per team and sector, joining the sector's players in
/// order across the width of the pitch. A sector with one player has none.
fn formation_shapes(frame: &Frame, roster: &[u8; 22], out: &mut Vec<Shape>) {
    for side in [Side::Home, Side::Away] {
        let start = first_of(side);
        let color = match side {
            Side::Home => Rgba(1.0, 0.45, 0.42, 0.6),
            Side::Away => Rgba(0.35, 0.75, 1.0, 0.6),
        };
        for s in 0..3 {
            let mut line = [Vec2::ZERO; SIDE];
            let mut n = 0;
            for (i, &code) in roster.iter().enumerate().skip(start).take(SIDE) {
                if frame.sent_off >> i & 1 == 0 && sector(code) == Some(s) {
                    line[n] = frame.players[i];
                    n += 1;
                }
            }
            // Stable: players level across the pitch keep the engine order.
            line[..n].sort_by(|a, b| a.y.total_cmp(&b.y));
            for pair in line[..n].windows(2) {
                out.push(Shape::Line {
                    a: pair[0],
                    b: pair[1],
                    width: 0.3,
                    color,
                });
            }
        }
    }
}

/// The 22 role codes from what the main thread holds; `None` when the
/// slice is not 22 values (no formation lines asked for).
#[must_use]
pub fn roster_from_parts(roster: &[u8]) -> Option<[u8; 22]> {
    roster.try_into().ok()
}

/// The whole frame (pitch, players, ball) for a `width_px` × `height_px`
/// canvas, with the `overlays` asked for. Players sent off are not drawn.
///
/// # Panics
/// Never: shirt numbers are 1..=11.
#[must_use]
pub fn frame_mesh(frame: &Frame, overlays: &Overlays, width_px: u32, height_px: u32) -> Mesh {
    let mut shapes = Vec::with_capacity(250);
    pitch_shapes(&mut shapes);
    // Overlays first, so the players sit on top of them.
    if let Some(roster) = overlays.formation {
        formation_shapes(frame, roster, &mut shapes);
    }
    if overlays.offside {
        offside_shapes(frame, &mut shapes);
    }
    if let Some(velocities) = overlays.velocities {
        let color = Rgba(1.0, 1.0, 1.0, 0.8);
        for (i, (&pos, &v)) in frame.players.iter().zip(velocities).enumerate() {
            if frame.sent_off >> i & 1 == 1 || v.length() < ARROW_MIN_SPEED {
                continue;
            }
            let tip = pos + v * ARROW_SECONDS;
            shapes.push(Shape::Line {
                a: pos,
                b: tip,
                width: 0.25,
                color,
            });
            shapes.push(Shape::Disc {
                centre: tip,
                radius: 0.3,
                color,
            });
        }
    }
    for (i, &pos) in frame.players.iter().enumerate() {
        if frame.sent_off >> i & 1 == 1 {
            continue;
        }
        let (fill, ring) = if i < SIDE {
            (Rgba::hex(0xD6_3031), Rgba::hex(0x7A_1414))
        } else {
            (Rgba::hex(0x09_84E3), Rgba::hex(0x05_3F6E))
        };
        shapes.push(Shape::Disc {
            centre: pos,
            radius: 1.15,
            color: ring,
        });
        shapes.push(Shape::Disc {
            centre: pos,
            radius: 0.95,
            color: fill,
        });
        shapes.push(Shape::Number {
            centre: pos,
            value: u8::try_from(i % SIDE + 1).expect("1..=11"),
            height: 0.9,
            color: Rgba(1.0, 1.0, 1.0, 1.0),
        });
    }
    // The ball grows a little with height so lofted balls read as airborne.
    let ball = frame.ball.xy();
    let lift = (frame.ball.z * 0.12).min(0.5);
    shapes.push(Shape::Disc {
        centre: ball,
        radius: 0.55 + lift,
        color: Rgba(0.0, 0.0, 0.0, 0.85),
    });
    shapes.push(Shape::Disc {
        centre: ball,
        radius: 0.45 + lift,
        color: Rgba(1.0, 1.0, 1.0, 1.0),
    });
    tessellate(&shapes, &pitch_view(width_px, height_px))
}

/// The 22 velocities from what the main thread holds: `v` is vx0, vy0, vx1,
/// … in m/s. `None` when the slice is not 44 values (no arrows asked for).
#[must_use]
pub fn velocities_from_parts(v: &[f32]) -> Option<[Vec2; 22]> {
    (v.len() == 44).then(|| {
        let mut out = [Vec2::ZERO; 22];
        for (i, o) in out.iter_mut().enumerate() {
            *o = Vec2::new(v[2 * i], v[2 * i + 1]);
        }
        out
    })
}

/// Label code of each role (spec Fase 6, 6B-2): the main thread turns it
/// into the abbreviation shown under a player with F1.
#[must_use]
pub const fn role_code(role: fm_match::Role) -> u8 {
    use fm_match::Role;
    match role {
        Role::Goalkeeper => 0,
        Role::CentreBack => 1,
        Role::FullBack => 2,
        Role::WingBack => 3,
        Role::DefensiveMidfielder => 4,
        Role::CentralMidfielder => 5,
        Role::WideMidfielder => 6,
        Role::AttackingMidfielder => 7,
        Role::Winger => 8,
        Role::Striker => 9,
    }
}

/// A frame from what the main thread holds after interpolating: `xy` is
/// x0, y0, x1, y1, … for the 22 players (missing values read as 0);
/// `phases` is the snapshot's phase word (home | away << 8 | half << 16).
#[must_use]
pub fn frame_from_parts(xy: &[f32], ball: [f32; 3], sent_off: u32, phases: u32) -> Frame {
    let byte = |shift: u32| (phases >> shift & 0xFF) as u8;
    let mut players = [Vec2::ZERO; 22];
    for (i, p) in players.iter_mut().enumerate() {
        let at = |k: usize| xy.get(2 * i + k).copied().unwrap_or(0.0);
        *p = Vec2::new(at(0), at(1));
    }
    Frame {
        tick: 0,
        t_ms: 0,
        score: [0, 0],
        phases: [byte(0), byte(8)],
        sent_off,
        ball: fm_core::Vec3::new(ball[0], ball[1], ball[2]),
        players,
        half: byte(16),
        cards: [0; 4],
        held: [0; 2],
        stats: [crate::sab::TeamStats::default(); 2],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sab::{decode, encode, Hud};
    use fm_match::demo::demo_match;
    use fm_match::{LodLevel, MatchEngine};

    const PLAIN: Overlays = Overlays {
        velocities: None,
        offside: false,
        formation: None,
    };

    fn frame_at(ticks: u32) -> Frame {
        let (db, setup) = demo_match(7);
        let mut e = MatchEngine::new(&setup, &db);
        for _ in 0..ticks {
            e.tick_logic();
        }
        let snap = e
            .sample(LodLevel::Full, e.state().now_ms())
            .expect("snapshot");
        decode(&encode(&snap, &Hud::default()))
    }

    #[test]
    fn frame_mesh_stays_in_clip_space() {
        let mesh = frame_mesh(&frame_at(6_000), &PLAIN, 1280, 820);
        assert!(mesh.vertex_count() > 5_000, "{}", mesh.vertex_count());
        assert!(mesh
            .verts
            .chunks(6)
            .all(|v| v[0].abs() <= 1.0 && v[1].abs() <= 1.0));
    }

    /// Same frame, same mesh: the helpers keep no state.
    #[test]
    fn frame_mesh_is_a_pure_function_of_the_frame() {
        let frame = frame_at(1_234);
        let all = Overlays {
            velocities: None,
            offside: true,
            formation: None,
        };
        let a = frame_mesh(&frame, &all, 1280, 820);
        let b = frame_mesh(&frame, &all, 1280, 820);
        assert_eq!(a.verts, b.verts);
        // And the parts the main thread passes rebuild the same frame.
        let xy: Vec<f32> = frame.players.iter().flat_map(|p| [p.x, p.y]).collect();
        let rebuilt = frame_from_parts(
            &xy,
            [frame.ball.x, frame.ball.y, frame.ball.z],
            frame.sent_off,
            u32::from(frame.phases[0])
                | u32::from(frame.phases[1]) << 8
                | u32::from(frame.half) << 16,
        );
        assert_eq!(
            (rebuilt.phases, rebuilt.half),
            (frame.phases, frame.half),
            "the phase word carries phases and half"
        );
        assert_eq!(frame_mesh(&rebuilt, &all, 1280, 820).verts, a.verts);
    }

    /// The "second-last defender" of the overlay is the engine's offside
    /// line, bit for bit, on every tick the engine has one — whole matches,
    /// both halves, with whoever was sent off.
    #[test]
    fn second_last_defender_is_the_engines_line_bit_for_bit() {
        use fm_match::TickFrame;
        let mut compared = [0_u32; 2];
        for seed in [3, 7, 11] {
            let (db, setup) = demo_match(seed);
            let mut e = MatchEngine::new(&setup, &db);
            let mut hud = Hud::default();
            while !e.is_finished() {
                e.tick_logic();
                hud.observe(&e);
                let state = e.state();
                let mut tick = TickFrame::capture(state);
                tick.observe_ball(state);
                tick.compute_offside(state);
                let snap = e.sample(LodLevel::Full, state.now_ms()).expect("snapshot");
                let frame = decode(&encode(&snap, &hud));
                for attacking in [Side::Home, Side::Away] {
                    let Some(line) = tick.offside_line(attacking) else {
                        continue;
                    };
                    let ours = second_last_defender_x(&frame, attacking.other());
                    assert_eq!(
                        ours.to_bits(),
                        line.to_bits(),
                        "seed {seed} tick {}: {ours} vs {line}",
                        state.tick
                    );
                    compared[usize::from(frame.half)] += 1;
                }
            }
        }
        assert!(
            compared[0] > 10_000 && compared[1] > 10_000,
            "both halves compared: {compared:?}"
        );
    }

    /// The line of the Laws: the second-last defender, unless the ball or
    /// the halfway line is nearer the defended goal.
    #[test]
    fn offside_line_follows_the_laws() {
        let mut frame = frame_at(0);
        // Away (11..22) defends the right goal in the first half: keeper on
        // his line, one defender at 80 m, the rest at 60 m.
        frame.half = 0;
        frame.sent_off = 0;
        for p in &mut frame.players[11..] {
            *p = Vec2::new(60.0, 30.0);
        }
        frame.players[11] = Vec2::new(104.0, 34.0);
        frame.players[12] = Vec2::new(80.0, 30.0);
        frame.ball = fm_core::Vec3::new(70.0, 34.0, 0.0);
        frame.phases = [0, 1];
        assert_eq!(offside_line(&frame), Some((Side::Away, 80.0)));
        // Ball beyond the second-last defender: the ball is the line.
        frame.ball.x = 90.0;
        assert_eq!(offside_line(&frame), Some((Side::Away, 90.0)));
        // Defence pushed into the attackers' half: never past halfway.
        frame.ball.x = 30.0;
        frame.players[12].x = 45.0;
        for p in &mut frame.players[13..] {
            p.x = 40.0;
        }
        assert_eq!(offside_line(&frame), Some((Side::Away, pitch::HALF_LENGTH)));
        // The second-last man sent off: the next one holds the line.
        frame.players[12].x = 80.0;
        frame.ball.x = 50.0;
        for p in &mut frame.players[13..] {
            p.x = 60.0;
        }
        frame.sent_off = 1 << 12;
        assert_eq!(offside_line(&frame), Some((Side::Away, 60.0)));
        frame.sent_off = 0;
        // Transition counts as attacking; the other team attacking mirrors.
        frame.phases = [2, 3];
        assert_eq!(offside_line(&frame), Some((Side::Away, 80.0)));
        for p in &mut frame.players[..11] {
            *p = Vec2::new(40.0, 30.0);
        }
        frame.players[0] = Vec2::new(1.0, 34.0);
        frame.players[1] = Vec2::new(20.0, 30.0);
        frame.ball.x = 30.0;
        frame.phases = [1, 0];
        assert_eq!(offside_line(&frame), Some((Side::Home, 20.0)));
        // Second half: home defends the right goal.
        frame.half = 1;
        for p in &mut frame.players[..11] {
            p.x = pitch::LENGTH - p.x;
        }
        frame.ball.x = 75.0;
        assert_eq!(offside_line(&frame), Some((Side::Home, 85.0)));
        // Set piece: the snapshot does not tell who attacks.
        frame.phases = [4, 4];
        assert_eq!(offside_line(&frame), None);
    }

    /// F3 adds the dashed line and nothing else; no line, no vertices.
    #[test]
    fn offside_overlay_is_a_dashed_line() {
        let mut frame = frame_at(1_234);
        let on = Overlays {
            velocities: None,
            offside: true,
            formation: None,
        };
        frame.phases = [0, 1];
        let plain = frame_mesh(&frame, &PLAIN, 1280, 820);
        let with = frame_mesh(&frame, &on, 1280, 820);
        assert_eq!(with.vertex_count(), plain.vertex_count() + 17 * 6);
        assert!(with
            .verts
            .chunks(6)
            .all(|v| v[0].abs() <= 1.0 && v[1].abs() <= 1.0));
        frame.phases = [4, 4];
        assert_eq!(frame_mesh(&frame, &on, 1280, 820).verts, plain.verts);
    }

    /// F2: moving players get an arrow, standing ones and sent-off ones do
    /// not, and the rest of the mesh is untouched.
    #[test]
    fn velocity_arrows_are_added_for_moving_players_only() {
        let mut frame = frame_at(1_234);
        let plain = frame_mesh(&frame, &PLAIN, 1280, 820);
        let still = [Vec2::ZERO; 22];
        let arrows = |velocities| Overlays {
            velocities: Some(velocities),
            offside: false,
            formation: None,
        };
        assert_eq!(
            frame_mesh(&frame, &arrows(&still), 1280, 820).verts,
            plain.verts,
            "nobody moving: no arrows"
        );
        let mut moving = still;
        moving[3] = Vec2::new(6.0, 0.0);
        moving[15] = Vec2::new(0.0, -4.0);
        let with = frame_mesh(&frame, &arrows(&moving), 1280, 820);
        let per_arrow = (with.vertex_count() - plain.vertex_count()) / 2;
        assert!(per_arrow > 6, "an arrow is a line and a tip");
        assert_eq!(with.vertex_count(), plain.vertex_count() + 2 * per_arrow);
        assert!(with
            .verts
            .chunks(6)
            .all(|v| v[0].abs() <= 1.0 && v[1].abs() <= 1.0));
        // A sent-off player has no arrow either.
        frame.sent_off = 1 << 3;
        let gone = frame_mesh(&frame, &arrows(&moving), 1280, 820);
        frame.sent_off = 0;
        let nobody = frame_mesh(&frame, &PLAIN, 1280, 820).vertex_count();
        assert!(gone.vertex_count() < nobody + 2 * per_arrow);
        assert_eq!(velocities_from_parts(&[0.0; 44]), Some(still));
        assert_eq!(velocities_from_parts(&[]), None);
    }

    /// F4: one segment between neighbours of each sector, in the order they
    /// stand across the pitch; nobody sent off, and never the keeper.
    #[test]
    fn formation_lines_join_each_sector_across_the_pitch() {
        let (db, setup) = demo_match(7);
        let e = MatchEngine::new(&setup, &db);
        let mut roster = [0_u8; 22];
        for (code, p) in roster.iter_mut().zip(&e.state().players) {
            *code = role_code(p.role);
        }
        assert_eq!(roster_from_parts(&roster), Some(roster));
        assert_eq!(roster_from_parts(&roster[..21]), None);
        let mut frame = frame_at(1_234);
        let on = Overlays {
            formation: Some(&roster),
            ..PLAIN
        };
        let plain = frame_mesh(&frame, &PLAIN, 1280, 820).vertex_count();
        let segments = |sent_off: u32| {
            let mut total = 0;
            for start in [0, SIDE] {
                for s in 0..3 {
                    let n = (start..start + SIDE)
                        .filter(|&i| sent_off >> i & 1 == 0 && sector(roster[i]) == Some(s))
                        .count();
                    total += n.saturating_sub(1);
                }
            }
            total
        };
        // Ten outfielders in three sectors: seven segments a team.
        assert_eq!(segments(0), 14);
        let with = frame_mesh(&frame, &on, 1280, 820);
        assert_eq!(with.vertex_count(), plain + 14 * 6);
        assert!(with
            .verts
            .chunks(6)
            .all(|v| v[0].abs() <= 1.0 && v[1].abs() <= 1.0));
        // A defender sent off leaves his line (and is not drawn himself).
        let defender = roster.iter().position(|&c| c == 1).expect("a centre-back");
        frame.sent_off = 1 << defender;
        let gone = frame_mesh(&frame, &PLAIN, 1280, 820).vertex_count();
        assert_eq!(
            frame_mesh(&frame, &on, 1280, 820).vertex_count(),
            gone + 13 * 6
        );
        // The keeper is in no line: moving him moves no line.
        frame.sent_off = 0;
        let mut shapes = Vec::new();
        formation_shapes(&frame, &roster, &mut shapes);
        frame.players[0] = Vec2::new(50.0, 1.0);
        frame.players[11] = Vec2::new(50.0, 67.0);
        let mut moved = Vec::new();
        formation_shapes(&frame, &roster, &mut moved);
        assert_eq!(shapes, moved);
        // Neighbours across the pitch: every line runs with y not falling.
        for shape in &shapes {
            let Shape::Line { a, b, .. } = shape else {
                panic!("only lines");
            };
            assert!(a.y <= b.y);
        }
    }

    /// The sectors are the engine's lines (`Role::line`).
    #[test]
    fn sectors_are_the_role_lines() {
        use fm_match::{Formation, Line};
        for f in Formation::ALL {
            for slot in f.slots() {
                let expected = match slot.role.line() {
                    Line::Goalkeeper => None,
                    Line::Defence => Some(0),
                    Line::Midfield => Some(1),
                    Line::Attack => Some(2),
                };
                assert_eq!(sector(role_code(slot.role)), expected, "{:?}", slot.role);
            }
        }
    }

    #[test]
    fn every_role_has_its_own_label_code() {
        use fm_match::{Formation, Role};
        let mut seen = [false; 10];
        for f in Formation::ALL {
            for slot in f.slots() {
                seen[usize::from(role_code(slot.role))] = true;
            }
        }
        assert_eq!(role_code(Role::Goalkeeper), 0);
        assert_eq!(role_code(Role::Striker), 9);
        assert!(
            seen[0] && seen[1] && seen[9],
            "the formations use the codes"
        );
    }

    #[test]
    fn players_sent_off_are_not_drawn() {
        let mut frame = frame_at(100);
        let all = frame_mesh(&frame, &PLAIN, 1280, 820).vertex_count();
        frame.sent_off = 1 << 5;
        assert!(frame_mesh(&frame, &PLAIN, 1280, 820).vertex_count() < all);
    }
}
