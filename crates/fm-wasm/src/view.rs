//! Fase 6 v0 (spec): a demo match played in real time × speed and drawn as
//! one triangle mesh per frame. Safe code, testable natively; the WebGL
//! side lives in `fm_render::ffi::glow_backend`.

use fm_core::pitch::{self, GoalEnd};
use fm_core::Vec2;
use fm_match::demo::demo_match;
use fm_match::{LodLevel, MatchEngine, MatchSnapshot, Side, LOGICAL_DT_MS};
use fm_render::shapes::{tessellate, Mesh, Rgba, Shape, View};

/// Clear colour behind the pitch.
pub const BACKGROUND: [f32; 4] = [0.04, 0.24, 0.12, 1.0];

/// A demo match advanced by match time; frames are sampled analytically
/// between logical ticks (the engine's `sample`), so motion is smooth at any
/// frame rate without extra interpolation state.
pub struct Playback {
    engine: MatchEngine,
    match_ms: f64,
}

impl Playback {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let (db, setup) = demo_match(seed);
        Self {
            engine: MatchEngine::new(&setup, &db),
            match_ms: 0.0,
        }
    }

    /// Advances match time by `ms`, running every logical tick it covers.
    pub fn advance(&mut self, ms: f64) {
        if self.engine.is_finished() || ms <= 0.0 {
            return;
        }
        self.match_ms += ms;
        while !self.engine.is_finished()
            && f64::from(self.engine.state().now_ms() + LOGICAL_DT_MS) <= self.match_ms
        {
            self.engine.tick_logic();
        }
        if self.engine.is_finished() {
            self.match_ms = f64::from(self.engine.state().now_ms());
        }
    }

    /// Time the frame is sampled at: inside the current tick interval.
    #[must_use]
    pub fn sample_ms(&self) -> u32 {
        let now = self.engine.state().now_ms();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // ≥ 0, < 2^32
        let t = self.match_ms as u32;
        t.clamp(now, now + LOGICAL_DT_MS - 1)
    }

    /// Interpolated state at `sample_ms`.
    ///
    /// # Panics
    /// Never: `Full` always yields a snapshot.
    #[must_use]
    pub fn snapshot(&self) -> MatchSnapshot {
        self.engine
            .sample(LodLevel::Full, self.sample_ms())
            .expect("Full LOD yields a snapshot")
    }

    #[must_use]
    pub fn score(&self) -> [u8; 2] {
        let t = &self.engine.state().teams;
        [t[0].score, t[1].score]
    }

    #[must_use]
    pub fn finished(&self) -> bool {
        self.engine.is_finished()
    }
}

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
/// canvas.
///
/// # Panics
/// Never: shirt numbers are 1..=11.
#[must_use]
pub fn frame_mesh(snap: &MatchSnapshot, width_px: u32, height_px: u32) -> Mesh {
    let mut shapes = Vec::with_capacity(160);
    pitch_shapes(&mut shapes);
    for (i, p) in snap.players.iter().enumerate() {
        if p.sent_off {
            continue;
        }
        let (fill, ring) = match p.side {
            Side::Home => (Rgba::hex(0xD6_3031), Rgba::hex(0x7A_1414)),
            Side::Away => (Rgba::hex(0x09_84E3), Rgba::hex(0x05_3F6E)),
        };
        shapes.push(Shape::Disc {
            centre: p.pos,
            radius: 1.15,
            color: ring,
        });
        shapes.push(Shape::Disc {
            centre: p.pos,
            radius: 0.95,
            color: fill,
        });
        shapes.push(Shape::Number {
            centre: p.pos,
            value: u8::try_from(i % 11 + 1).expect("1..=11"),
            height: 0.9,
            color: Rgba(1.0, 1.0, 1.0, 1.0),
        });
    }
    // The ball grows a little with height so lofted balls read as airborne.
    let ball = snap.ball.xy();
    let lift = (snap.ball.z * 0.12).min(0.5);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_runs_the_ticks_it_covers_and_stops_at_full_time() {
        let mut pb = Playback::new(7);
        pb.advance(1_050.0);
        assert_eq!(pb.sample_ms(), 1_050);
        assert_eq!(pb.snapshot().tick, 10, "10 ticks of 100 ms covered");
        // Whole match in big steps: ends exactly at full time.
        for _ in 0..1_000 {
            pb.advance(10_000.0);
        }
        assert!(pb.finished());
        assert_eq!(pb.sample_ms(), 90 * 60 * 1_000);
        pb.advance(5_000.0);
        assert_eq!(pb.sample_ms(), 90 * 60 * 1_000, "nothing after full time");
    }

    #[test]
    fn frames_interpolate_between_ticks() {
        let mut pb = Playback::new(7);
        pb.advance(30_000.0);
        let a = pb.snapshot();
        pb.advance(40.0);
        let b = pb.snapshot();
        assert_eq!(a.tick, b.tick, "same logical tick");
        assert!(
            a.players
                .iter()
                .zip(&b.players)
                .any(|(p, q)| p.pos != q.pos),
            "players move between ticks"
        );
    }

    #[test]
    fn frame_mesh_stays_in_clip_space() {
        let mut pb = Playback::new(7);
        pb.advance(600_000.0);
        let mesh = frame_mesh(&pb.snapshot(), 1280, 820);
        assert!(mesh.vertex_count() > 5_000, "{}", mesh.vertex_count());
        assert!(mesh
            .verts
            .chunks(6)
            .all(|v| v[0].abs() <= 1.0 && v[1].abs() <= 1.0));
    }
}
