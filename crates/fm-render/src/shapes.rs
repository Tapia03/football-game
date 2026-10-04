//! Minimal safe draw list (spec Fase 6, 6A): shapes in pitch metres →
//! coloured triangles in clip space. The full `Renderer2D` / `DrawList`
//! contract lands later in Phase 6.

#![allow(clippy::many_single_char_names)] // geometry: a, b, c, d, p, q, u, v

use fm_core::{math, Vec2};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba(pub f32, pub f32, pub f32, pub f32);

impl Rgba {
    #[must_use]
    pub fn hex(rgb: u32) -> Self {
        let c = |shift: u32| f32::from(u8::try_from((rgb >> shift) & 0xFF).unwrap_or(0)) / 255.0;
        Self(c(16), c(8), c(0), 1.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    /// Axis-aligned filled rectangle `min`..`max`.
    Rect { min: Vec2, max: Vec2, color: Rgba },
    /// Segment of the given width.
    Line {
        a: Vec2,
        b: Vec2,
        width: f32,
        color: Rgba,
    },
    /// Filled disc.
    Disc {
        centre: Vec2,
        radius: f32,
        color: Rgba,
    },
    /// Circle outline.
    Ring {
        centre: Vec2,
        radius: f32,
        width: f32,
        color: Rgba,
    },
    /// Number (0..=99) drawn as 7-segment digits centred on `centre`.
    Number {
        centre: Vec2,
        value: u8,
        height: f32,
        color: Rgba,
    },
}

/// Maps pitch metres to clip space, keeping aspect ratio, with a margin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    centre: Vec2,
    sx: f32,
    sy: f32,
}

impl View {
    /// Fits `world` (width × height metres, centred on `centre`) into a
    /// `px_w` × `px_h` canvas.
    #[must_use]
    pub fn fit(centre: Vec2, world: Vec2, px_w: f32, px_h: f32) -> Self {
        let px_per_m = (px_w / world.x).min(px_h / world.y);
        Self {
            centre,
            sx: 2.0 * px_per_m / px_w,
            sy: 2.0 * px_per_m / px_h,
        }
    }

    fn clip(&self, p: Vec2) -> Vec2 {
        Vec2::new(
            (p.x - self.centre.x) * self.sx,
            (p.y - self.centre.y) * self.sy,
        )
    }
}

/// Interleaved vertices `[x, y, r, g, b, a]` in clip space.
pub struct Mesh {
    pub verts: Vec<f32>,
}

impl Mesh {
    fn tri(&mut self, view: &View, a: Vec2, b: Vec2, c: Vec2, col: Rgba) {
        for p in [a, b, c] {
            let q = view.clip(p);
            self.verts
                .extend_from_slice(&[q.x, q.y, col.0, col.1, col.2, col.3]);
        }
    }

    fn quad(&mut self, view: &View, a: Vec2, b: Vec2, c: Vec2, d: Vec2, col: Rgba) {
        self.tri(view, a, b, c, col);
        self.tri(view, a, c, d, col);
    }

    fn line(&mut self, view: &View, a: Vec2, b: Vec2, w: f32, col: Rgba) {
        let dir = (b - a).normalize();
        let n = Vec2::new(-dir.y, dir.x) * (w / 2.0);
        self.quad(view, a + n, b + n, b - n, a - n, col);
    }

    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.verts.len() / 6
    }
}

const SEGMENTS: usize = 32;

fn unit_circle(i: usize) -> Vec2 {
    #[allow(clippy::cast_precision_loss)]
    let a = math::TAU * i as f32 / SEGMENTS as f32;
    Vec2::from_angle(a)
}

/// 7-segment layout (a b c d e f g) per digit.
const DIGITS: [u8; 10] = [
    0b011_1111, 0b000_0110, 0b101_1011, 0b100_1111, 0b110_0110, 0b110_1101, 0b111_1101, 0b000_0111,
    0b111_1111, 0b110_1111,
];

fn digit(mesh: &mut Mesh, view: &View, centre: Vec2, h: f32, d: u8, col: Rgba) {
    let w = h * 0.55;
    let t = h * 0.14;
    let (l, r) = (centre.x - w / 2.0, centre.x + w / 2.0);
    let (bot, mid, top) = (centre.y - h / 2.0, centre.y, centre.y + h / 2.0);
    let segs = [
        (Vec2::new(l, top), Vec2::new(r, top)), // a
        (Vec2::new(r, top), Vec2::new(r, mid)), // b
        (Vec2::new(r, mid), Vec2::new(r, bot)), // c
        (Vec2::new(l, bot), Vec2::new(r, bot)), // d
        (Vec2::new(l, mid), Vec2::new(l, bot)), // e
        (Vec2::new(l, top), Vec2::new(l, mid)), // f
        (Vec2::new(l, mid), Vec2::new(r, mid)), // g
    ];
    let bits = DIGITS[usize::from(d % 10)];
    for (i, (a, b)) in segs.iter().enumerate() {
        if bits & (1 << i) != 0 {
            mesh.line(view, *a, *b, t, col);
        }
    }
}

/// Tessellates `shapes` (in draw order) for `view`.
#[must_use]
#[allow(clippy::many_single_char_names)] // geometry: a, b, c, p, q, u, v
pub fn tessellate(shapes: &[Shape], view: &View) -> Mesh {
    let mut m = Mesh { verts: Vec::new() };
    for s in shapes {
        match *s {
            Shape::Rect { min, max, color } => m.quad(
                view,
                min,
                Vec2::new(max.x, min.y),
                max,
                Vec2::new(min.x, max.y),
                color,
            ),
            Shape::Line { a, b, width, color } => m.line(view, a, b, width, color),
            Shape::Disc {
                centre,
                radius,
                color,
            } => {
                for i in 0..SEGMENTS {
                    let p = centre + unit_circle(i) * radius;
                    let q = centre + unit_circle(i + 1) * radius;
                    m.tri(view, centre, p, q, color);
                }
            }
            Shape::Ring {
                centre,
                radius,
                width,
                color,
            } => {
                let (ri, ro) = (radius - width / 2.0, radius + width / 2.0);
                for i in 0..SEGMENTS {
                    let (u, v) = (unit_circle(i), unit_circle(i + 1));
                    m.quad(
                        view,
                        centre + u * ri,
                        centre + u * ro,
                        centre + v * ro,
                        centre + v * ri,
                        color,
                    );
                }
            }
            Shape::Number {
                centre,
                value,
                height,
                color,
            } => {
                if value >= 10 {
                    let off = height * 0.4;
                    digit(
                        &mut m,
                        view,
                        Vec2::new(centre.x - off, centre.y),
                        height,
                        value / 10,
                        color,
                    );
                    digit(
                        &mut m,
                        view,
                        Vec2::new(centre.x + off, centre.y),
                        height,
                        value % 10,
                        color,
                    );
                } else {
                    digit(&mut m, view, centre, height, value, color);
                }
            }
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::{tessellate, Rgba, Shape, View};
    use fm_core::Vec2;

    #[test]
    fn tessellation_counts_and_clip_space() {
        let view = View::fit(Vec2::new(52.5, 34.0), Vec2::new(113.0, 76.0), 1130.0, 760.0);
        let white = Rgba::hex(0xFF_FFFF);
        let m = tessellate(
            &[
                Shape::Rect {
                    min: Vec2::new(0.0, 0.0),
                    max: Vec2::new(105.0, 68.0),
                    color: white,
                },
                Shape::Disc {
                    centre: Vec2::new(52.5, 34.0),
                    radius: 1.0,
                    color: white,
                },
                Shape::Number {
                    centre: Vec2::new(10.0, 10.0),
                    value: 8,
                    height: 1.0,
                    color: white,
                },
            ],
            &view,
        );
        // quad 6 + disc 32×3 + digit "8" 7 segments × 6.
        assert_eq!(m.vertex_count(), 6 + 96 + 42);
        for v in m.verts.chunks(6) {
            assert!(
                v[0].abs() <= 1.0 && v[1].abs() <= 1.0,
                "outside clip: {v:?}"
            );
        }
    }
}
