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
    /// Cards and held-ball ticks so far (the HUD words of each snapshot).
    hud: sab::Hud,
    /// Tactical commands applied so far (`sab::COMMAND_WORDS` each): with
    /// the seed, the whole match.
    commands: Vec<u32>,
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
            hud: sab::Hud::default(),
            commands: Vec::new(),
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
            self.step();
        }
        self.publish_clock();
    }

    /// Runs the match up to logical tick `tick` exactly (no further) and
    /// leaves the clock on it. With the worker paused afterwards the page
    /// shows one reproducible instant: what the pixel goldens need, since
    /// the match otherwise follows the real clock.
    pub fn run_to(&mut self, tick: u32) {
        while !self.engine.is_finished() && self.engine.state().tick < tick {
            self.step();
        }
        self.match_ms = f64::from(self.engine.state().now_ms());
        self.publish_clock();
    }

    /// A tactical command of the panel (6C): `side` 0 home, 1 away;
    /// `mentality` 0 Defensive … 4 Attacking; `pressing` 0 Low … 3
    /// `UltraHigh`. In force from the next logical tick, and visible in the
    /// snapshots from then on.
    ///
    /// # Errors
    /// When a code is out of range (nothing changes).
    pub fn set_tactics(&mut self, side: u32, mentality: u32, pressing: u32) -> Result<(), String> {
        if !sab::apply_tactics(&mut self.engine, side, mentality, pressing) {
            return Err(format!(
                "invalid tactics: side {side}, mentality {mentality}, pressing {pressing}"
            ));
        }
        self.commands
            .extend([self.engine.state().tick, side, mentality, pressing]);
        Ok(())
    }

    /// The commands applied so far, `[tick, side, mentality, pressing]`
    /// each: what `reference_slot_with` takes to replay the match.
    #[must_use]
    pub fn commands(&self) -> Vec<u32> {
        self.commands.clone()
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

    /// Role code of each of the 22 players (`mesh::role_code`), in engine
    /// order. Fixed for the match: sent once to the main thread, outside
    /// the snapshot ring.
    #[must_use]
    pub fn roster(&self) -> Vec<u8> {
        self.engine
            .state()
            .players
            .iter()
            .map(|p| mesh::role_code(p.role))
            .collect()
    }
}

#[cfg(target_arch = "wasm32")]
impl EngineHost {
    /// One logical tick, accounted for the HUD and published.
    fn step(&mut self) {
        self.engine.tick_logic();
        self.hud.observe(&self.engine);
        self.publish_tick();
    }

    /// Publishes the match clock (and the end of the match).
    fn publish_clock(&mut self) {
        if self.engine.is_finished() {
            self.match_ms = f64::from(self.engine.state().now_ms());
            let _ = js_sys::Atomics::store(&self.ints, sab::H_STATE, sab::STATE_FINISHED);
        }
        #[allow(clippy::cast_possible_truncation)] // < 2^31 ms: a match is 5.4e6 ms
        let _ = js_sys::Atomics::store(&self.ints, sab::H_NOW, self.match_ms as i32);
    }

    /// Publishes the snapshots of the tick just run: its whole 100 ms at
    /// 60 Hz, ahead of time (see `sab::tick_frames`).
    fn publish_tick(&mut self) {
        for words in sab::tick_frames(&self.engine, &self.hud) {
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
/// x0, y0, x1, y1, … for the 22 players; `phases` is the snapshot's phase
/// word (home | away << 8 | half << 16); `velocities` is vx0, vy0, … in m/s
/// to draw velocity arrows, or empty for none; `overlays` is a bit set
/// (`OVERLAY_OFFSIDE`, `OVERLAY_FORMATION`); `roster` is the role code of
/// each player (`EngineHost::roster`), needed by the formation lines.
#[wasm_bindgen]
#[must_use]
#[allow(clippy::too_many_arguments)] // a flat frame across the wasm boundary
pub fn frame_mesh_vertices(
    xy: &[f32],
    ball_x: f32,
    ball_y: f32,
    ball_z: f32,
    sent_off: u32,
    phases: u32,
    velocities: &[f32],
    overlays: u32,
    roster: &[u8],
    width_px: u32,
    height_px: u32,
) -> Vec<f32> {
    let frame = mesh::frame_from_parts(xy, [ball_x, ball_y, ball_z], sent_off, phases);
    let velocities = mesh::velocities_from_parts(velocities);
    let roster = mesh::roster_from_parts(roster);
    let overlays = mesh::Overlays {
        velocities: velocities.as_ref(),
        offside: overlays & OVERLAY_OFFSIDE != 0,
        formation: roster
            .as_ref()
            .filter(|_| overlays & OVERLAY_FORMATION != 0),
    };
    mesh::frame_mesh(&frame, &overlays, width_px, height_px).verts
}

/// Bit of `overlays` that draws the offside line (F3).
pub const OVERLAY_OFFSIDE: u32 = 1;
/// Bit of `overlays` that draws the formation lines (F4).
pub const OVERLAY_FORMATION: u32 = 2;

/// How the pitch is fitted into a `width_px` × `height_px` canvas, as
/// `[centre x, centre y, scale x, scale y]`: pitch point `p` is drawn at
/// clip `((p.x − cx) · sx, (p.y − cy) · sy)`. Pure; the same view as the
/// mesh, for labels laid over the canvas.
#[wasm_bindgen]
#[must_use]
pub fn pitch_view(width_px: u32, height_px: u32) -> Vec<f32> {
    mesh::pitch_view(width_px, height_px).params().to_vec()
}

/// One ring slot as a fresh engine produces it for demo match `seed` after
/// `tick` logical ticks. A pure reference for tests: the snapshot the
/// worker published for that tick must be these words, bit for bit.
#[wasm_bindgen]
#[must_use]
pub fn reference_slot(seed: u32, tick: u32) -> Vec<u32> {
    sab::reference_words(seed, tick, &[]).to_vec()
}

/// Like `reference_slot`, for a match that received tactical commands:
/// `commands` is `[tick, side, mentality, pressing]` for each one
/// (`EngineHost::commands`).
#[wasm_bindgen]
#[must_use]
pub fn reference_slot_with(seed: u32, tick: u32, commands: &[u32]) -> Vec<u32> {
    sab::reference_words(seed, tick, commands).to_vec()
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
    #[allow(clippy::too_many_arguments)] // a flat frame across the wasm boundary
    pub fn draw(
        &self,
        xy: &[f32],
        ball_x: f32,
        ball_y: f32,
        ball_z: f32,
        sent_off: u32,
        phases: u32,
        velocities: &[f32],
        overlays: u32,
        roster: &[u8],
    ) -> Result<u32, String> {
        let (w, h) = self.renderer.size();
        let verts = frame_mesh_vertices(
            xy, ball_x, ball_y, ball_z, sent_off, phases, velocities, overlays, roster, w, h,
        );
        self.renderer.draw(mesh::BACKGROUND, &verts)?;
        u32::try_from(verts.len() / 6).map_err(|_| "mesh too large".into())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn hello_reports_libm_pi() {
        assert_eq!(super::hello(), "Hello from Rust (libm: 3.141592653589793)");
    }
}
