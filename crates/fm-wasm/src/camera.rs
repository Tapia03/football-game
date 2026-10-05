//! Camera (spec Fase 6, 6D): which window of the pitch the mesh is drawn
//! through. The mesh is tessellated straight into clip space from a `View`,
//! so the camera is that view with other numbers — nothing on the GPU,
//! nothing in the engine. Pure functions: the main thread keeps the mode
//! and the current camera, and asks here for the target and for each step
//! towards it.

use fm_core::{math, pitch, Vec2};
use fm_render::shapes::View;

/// Margin around the pitch in the full view (m, each side).
const MARGIN: f32 = 4.0;
/// Window of the full view (m): the pitch and its margin. Every other mode
/// is this window scaled by `1 / zoom`, so the aspect never changes.
const FULL_WIDTH: f32 = pitch::LENGTH + 2.0 * MARGIN;
const FULL_HEIGHT: f32 = pitch::WIDTH + 2.0 * MARGIN;

/// Zoom of `HalfPitch`: half the length in view (56.5 m), as on television.
pub const HALF_ZOOM: f32 = 2.0;
/// Zoom of `Tactical`: a 125 m window, the whole pitch with room around it.
pub const TACTICAL_ZOOM: f32 = FULL_WIDTH / 125.0;
/// `HalfPitch` lets the ball move inside this central share of the window
/// before the camera follows.
pub const DEAD_ZONE: f32 = 0.4;
/// Time constant of the blend towards the target (ms of real time).
pub const SMOOTH_MS: f32 = 100.0;
/// Closer to the target than this, the camera lands on it exactly: 1 cm
/// and 0.1% of zoom.
const SNAP_M: f32 = 0.01;
const SNAP_ZOOM: f32 = 0.001;

/// Centre of the window on the pitch (m) and zoom relative to the full view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub centre: Vec2,
    pub zoom: f32,
}

impl Camera {
    /// The whole pitch: the view every frame was drawn with before 6D.
    pub const FULL: Self = Self {
        centre: pitch::CENTRE,
        zoom: 1.0,
    };

    /// From what crosses the wasm boundary; anything that is not a usable
    /// camera reads as the full view.
    #[must_use]
    pub fn from_parts(x: f32, y: f32, zoom: f32) -> Self {
        if x.is_finite() && y.is_finite() && zoom.is_finite() && zoom >= 0.1 {
            Self {
                centre: Vec2::new(x, y),
                zoom,
            }
        } else {
            Self::FULL
        }
    }

    #[must_use]
    pub const fn parts(self) -> [f32; 3] {
        [self.centre.x, self.centre.y, self.zoom]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    FullPitch,
    /// Zoomed in, following the ball with a dead zone.
    HalfPitch,
    /// Zoomed out a little (the page also turns labels and formation lines
    /// on while in this mode).
    Tactical,
}

impl Mode {
    /// 0 `FullPitch`, 1 `HalfPitch`, 2 `Tactical`; anything else is the
    /// full pitch.
    #[must_use]
    pub const fn from_code(code: u32) -> Self {
        match code {
            1 => Self::HalfPitch,
            2 => Self::Tactical,
            _ => Self::FullPitch,
        }
    }
}

/// How `camera` maps the pitch into a `width_px` × `height_px` canvas: the
/// one view both the mesh and anything laid over the canvas use.
#[must_use]
pub fn view(camera: Camera, width_px: u32, height_px: u32) -> View {
    #[allow(clippy::cast_precision_loss)] // canvas sizes ≪ 2^24
    let (w, h) = (width_px.max(1) as f32, height_px.max(1) as f32);
    View::fit(
        camera.centre,
        Vec2::new(FULL_WIDTH / camera.zoom, FULL_HEIGHT / camera.zoom),
        w,
        h,
    )
}

/// Where the camera of `mode` wants to be for the ball at `ball`, given
/// the centre it wanted last frame (`previous`: the dead zone has memory).
///
/// `HalfPitch`: the centre stays while the ball is inside the central
/// `DEAD_ZONE` of the window and follows just enough to keep it there
/// otherwise; the window never leaves the full view.
#[must_use]
pub fn target(mode: Mode, ball: Vec2, previous: Vec2) -> Camera {
    match mode {
        Mode::FullPitch => Camera::FULL,
        Mode::Tactical => Camera {
            centre: pitch::CENTRE,
            zoom: TACTICAL_ZOOM,
        },
        Mode::HalfPitch => {
            // Half extents of the window, of its dead zone, and of the
            // range the centre may take.
            let half = Vec2::new(FULL_WIDTH, FULL_HEIGHT) * (0.5 / HALF_ZOOM);
            let dead = half * DEAD_ZONE;
            let range = Vec2::new(FULL_WIDTH, FULL_HEIGHT) * 0.5 - half;
            let axis = |ball: f32, previous: f32, dead: f32, mid: f32, range: f32| {
                previous
                    .clamp(ball - dead, ball + dead)
                    .clamp(mid - range, mid + range)
            };
            Camera {
                centre: Vec2::new(
                    axis(ball.x, previous.x, dead.x, pitch::CENTRE.x, range.x),
                    axis(ball.y, previous.y, dead.y, pitch::CENTRE.y, range.y),
                ),
                zoom: HALF_ZOOM,
            }
        }
    }
}

/// The camera `dt_ms` of real time later, on its way from `current` to
/// `target`: an exponential blend (the same at any frame rate), landing
/// exactly on the target once within a centimetre and 0.1% of zoom.
#[must_use]
pub fn step(current: Camera, target: Camera, dt_ms: f32) -> Camera {
    // Share of the remaining way covered in `dt_ms`.
    let k = if dt_ms > 0.0 {
        1.0 - math::exp(-dt_ms / SMOOTH_MS)
    } else {
        0.0
    };
    let next = Camera {
        centre: current.centre + (target.centre - current.centre) * k,
        zoom: current.zoom + (target.zoom - current.zoom) * k,
    };
    if next.centre.distance(target.centre) < SNAP_M
        && (next.zoom - target.zoom).abs() < SNAP_ZOOM * target.zoom
    {
        target
    } else {
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real time for `from` to land exactly on `to` at `fps` (ms).
    fn settle_ms(from: Camera, to: Camera, fps: f32) -> f32 {
        let dt = 1_000.0 / fps;
        let mut camera = from;
        let mut t = 0.0;
        while camera != to {
            camera = step(camera, to, dt);
            t += dt;
            assert!(t < 10_000.0, "never settles");
        }
        t
    }

    /// The full camera is the view every frame had before 6D, bit for bit.
    #[test]
    fn full_camera_is_the_old_pitch_view() {
        for (w, h) in [(1280, 820), (1560, 1050), (375, 600)] {
            #[allow(clippy::cast_precision_loss)]
            let old = View::fit(
                pitch::CENTRE,
                Vec2::new(pitch::LENGTH + 8.0, pitch::WIDTH + 8.0),
                w as f32,
                h as f32,
            );
            assert_eq!(view(Camera::FULL, w, h).params(), old.params());
        }
        assert_eq!(Camera::from_parts(52.5, 34.0, 1.0), Camera::FULL);
        assert_eq!(Camera::from_parts(f32::NAN, 34.0, 1.0), Camera::FULL);
        assert_eq!(Camera::from_parts(10.0, 10.0, 0.0), Camera::FULL);
        assert_eq!(
            Camera::from_parts(10.0, 20.0, 2.0).parts(),
            [10.0, 20.0, 2.0]
        );
    }

    #[test]
    fn fixed_modes_look_at_the_centre_of_the_pitch() {
        let anywhere = Vec2::new(3.0, 60.0);
        assert_eq!(target(Mode::FullPitch, anywhere, anywhere), Camera::FULL);
        let tactical = target(Mode::Tactical, anywhere, anywhere);
        assert_eq!(tactical.centre, pitch::CENTRE);
        // A 125 m window: the whole pitch fits with room to spare.
        assert!((FULL_WIDTH / tactical.zoom - 125.0).abs() < 1e-3);
        assert_eq!(Mode::from_code(0), Mode::FullPitch);
        assert_eq!(Mode::from_code(1), Mode::HalfPitch);
        assert_eq!(Mode::from_code(2), Mode::Tactical);
        assert_eq!(Mode::from_code(9), Mode::FullPitch);
    }

    /// The ball moves freely in the central 40% of the window; beyond it
    /// the centre follows just enough to keep the ball on the edge of that
    /// zone. No trembling: a ball jittering inside leaves the target alone.
    #[test]
    fn half_pitch_follows_the_ball_with_a_dead_zone() {
        let centre = pitch::CENTRE;
        // Window 56.5 × 38 m: the dead zone is ±11.3 m × ±7.6 m.
        let half = target(Mode::HalfPitch, centre, centre);
        assert_eq!((half.centre, half.zoom), (centre, HALF_ZOOM));
        let mut previous = centre;
        for i in 0..200_u8 {
            let jitter = Vec2::new(f32::from(i % 7) - 3.0, f32::from(i % 5) - 2.0) * 3.0;
            let t = target(Mode::HalfPitch, centre + jitter, previous);
            assert_eq!(t.centre, centre, "ball inside the dead zone: step {i}");
            previous = t.centre;
        }
        // 15 m to the right: 3.7 m past the dead zone, the centre moves 3.7.
        let t = target(Mode::HalfPitch, centre + Vec2::new(15.0, 0.0), centre);
        assert!(
            (t.centre.x - (centre.x + 3.7)).abs() < 1e-4,
            "{}",
            t.centre.x
        );
        assert!((t.centre.y - centre.y).abs() < f32::EPSILON);
        // The memory is the previous target: coming back 5 m moves nothing.
        let back = target(Mode::HalfPitch, centre + Vec2::new(10.0, 0.0), t.centre);
        assert_eq!(back.centre, t.centre);
        // Same inputs, same target.
        assert_eq!(
            target(Mode::HalfPitch, Vec2::new(70.0, 20.0), centre),
            target(Mode::HalfPitch, Vec2::new(70.0, 20.0), centre)
        );
    }

    /// The window never leaves the full view: at the four borders the
    /// camera stops and the ball goes on to the edge.
    #[test]
    fn half_pitch_window_stays_inside_the_full_view() {
        let corners = [
            Vec2::new(0.0, 0.0),
            Vec2::new(pitch::LENGTH, 0.0),
            Vec2::new(0.0, pitch::WIDTH),
            Vec2::new(pitch::LENGTH, pitch::WIDTH),
        ];
        for ball in corners {
            let t = target(Mode::HalfPitch, ball, pitch::CENTRE);
            let (hw, hh) = (FULL_WIDTH / (2.0 * t.zoom), FULL_HEIGHT / (2.0 * t.zoom));
            assert!(
                t.centre.x - hw >= -MARGIN - 1e-4,
                "{ball:?}: {:?}",
                t.centre
            );
            assert!(t.centre.x + hw <= pitch::LENGTH + MARGIN + 1e-4);
            assert!(t.centre.y - hh >= -MARGIN - 1e-4);
            assert!(t.centre.y + hh <= pitch::WIDTH + MARGIN + 1e-4);
            // …and the ball in the corner is still inside the window.
            assert!((ball.x - t.centre.x).abs() <= hw && (ball.y - t.centre.y).abs() <= hh);
        }
    }

    /// Blend, not cut: monotonic towards the target, exactly on it at the
    /// end, and settled within a second for the longest switch there is
    /// (full view to a corner of the half-pitch range), at any frame rate.
    #[test]
    fn camera_settles_on_the_target_within_a_second() {
        let corner = target(Mode::HalfPitch, Vec2::new(0.0, 0.0), pitch::CENTRE);
        let tactical = target(Mode::Tactical, pitch::CENTRE, pitch::CENTRE);
        for (from, to) in [
            (Camera::FULL, corner),
            (corner, Camera::FULL),
            (corner, tactical),
            (tactical, corner),
            (Camera::FULL, tactical),
        ] {
            for fps in [30.0, 60.0, 144.0] {
                let ms = settle_ms(from, to, fps);
                assert!(ms <= 1_000.0, "{from:?} -> {to:?} at {fps} fps: {ms} ms");
            }
            // Never further from the target than a step before.
            let mut camera = from;
            let mut distance = camera.centre.distance(to.centre);
            for _ in 0..120 {
                camera = step(camera, to, 1_000.0 / 60.0);
                let d = camera.centre.distance(to.centre);
                assert!(d <= distance);
                distance = d;
            }
            assert_eq!(camera, to);
        }
        // On the target, or with no time passed, nothing moves.
        assert_eq!(step(corner, corner, 16.0), corner);
        assert_eq!(step(Camera::FULL, corner, 0.0), Camera::FULL);
    }

    /// The same blend whatever the frame rate: after 300 ms the camera is
    /// in the same place at 30, 60 and 144 fps (to a few centimetres).
    #[test]
    fn blend_does_not_depend_on_the_frame_rate() {
        let to = target(Mode::HalfPitch, Vec2::new(0.0, 0.0), pitch::CENTRE);
        let at_300 = |fps: f32| {
            let frames = (0.3 * fps).round();
            let mut camera = Camera::FULL;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            for _ in 0..frames as u32 {
                camera = step(camera, to, 300.0 / frames);
            }
            camera
        };
        let reference = at_300(60.0);
        // 95% of the way after three time constants.
        let done =
            1.0 - reference.centre.distance(to.centre) / Camera::FULL.centre.distance(to.centre);
        assert!((done - 0.95).abs() < 0.01, "{done}");
        for fps in [30.0, 144.0] {
            let c = at_300(fps);
            assert!(c.centre.distance(reference.centre) < 0.05, "{fps} fps");
            assert!((c.zoom - reference.zoom).abs() < 0.005);
        }
    }
}
