//! Layout of the snapshot ring the engine worker publishes in a
//! `SharedArrayBuffer` (spec Fase 6, 6A). Plain words, little-endian; the
//! main thread reads the same bytes through `Int32Array` / `Float32Array`
//! views (`frontend/src/engine-bridge/sab.ts` mirrors these constants).
//!
//! ```text
//! header, 16 × i32:  [0] magic+version  [1] write sequence (atomic)
//!                    [2] ring slots     [3] bytes per slot
//!                    [4] state          [5] speed × 1000   [6] seed
//!                    [7] match clock of the worker (ms)
//! slot, 56 words:    [0] tick  [1] t_ms  [2] score (home | away << 8)
//!                    [3] phases (home | away << 8 | half << 16)
//!                    [4] sent-off mask (bit i = player i)
//!                    [5..8] ball x, y, z (f32)
//!                    [8..52] players x, y (f32), engine order
//!                    [52] cards (home yellow | home red << 8
//!                                | away yellow << 16 | away red << 24)
//!                    [53] home held-ball ticks  [54] away held-ball ticks
//!                    [55] reserved (0)
//! ```
//!
//! Version 2 (6B-1) appended words 52..56 and the half; nothing moved.
//!
//! Safe code: the engine fills a `[u32; SLOT_WORDS]` and the bindings copy
//! it into a typed-array view of the buffer. Nothing here touches memory.

use fm_core::{Vec2, Vec3};
use fm_match::state::BallState;
use fm_match::{CardKind, EventKind, LodLevel, MatchEngine, MatchSnapshot, Phase, Side};

/// `"FM"` and the layout version (bump on any layout change).
pub const MAGIC: u32 = 0x464D_0002;
/// Words of one slot (the length of the encoded array).
pub const SLOT_WORDS: usize = 56;
// Sizes and indices as `u32`: that is what the typed-array API takes.
pub const HEADER_WORDS: u32 = 16;
pub const RING_SLOTS: u32 = 16;
pub const SLOT_WORDS_U32: u32 = 56;
pub const SLOT_BYTES: u32 = SLOT_WORDS_U32 * 4;
pub const BUFFER_BYTES: u32 = HEADER_WORDS * 4 + RING_SLOTS * SLOT_BYTES;

pub const H_MAGIC: u32 = 0;
pub const H_SEQ: u32 = 1;
pub const H_SLOTS: u32 = 2;
pub const H_SLOT_BYTES: u32 = 3;
pub const H_STATE: u32 = 4;
pub const H_SPEED: u32 = 5;
pub const H_SEED: u32 = 6;
/// The worker's match clock (ms since kick-off), updated on every step:
/// the main thread renders one sample interval behind it.
pub const H_NOW: u32 = 7;

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
const S_CARDS: usize = 52;
const S_HELD: usize = 53;

const PLAYERS: usize = 22;

/// What the HUD shows besides the snapshot itself (spec Fase 6, 6B-1),
/// accumulated by the engine worker after every tick: cards per side and
/// how long each side has held the ball. Outside `tick_logic`: the engine
/// is only read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Hud {
    /// 0 first half, 1 second half.
    pub half: u8,
    /// `[home yellow, home red, away yellow, away red]`.
    pub cards: [u8; 4],
    /// Ticks with the ball held by `[home, away]`.
    pub held: [u32; 2],
    /// Events of the log already counted.
    seen: usize,
}

impl Hud {
    /// Accounts for the tick the engine has just run.
    pub fn observe(&mut self, engine: &MatchEngine) {
        let state = engine.state();
        self.half = u8::from(state.second_half);
        let events = engine.events();
        for event in &events[self.seen.min(events.len())..] {
            if let EventKind::Card { player, card } = event.kind {
                let away = state
                    .players
                    .iter()
                    .any(|p| p.id == player && p.side == Side::Away);
                let slot = 2 * usize::from(away) + usize::from(card == CardKind::Red);
                self.cards[slot] = self.cards[slot].saturating_add(1);
            }
        }
        self.seen = events.len();
        if let BallState::Held { holder } = state.ball {
            let away = state.players[usize::from(holder)].side == Side::Away;
            self.held[usize::from(away)] += 1;
        }
    }
}

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
pub fn encode(s: &MatchSnapshot, hud: &Hud) -> [u32; SLOT_WORDS] {
    let mut w = [0_u32; SLOT_WORDS];
    w[S_TICK] = s.tick;
    w[S_T_MS] = s.t_ms;
    w[S_SCORE] = u32::from(s.score[0]) | u32::from(s.score[1]) << 8;
    w[S_PHASES] =
        phase_code(s.phases[0]) | phase_code(s.phases[1]) << 8 | u32::from(hud.half) << 16;
    w[S_CARDS] = u32::from_le_bytes(hud.cards);
    w[S_HELD] = hud.held[0];
    w[S_HELD + 1] = hud.held[1];
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

/// Snapshots published per logical tick: the 60 Hz grid of LOD `Full`.
pub const SAMPLES_PER_TICK: usize = 6;

/// The snapshots of the tick the engine has just run, ahead of time: the
/// six instants of LOD `Full` inside the coming 100 ms (0, 17, 33, 50, 67,
/// 83 ms). Trajectories are pure functions of time inside a tick, so the
/// whole interval is known as soon as the tick has run.
#[must_use]
pub fn tick_frames(engine: &MatchEngine, hud: &Hud) -> [[u32; SLOT_WORDS]; SAMPLES_PER_TICK] {
    let now = engine.state().now_ms();
    let mut out = [[0_u32; SLOT_WORDS]; SAMPLES_PER_TICK];
    for (slot, off) in out.iter_mut().zip(LodLevel::Full.sample_offsets_ms()) {
        if let Some(snap) = engine.sample(LodLevel::Full, now + off) {
            *slot = encode(&snap, hud);
        }
    }
    out
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
    /// 0 first half, 1 second half.
    pub half: u8,
    /// `[home yellow, home red, away yellow, away red]`.
    pub cards: [u8; 4],
    /// Ticks with the ball held by `[home, away]`.
    pub held: [u32; 2],
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
        half: byte(w[S_PHASES], 16),
        cards: w[S_CARDS].to_le_bytes(),
        held: [w[S_HELD], w[S_HELD + 1]],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_match::demo::demo_match;

    #[test]
    fn layout_sizes_are_the_ones_in_the_spec() {
        assert_eq!(SLOT_BYTES, 224);
        assert_eq!(BUFFER_BYTES, 64 + 16 * 224);
        assert_eq!(BUFFER_BYTES, 3_648);
        // Version 2 only appended: the 6A fields sit where they were.
        assert_eq!(S_PLAYERS + 2 * PLAYERS, S_CARDS);
        assert_eq!(S_HELD + 3, SLOT_WORDS);
        assert_eq!(MAGIC & 0xFFFF, 2);
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
        let hud = Hud {
            half: 1,
            cards: [3, 1, 2, 0],
            held: [1_234, 987],
            seen: 0,
        };
        let f = decode(&encode(&snap, &hud));
        assert_eq!((f.half, f.cards, f.held), (hud.half, hud.cards, hud.held));
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

    /// The HUD counters agree with the engine over a whole match: cards with
    /// the event log, held ticks with the ball state, the half with the
    /// state.
    #[test]
    fn hud_counters_match_the_engine_over_a_match() {
        // Seeds chosen to cover matches with and without cards.
        let mut any_card = false;
        for seed in [3_u64, 7, 11] {
            let (db, setup) = demo_match(seed);
            let mut e = MatchEngine::new(&setup, &db);
            let mut hud = Hud::default();
            let mut held = [0_u32; 2];
            assert_eq!(hud.half, 0);
            while !e.is_finished() {
                e.tick_logic();
                hud.observe(&e);
                if let BallState::Held { holder } = e.state().ball {
                    let away = e.state().players[usize::from(holder)].side == Side::Away;
                    held[usize::from(away)] += 1;
                }
            }
            let mut cards = [0_u8; 4];
            for event in e.events() {
                if let EventKind::Card { player, card } = event.kind {
                    let p = e
                        .state()
                        .players
                        .iter()
                        .find(|p| p.id == player)
                        .expect("carded player is on the sheet");
                    let slot =
                        2 * usize::from(p.side == Side::Away) + usize::from(card == CardKind::Red);
                    cards[slot] += 1;
                }
            }
            assert_eq!(hud.cards, cards, "seed {seed}");
            assert_eq!(hud.held, held, "seed {seed}");
            assert_eq!(hud.half, 1, "second half by the end");
            assert!(
                held[0] > 5_000 && held[1] > 5_000,
                "both sides hold the ball"
            );
            any_card |= cards.iter().any(|&c| c > 0);
        }
        assert!(any_card, "the seeds cover a match with cards");
    }

    /// Each tick publishes six snapshots 16–17 ms apart, and the grid has no
    /// gap across ticks (83 ms → the next tick's 0 ms is 17 ms later).
    #[test]
    fn a_tick_publishes_the_sixty_hertz_grid() {
        assert_eq!(
            LodLevel::Full.sample_offsets_ms().len(),
            SAMPLES_PER_TICK,
            "one slot per Full offset"
        );
        let (db, setup) = demo_match(7);
        let mut e = MatchEngine::new(&setup, &db);
        let mut stamps = Vec::new();
        let mut moved = false;
        let mut hud = Hud::default();
        for _ in 0..50 {
            e.tick_logic();
            hud.observe(&e);
            let frames = tick_frames(&e, &hud).map(|w| decode(&w));
            // The first sample is the tick itself, bit for bit.
            let at_tick = e
                .sample(LodLevel::Reduced, e.state().now_ms())
                .expect("snapshot");
            let w = encode(&at_tick, &hud);
            assert_eq!(decode(&w), frames[0]);
            moved |= frames[0].players != frames[5].players;
            stamps.extend(frames.iter().map(|f| f.t_ms));
        }
        assert!(moved, "players move between the sub-samples");
        for pair in stamps.windows(2) {
            let step = pair[1] - pair[0];
            assert!((16..=17).contains(&step), "grid step {step} ms");
        }
    }
}
