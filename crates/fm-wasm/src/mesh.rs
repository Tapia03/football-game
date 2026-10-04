//! Frame mesh (spec Fase 6, 6A): the pitch, the 22 players and the ball of
//! one decoded snapshot as a triangle list in clip space. Pure functions of
//! their arguments — no engine and no state: the main thread reads the
//! snapshot ring, interpolates, and hands the frame in.

use fm_core::pitch::{self, GoalEnd};
use fm_core::Vec2;
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

/// The whole frame (pitch, players, ball) for a `width_px` × `height_px`
/// canvas. Players sent off are not drawn.
///
/// # Panics
/// Never: shirt numbers are 1..=11.
#[must_use]
pub fn frame_mesh(frame: &Frame, width_px: u32, height_px: u32) -> Mesh {
    let mut shapes = Vec::with_capacity(160);
    pitch_shapes(&mut shapes);
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
    #[allow(clippy::cast_precision_loss)] // canvas sizes ≪ 2^24
    let (w, h) = (width_px.max(1) as f32, height_px.max(1) as f32);
    let view = View::fit(
        pitch::CENTRE,
        Vec2::new(pitch::LENGTH + 8.0, pitch::WIDTH + 8.0),
        w,
        h,
    );
    tessellate(&shapes, &view)
}

/// A frame from what the main thread holds after interpolating: `xy` is
/// x0, y0, x1, y1, … for the 22 players (missing values read as 0).
#[must_use]
pub fn frame_from_parts(xy: &[f32], ball: [f32; 3], sent_off: u32) -> Frame {
    let mut players = [Vec2::ZERO; 22];
    for (i, p) in players.iter_mut().enumerate() {
        let at = |k: usize| xy.get(2 * i + k).copied().unwrap_or(0.0);
        *p = Vec2::new(at(0), at(1));
    }
    Frame {
        tick: 0,
        t_ms: 0,
        score: [0, 0],
        phases: [0, 0],
        sent_off,
        ball: fm_core::Vec3::new(ball[0], ball[1], ball[2]),
        players,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sab::{decode, encode};
    use fm_match::demo::demo_match;
    use fm_match::{LodLevel, MatchEngine};

    fn frame_at(ticks: u32) -> Frame {
        let (db, setup) = demo_match(7);
        let mut e = MatchEngine::new(&setup, &db);
        for _ in 0..ticks {
            e.tick_logic();
        }
        let snap = e
            .sample(LodLevel::Full, e.state().now_ms())
            .expect("snapshot");
        decode(&encode(&snap))
    }

    #[test]
    fn frame_mesh_stays_in_clip_space() {
        let mesh = frame_mesh(&frame_at(6_000), 1280, 820);
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
        let a = frame_mesh(&frame, 1280, 820);
        let b = frame_mesh(&frame, 1280, 820);
        assert_eq!(a.verts, b.verts);
        // And the parts the main thread passes rebuild the same frame.
        let xy: Vec<f32> = frame.players.iter().flat_map(|p| [p.x, p.y]).collect();
        let rebuilt = frame_from_parts(
            &xy,
            [frame.ball.x, frame.ball.y, frame.ball.z],
            frame.sent_off,
        );
        assert_eq!(frame_mesh(&rebuilt, 1280, 820).verts, a.verts);
    }

    #[test]
    fn players_sent_off_are_not_drawn() {
        let mut frame = frame_at(100);
        let all = frame_mesh(&frame, 1280, 820).vertex_count();
        frame.sent_off = 1 << 5;
        assert!(frame_mesh(&frame, 1280, 820).vertex_count() < all);
    }
}
