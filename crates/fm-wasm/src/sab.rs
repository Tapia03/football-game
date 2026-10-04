//! Layout of the snapshot ring the engine worker publishes in a
//! `SharedArrayBuffer` (spec Fase 6, 6A). Plain words, little-endian; the
//! main thread reads the same bytes through `Int32Array` / `Float32Array`
//! views (`frontend/src/engine-bridge/sab.ts` mirrors these constants).
//!
//! ```text
//! header, 16 × i32:  [0] magic+version  [1] write sequence (atomic)
//!                    [2] ring slots     [3] bytes per slot
//!                    [4] state          [5] speed × 1000   [6] seed
//! slot, 52 words:    [0] tick  [1] t_ms  [2] score (home | away << 8)
//!                    [3] phases (home | away << 8)
//!                    [4] sent-off mask (bit i = player i)
//!                    [5..8] ball x, y, z (f32)
//!                    [8..52] players x, y (f32), engine order
//! ```
//!
//! Safe code: the engine fills a `[u32; SLOT_WORDS]` and the bindings copy
//! it into a typed-array view of the buffer. Nothing here touches memory.

use fm_core::{Vec2, Vec3};
use fm_match::{MatchSnapshot, Phase};

/// `"FM"` and the layout version (bump on any layout change).
pub const MAGIC: u32 = 0x464D_0001;
/// Words of one slot (the length of the encoded array).
pub const SLOT_WORDS: usize = 52;
// Sizes and indices as `u32`: that is what the typed-array API takes.
pub const HEADER_WORDS: u32 = 16;
pub const RING_SLOTS: u32 = 16;
pub const SLOT_WORDS_U32: u32 = 52;
pub const SLOT_BYTES: u32 = SLOT_WORDS_U32 * 4;
pub const BUFFER_BYTES: u32 = HEADER_WORDS * 4 + RING_SLOTS * SLOT_BYTES;

pub const H_MAGIC: u32 = 0;
pub const H_SEQ: u32 = 1;
pub const H_SLOTS: u32 = 2;
pub const H_SLOT_BYTES: u32 = 3;
pub const H_STATE: u32 = 4;
pub const H_SPEED: u32 = 5;
pub const H_SEED: u32 = 6;

pub const STATE_RUNNING: i32 = 1;
pub const STATE_PAUSED: i32 = 2;
pub const STATE_FINISHED: i32 = 3;

const S_TICK: usize = 0;
const S_T_MS: usize = 1;
const S_SCORE: usize = 2;
const S_PHASES: usize = 3;
const S_SENT_OFF: usize = 4;
const S_BALL: usize = 5;
const S_PLAYERS: usize = 8;

const PLAYERS: usize = 22;

/// Word index of ring slot `seq` (the `seq`-th snapshot written).
#[inline]
#[must_use]
pub const fn slot_offset(seq: u32) -> u32 {
    HEADER_WORDS + (seq % RING_SLOTS) * SLOT_WORDS_U32
}

const fn phase_code(p: Phase) -> u32 {
    match p {
        Phase::InPossession => 0,
        Phase::OutOfPossession => 1,
        Phase::TransitionAttack => 2,
        Phase::TransitionDefense => 3,
        Phase::SetPiece => 4,
    }
}

/// One snapshot as it sits in a ring slot.
#[must_use]
pub fn encode(s: &MatchSnapshot) -> [u32; SLOT_WORDS] {
    let mut w = [0_u32; SLOT_WORDS];
    w[S_TICK] = s.tick;
    w[S_T_MS] = s.t_ms;
    w[S_SCORE] = u32::from(s.score[0]) | u32::from(s.score[1]) << 8;
    w[S_PHASES] = phase_code(s.phases[0]) | phase_code(s.phases[1]) << 8;
    w[S_BALL] = s.ball.x.to_bits();
    w[S_BALL + 1] = s.ball.y.to_bits();
    w[S_BALL + 2] = s.ball.z.to_bits();
    for (i, p) in s.players.iter().enumerate() {
        w[S_SENT_OFF] |= u32::from(p.sent_off) << i;
        w[S_PLAYERS + 2 * i] = p.pos.x.to_bits();
        w[S_PLAYERS + 2 * i + 1] = p.pos.y.to_bits();
    }
    w
}

/// What a reader gets back from a slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub tick: u32,
    pub t_ms: u32,
    pub score: [u8; 2],
    /// Phase codes, `[home, away]` (0 in possession … 4 set piece).
    pub phases: [u8; 2],
    /// Bit `i` set: player `i` was sent off.
    pub sent_off: u32,
    pub ball: Vec3,
    pub players: [Vec2; PLAYERS],
}

#[must_use]
pub fn decode(w: &[u32; SLOT_WORDS]) -> Frame {
    let byte = |v: u32, shift: u32| (v >> shift & 0xFF) as u8;
    let mut players = [Vec2::ZERO; PLAYERS];
    for (i, p) in players.iter_mut().enumerate() {
        *p = Vec2::new(
            f32::from_bits(w[S_PLAYERS + 2 * i]),
            f32::from_bits(w[S_PLAYERS + 2 * i + 1]),
        );
    }
    Frame {
        tick: w[S_TICK],
        t_ms: w[S_T_MS],
        score: [byte(w[S_SCORE], 0), byte(w[S_SCORE], 8)],
        phases: [byte(w[S_PHASES], 0), byte(w[S_PHASES], 8)],
        sent_off: w[S_SENT_OFF],
        ball: Vec3::new(
            f32::from_bits(w[S_BALL]),
            f32::from_bits(w[S_BALL + 1]),
            f32::from_bits(w[S_BALL + 2]),
        ),
        players,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_match::demo::demo_match;
    use fm_match::{LodLevel, MatchEngine};

    #[test]
    fn layout_sizes_are_the_ones_in_the_spec() {
        assert_eq!(SLOT_BYTES, 208);
        assert_eq!(BUFFER_BYTES, 64 + 16 * 208);
        assert_eq!(BUFFER_BYTES, 3_392);
        assert_eq!(S_PLAYERS + 2 * PLAYERS, SLOT_WORDS);
    }

    #[test]
    fn ring_wraps_after_sixteen_slots() {
        assert_eq!(slot_offset(0), HEADER_WORDS);
        assert_eq!(slot_offset(1), HEADER_WORDS + SLOT_WORDS_U32);
        assert_eq!(SLOT_WORDS_U32 as usize, SLOT_WORDS);
        assert_eq!(slot_offset(16), slot_offset(0));
        assert_eq!(slot_offset(u32::MAX), slot_offset(15));
    }

    /// A snapshot survives the slot bit for bit.
    #[test]
    fn a_snapshot_round_trips_bit_for_bit() {
        let (db, setup) = demo_match(7);
        let mut e = MatchEngine::new(&setup, &db);
        for _ in 0..3_000 {
            e.tick_logic();
        }
        let now = e.state().now_ms();
        let snap = e.sample(LodLevel::Full, now + 33).expect("snapshot");
        let f = decode(&encode(&snap));
        assert_eq!((f.tick, f.t_ms), (snap.tick, snap.t_ms));
        assert_eq!(f.score, snap.score);
        assert_eq!(f.ball.x.to_bits(), snap.ball.x.to_bits());
        assert_eq!(f.ball.y.to_bits(), snap.ball.y.to_bits());
        assert_eq!(f.ball.z.to_bits(), snap.ball.z.to_bits());
        for (i, p) in snap.players.iter().enumerate() {
            assert_eq!(f.players[i].x.to_bits(), p.pos.x.to_bits(), "player {i} x");
            assert_eq!(f.players[i].y.to_bits(), p.pos.y.to_bits(), "player {i} y");
            assert_eq!(f.sent_off >> i & 1 == 1, p.sent_off, "player {i} sent off");
        }
        assert_eq!(
            [u32::from(f.phases[0]), u32::from(f.phases[1])],
            [phase_code(snap.phases[0]), phase_code(snap.phases[1])]
        );
    }
}
