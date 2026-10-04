//! wasm-bindgen surface of the engine. Phase 0 exposes `hello()` and the libm
//! parity check so the browser can prove the WASM build matches native.

#![forbid(unsafe_code)]

pub mod mesh;
pub mod sab;

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

/// The match, living in the engine worker (spec Fase 6, 6A): the only
/// `MatchEngine` of the page. It advances with match time and publishes
/// its snapshots in the `SharedArrayBuffer` ring (`sab`); the main thread
/// only reads that buffer.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct EngineHost {
    engine: fm_match::MatchEngine,
    match_ms: f64,
    /// Snapshots written so far (the ring's write sequence).
    seq: u32,
    ints: js_sys::Int32Array,
    words: js_sys::Uint32Array,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl EngineHost {
    /// Demo match `seed`, publishing into `buffer` (at least
    /// `sab::BUFFER_BYTES` long). Writes the header and the kick-off
    /// snapshot.
    ///
    /// # Errors
    /// When the buffer is too small for the ring.
    #[wasm_bindgen(constructor)]
    pub fn new(seed: u32, buffer: &js_sys::SharedArrayBuffer) -> Result<EngineHost, String> {
        if buffer.byte_length() < sab::BUFFER_BYTES {
            return Err(format!(
                "snapshot buffer has {} bytes, needs {}",
                buffer.byte_length(),
                sab::BUFFER_BYTES
            ));
        }
        let (db, setup) = fm_match::demo::demo_match(u64::from(seed));
        let mut host = Self {
            engine: fm_match::MatchEngine::new(&setup, &db),
            match_ms: 0.0,
            seq: 0,
            ints: js_sys::Int32Array::new(buffer),
            words: js_sys::Uint32Array::new(buffer),
        };
        let header = [
            (sab::H_MAGIC, sab::MAGIC),
            (sab::H_SEQ, 0),
            (sab::H_SLOTS, sab::RING_SLOTS),
            (sab::H_SLOT_BYTES, sab::SLOT_BYTES),
            (sab::H_SEED, seed),
        ];
        for (i, v) in header {
            host.words.set_index(i, v);
        }
        host.publish_tick();
        Ok(host)
    }

    /// Advances match time by `match_ms` milliseconds, running every
    /// logical tick it covers and publishing each one's 60 Hz samples, then
    /// publishes the match clock.
    pub fn advance(&mut self, match_ms: f64) {
        if self.engine.is_finished() || match_ms <= 0.0 {
            return;
        }
        self.match_ms += match_ms;
        while !self.engine.is_finished()
            && f64::from(self.engine.state().now_ms() + fm_match::LOGICAL_DT_MS) <= self.match_ms
        {
            self.engine.tick_logic();
            self.publish_tick();
        }
        if self.engine.is_finished() {
            self.match_ms = f64::from(self.engine.state().now_ms());
            let _ = js_sys::Atomics::store(&self.ints, sab::H_STATE, sab::STATE_FINISHED);
        }
        #[allow(clippy::cast_possible_truncation)] // < 2^31 ms: a match is 5.4e6 ms
        let _ = js_sys::Atomics::store(&self.ints, sab::H_NOW, self.match_ms as i32);
    }

    #[must_use]
    pub fn finished(&self) -> bool {
        self.engine.is_finished()
    }

    /// Logical time of the match (ms since kick-off).
    #[must_use]
    pub fn clock_ms(&self) -> u32 {
        self.engine.state().now_ms()
    }

    /// Snapshots published so far.
    #[must_use]
    pub fn sequence(&self) -> u32 {
        self.seq
    }
}

#[cfg(target_arch = "wasm32")]
impl EngineHost {
    /// Publishes the snapshots of the tick just run: its whole 100 ms at
    /// 60 Hz, ahead of time (see `sab::tick_frames`).
    fn publish_tick(&mut self) {
        for words in sab::tick_frames(&self.engine) {
            self.publish(&words);
        }
    }

    /// Writes one snapshot into the next ring slot, then makes it visible
    /// by storing the new sequence: a reader never sees a half-written
    /// slot as the latest one.
    fn publish(&mut self, words: &[u32; sab::SLOT_WORDS]) {
        let at = sab::slot_offset(self.seq);
        self.words
            .subarray(at, at + sab::SLOT_WORDS_U32)
            .copy_from(words);
        self.seq = self.seq.wrapping_add(1);
        #[allow(clippy::cast_possible_wrap)] // a counter: wrapping is fine
        let _ = js_sys::Atomics::store(&self.ints, sab::H_SEQ, self.seq as i32);
    }
}

/// Triangle list of one frame (`[x, y, r, g, b, a]` per vertex, clip
/// space) for a `width_px` × `height_px` canvas. Pure: the main thread
/// passes what it read from the snapshot ring and interpolated — `xy` is
/// x0, y0, x1, y1, … for the 22 players.
#[wasm_bindgen]
#[must_use]
pub fn frame_mesh_vertices(
    xy: &[f32],
    ball_x: f32,
    ball_y: f32,
    ball_z: f32,
    sent_off: u32,
    width_px: u32,
    height_px: u32,
) -> Vec<f32> {
    let frame = mesh::frame_from_parts(xy, [ball_x, ball_y, ball_z], sent_off);
    mesh::frame_mesh(&frame, width_px, height_px).verts
}

/// The canvas the match is drawn on (main thread). It owns the WebGL2
/// objects and nothing of the match: every frame is handed in.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct MatchCanvas {
    renderer: fm_render::ffi::glow_backend::GlowMeshRenderer,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl MatchCanvas {
    /// Binds to the canvas `canvas_id`.
    ///
    /// # Errors
    /// When the canvas or WebGL2 is unavailable.
    #[wasm_bindgen(constructor)]
    pub fn new(canvas_id: &str) -> Result<MatchCanvas, String> {
        Ok(Self {
            renderer: fm_render::ffi::glow_backend::GlowMeshRenderer::new(canvas_id)?,
        })
    }

    /// Draws one frame (see `frame_mesh_vertices`). Returns the vertex count.
    ///
    /// # Errors
    /// When drawing fails.
    pub fn draw(
        &self,
        xy: &[f32],
        ball_x: f32,
        ball_y: f32,
        ball_z: f32,
        sent_off: u32,
    ) -> Result<u32, String> {
        let (w, h) = self.renderer.size();
        let frame = mesh::frame_from_parts(xy, [ball_x, ball_y, ball_z], sent_off);
        let mesh = mesh::frame_mesh(&frame, w, h);
        self.renderer.draw(mesh::BACKGROUND, &mesh.verts)?;
        u32::try_from(mesh.vertex_count()).map_err(|_| "mesh too large".into())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn hello_reports_libm_pi() {
        assert_eq!(super::hello(), "Hello from Rust (libm: 3.141592653589793)");
    }
}
