//! wasm-bindgen surface of the engine. Phase 0 exposes `hello()` and the libm
//! parity check so the browser can prove the WASM build matches native.

#![forbid(unsafe_code)]

use wasm_bindgen::prelude::wasm_bindgen;

/// Greeting shown by the bootstrap page; pi comes from libm inside WASM.
#[wasm_bindgen]
#[must_use]
pub fn hello() -> String {
    format!("Hello from Rust (libm: {})", fm_core::pi_libm())
}

/// Number of libm golden lines that differ in this build (0 = parity).
#[wasm_bindgen]
#[must_use]
pub fn libm_golden_mismatches() -> u32 {
    let actual = fm_core::libm_golden::generate();
    let golden = fm_core::libm_golden::GOLDEN;
    let differing = actual
        .lines()
        .zip(golden.lines())
        .filter(|(a, g)| a != g)
        .count();
    let missing = actual.lines().count().abs_diff(golden.lines().count());
    u32::try_from(differing + missing).unwrap_or(u32::MAX)
}

/// WebGL2 smoke test through glow: RGBA of the drawn pixel, or an error
/// message. The page compares it with `webgl2_smoke_expected()`.
///
/// # Errors
/// When WebGL2 is unavailable or the smoke shader fails.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn webgl2_smoke() -> Result<Vec<u8>, String> {
    fm_render::ffi::glow_backend::webgl2_smoke().map(|px| px.to_vec())
}

/// Pixel the smoke shader must produce.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
#[must_use]
pub fn webgl2_smoke_expected() -> Vec<u8> {
    fm_render::ffi::glow_backend::SMOKE_EXPECTED_RGBA.to_vec()
}

/// SPIKE (branch `spike-render`, not for merge): renders one static frame —
/// pitch, the 22 players of a demo match lined up for kick-off (real engine
/// snapshot, no ticks), and the ball on the centre spot — onto `canvas_id`.
///
/// # Errors
/// When the canvas or WebGL2 is unavailable.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn spike_render(canvas_id: &str, width_px: u32, height_px: u32) -> Result<u32, String> {
    let mesh = spike::scene_mesh(width_px, height_px);
    let bg = [0.04, 0.24, 0.12, 1.0];
    fm_render::ffi::glow_backend::draw_mesh(canvas_id, bg, &mesh.verts)?;
    u32::try_from(mesh.vertex_count()).map_err(|_| "mesh too large".into())
}

/// Scene construction for the render spike (safe, testable natively).
pub mod spike {
    use fm_core::pitch::{self, GoalEnd};
    use fm_core::Vec2;
    use fm_match::demo::demo_match;
    use fm_match::{LodLevel, MatchEngine, Side};
    use fm_render::shapes::{tessellate, Mesh, Rgba, Shape, View};

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

    /// The whole frame as a triangle mesh for a `width_px` × `height_px` canvas.
    #[must_use]
    pub fn scene_mesh(width_px: u32, height_px: u32) -> Mesh {
        let mut shapes = Vec::new();
        pitch_shapes(&mut shapes);

        // Real engine state: kick-off line-up, no ticks run.
        let (db, setup) = demo_match(7);
        let engine = MatchEngine::new(&setup, &db);
        let snap = engine
            .sample(LodLevel::Full, 0)
            .expect("Full LOD yields a snapshot");
        for (i, p) in snap.players.iter().enumerate() {
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
            let number = u8::try_from(i % 11 + 1).expect("1..=11");
            shapes.push(Shape::Number {
                centre: p.pos,
                value: number,
                height: 0.9,
                color: Rgba(1.0, 1.0, 1.0, 1.0),
            });
        }
        let ball = snap.ball.xy();
        shapes.push(Shape::Disc {
            centre: ball,
            radius: 0.55,
            color: Rgba(0.0, 0.0, 0.0, 0.85),
        });
        shapes.push(Shape::Disc {
            centre: ball,
            radius: 0.45,
            color: Rgba(1.0, 1.0, 1.0, 1.0),
        });

        #[allow(clippy::cast_precision_loss)]
        let (w, h) = (width_px.max(1) as f32, height_px.max(1) as f32);
        let view = View::fit(
            pitch::CENTRE,
            Vec2::new(pitch::LENGTH + 8.0, pitch::WIDTH + 8.0),
            w,
            h,
        );
        tessellate(&shapes, &view)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn spike_scene_has_pitch_players_and_ball() {
        let mesh = super::spike::scene_mesh(1280, 820);
        // Pitch + 22 players (2 discs + digits each) + ball: thousands of verts.
        assert!(mesh.vertex_count() > 5_000, "{}", mesh.vertex_count());
        assert!(mesh
            .verts
            .chunks(6)
            .all(|v| v[0].abs() <= 1.0 && v[1].abs() <= 1.0));
    }

    #[test]
    fn hello_reports_libm_pi() {
        assert_eq!(super::hello(), "Hello from Rust (libm: 3.141592653589793)");
    }
}
