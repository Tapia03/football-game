// Snapshot ring shared with the engine worker (spec Fase 6, 6A). Mirrors
// `crates/fm-wasm/src/sab.rs`: the worker writes, the main thread reads the
// same bytes through an Int32Array and a Float32Array.

/**
 * "FM" + layout version 3 (6B-1 added half, cards and held-ball ticks;
 * 6B-2 added the team statistics).
 */
export const MAGIC = 0x464d_0003;
export const HEADER_WORDS = 16;
export const RING_SLOTS = 16;
export const SLOT_WORDS = 72;
export const SLOT_BYTES = SLOT_WORDS * 4;
export const BUFFER_BYTES = HEADER_WORDS * 4 + RING_SLOTS * SLOT_BYTES;

export const H_MAGIC = 0;
export const H_SEQ = 1;
export const H_SLOTS = 2;
export const H_SLOT_BYTES = 3;
export const H_STATE = 4;
export const H_SPEED = 5;
export const H_SEED = 6;
/** The worker's match clock (ms since kick-off). */
export const H_NOW = 7;
/**
 * Wall-clock time of the worker's last step (ms, see `wallClockMs`): lets
 * the main thread measure how stale the match clock is when it draws.
 */
export const H_STEP_WALL = 8;

/** A wall clock both threads share, folded into 31 bits. */
export function wallClockMs(): number {
  return Math.floor(performance.timeOrigin + performance.now()) & 0x7fff_ffff;
}

/** Snapshots per logical tick (the 60 Hz grid) and their spacing. */
export const SAMPLES_PER_TICK = 6;
export const SAMPLE_INTERVAL_MS = 100 / SAMPLES_PER_TICK;

export const STATE_RUNNING = 1;
export const STATE_PAUSED = 2;
export const STATE_FINISHED = 3;

const S_TICK = 0;
const S_T_MS = 1;
const S_SCORE = 2;
const S_PHASES = 3;
const S_SENT_OFF = 4;
const S_BALL = 5;
const S_PLAYERS = 8;
const S_CARDS = 52;
const S_HELD = 53;
const S_STATS = 56;
const STATS_WORDS = 7;

export const PLAYERS = 22;

/** One team's running statistics. */
export type TeamStats = {
  shots: number;
  onTarget: number;
  xg: number;
  passes: number;
  passesCompleted: number;
  tackles: number;
  /** Fouls committed. */
  fouls: number;
};

function newStats(): TeamStats {
  return { shots: 0, onTarget: 0, xg: 0, passes: 0, passesCompleted: 0, tackles: 0, fouls: 0 };
}

export function copyStats(from: TeamStats, out: TeamStats): void {
  out.shots = from.shots;
  out.onTarget = from.onTarget;
  out.xg = from.xg;
  out.passes = from.passes;
  out.passesCompleted = from.passesCompleted;
  out.tackles = from.tackles;
  out.fouls = from.fouls;
}

/** One snapshot copied out of the ring. `xy` is x0, y0, x1, y1, … */
export type Frame = {
  tick: number;
  tMs: number;
  homeGoals: number;
  awayGoals: number;
  homePhase: number;
  awayPhase: number;
  /** Bit i set: player i was sent off. */
  sentOff: number;
  ballX: number;
  ballY: number;
  ballZ: number;
  readonly xy: Float32Array;
  /** 0 first half, 1 second half. */
  half: number;
  homeYellows: number;
  homeReds: number;
  awayYellows: number;
  awayReds: number;
  /** Logical ticks with the ball held by each side so far. */
  homeHeld: number;
  awayHeld: number;
  readonly homeStats: TeamStats;
  readonly awayStats: TeamStats;
};

export function newFrame(): Frame {
  return {
    tick: 0,
    tMs: 0,
    homeGoals: 0,
    awayGoals: 0,
    homePhase: 0,
    awayPhase: 0,
    sentOff: 0,
    ballX: 0,
    ballY: 0,
    ballZ: 0,
    xy: new Float32Array(PLAYERS * 2),
    half: 0,
    homeYellows: 0,
    homeReds: 0,
    awayYellows: 0,
    awayReds: 0,
    homeHeld: 0,
    awayHeld: 0,
    homeStats: newStats(),
    awayStats: newStats(),
  };
}

/** Allocates the buffer the worker publishes into. */
export function newSnapshotBuffer(): SharedArrayBuffer {
  return new SharedArrayBuffer(BUFFER_BYTES);
}

/**
 * Lock-free reader of the ring (single writer, single reader). The writer
 * fills slot `seq % RING_SLOTS` and only then publishes `seq + 1`, so every
 * slot below the sequence is complete — unless the writer has lapped it,
 * which the re-check after the copy detects.
 */
export class SnapshotReader {
  private readonly ints: Int32Array;
  private readonly floats: Float32Array;

  constructor(buffer: SharedArrayBuffer) {
    this.ints = new Int32Array(buffer);
    this.floats = new Float32Array(buffer);
  }

  /** True once the worker has written the header. */
  ready(): boolean {
    return (
      Atomics.load(this.ints, H_SEQ) > 0 &&
      this.header(H_MAGIC) === MAGIC &&
      this.header(H_SLOTS) === RING_SLOTS &&
      this.header(H_SLOT_BYTES) === SLOT_BYTES
    );
  }

  /** Snapshots published so far. */
  sequence(): number {
    return Atomics.load(this.ints, H_SEQ);
  }

  state(): number {
    return Atomics.load(this.ints, H_STATE);
  }

  /** The worker's match clock (ms since kick-off). */
  clockMs(): number {
    return Atomics.load(this.ints, H_NOW);
  }

  /**
   * Real milliseconds since the worker last advanced the match (0 before
   * its first step; never negative, whatever the two clocks' granularity).
   */
  stalenessMs(): number {
    const at = Atomics.load(this.ints, H_STEP_WALL);
    return at === 0 ? 0 : Math.max(0, wallClockMs() - at);
  }

  /**
   * The raw words of snapshot `n`, or undefined when it is not in the ring
   * (tests compare them bit for bit with a reference).
   */
  rawSlot(n: number): Uint32Array | undefined {
    const seq = this.sequence();
    if (n < 0 || n >= seq || seq - n >= RING_SLOTS) return undefined;
    const at = HEADER_WORDS + (n % RING_SLOTS) * SLOT_WORDS;
    const words = new Uint32Array(this.ints.buffer, at * 4, SLOT_WORDS).slice();
    return this.sequence() - n < RING_SLOTS ? words : undefined;
  }

  /** Match time of snapshot `n` (only meaningful while it is in the ring). */
  timeOf(n: number): number {
    return this.ints[HEADER_WORDS + (n % RING_SLOTS) * SLOT_WORDS + S_T_MS] ?? 0;
  }

  header(index: number): number {
    return (this.ints[index] ?? 0) >>> 0;
  }

  /**
   * Copies snapshot number `n` (0-based) into `out`. Returns false when it
   * is not written yet or the writer has already overwritten it.
   */
  read(n: number, out: Frame): boolean {
    const seq = this.sequence();
    if (n < 0 || n >= seq || seq - n >= RING_SLOTS) return false;
    const at = HEADER_WORDS + (n % RING_SLOTS) * SLOT_WORDS;
    const i = this.ints;
    const f = this.floats;
    out.tick = i[at + S_TICK] ?? 0;
    out.tMs = i[at + S_T_MS] ?? 0;
    const score = i[at + S_SCORE] ?? 0;
    out.homeGoals = score & 0xff;
    out.awayGoals = (score >> 8) & 0xff;
    const phases = i[at + S_PHASES] ?? 0;
    out.homePhase = phases & 0xff;
    out.awayPhase = (phases >> 8) & 0xff;
    out.half = (phases >> 16) & 0xff;
    const cards = i[at + S_CARDS] ?? 0;
    out.homeYellows = cards & 0xff;
    out.homeReds = (cards >> 8) & 0xff;
    out.awayYellows = (cards >> 16) & 0xff;
    out.awayReds = (cards >>> 24) & 0xff;
    out.homeHeld = i[at + S_HELD] ?? 0;
    out.awayHeld = i[at + S_HELD + 1] ?? 0;
    this.readStats(at + S_STATS, out.homeStats);
    this.readStats(at + S_STATS + STATS_WORDS, out.awayStats);
    out.sentOff = i[at + S_SENT_OFF] ?? 0;
    out.ballX = f[at + S_BALL] ?? 0;
    out.ballY = f[at + S_BALL + 1] ?? 0;
    out.ballZ = f[at + S_BALL + 2] ?? 0;
    out.xy.set(f.subarray(at + S_PLAYERS, at + S_PLAYERS + PLAYERS * 2));
    // Lapped while copying: the slot may be half overwritten.
    return this.sequence() - n < RING_SLOTS;
  }

  private readStats(at: number, out: TeamStats): void {
    const i = this.ints;
    out.shots = i[at] ?? 0;
    out.onTarget = i[at + 1] ?? 0;
    out.xg = this.floats[at + 2] ?? 0;
    out.passes = i[at + 3] ?? 0;
    out.passesCompleted = i[at + 4] ?? 0;
    out.tackles = i[at + 5] ?? 0;
    out.fouls = i[at + 6] ?? 0;
  }

  /** The newest snapshot, if any. */
  latest(out: Frame): boolean {
    for (let attempt = 0; attempt < 4; attempt += 1) {
      const seq = this.sequence();
      if (seq === 0) return false;
      if (this.read(seq - 1, out)) return true;
    }
    return false;
  }
}
