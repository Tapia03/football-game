// Main-thread side of the snapshot ring (spec Fase 6, 6A): picks the two
// published snapshots around a match time and interpolates between them.
// No engine here — two points and a lerp.

import {
  PLAYERS,
  RING_SLOTS,
  SAMPLE_INTERVAL_MS,
  type Frame,
  type SnapshotReader,
  copyStats,
  newFrame,
} from '../engine-bridge/sab';

function copy(from: Frame, out: Frame): void {
  out.tick = from.tick;
  out.tMs = from.tMs;
  out.homeGoals = from.homeGoals;
  out.awayGoals = from.awayGoals;
  out.homePhase = from.homePhase;
  out.awayPhase = from.awayPhase;
  out.sentOff = from.sentOff;
  out.half = from.half;
  out.homeYellows = from.homeYellows;
  out.homeReds = from.homeReds;
  out.awayYellows = from.awayYellows;
  out.awayReds = from.awayReds;
  out.homeHeld = from.homeHeld;
  out.awayHeld = from.awayHeld;
  copyStats(from.homeStats, out.homeStats);
  copyStats(from.awayStats, out.awayStats);
  out.ballX = from.ballX;
  out.ballY = from.ballY;
  out.ballZ = from.ballZ;
  out.xy.set(from.xy);
}

/**
 * `a` moved `alpha` of the way to `b` (0 = a, 1 = b). Positions are
 * interpolated; discrete fields (score, phases, sent-off) come from `a`.
 */
export function lerpFrames(a: Frame, b: Frame, alpha: number, out: Frame): void {
  copy(a, out);
  out.tMs = a.tMs + (b.tMs - a.tMs) * alpha;
  out.ballX = a.ballX + (b.ballX - a.ballX) * alpha;
  out.ballY = a.ballY + (b.ballY - a.ballY) * alpha;
  out.ballZ = a.ballZ + (b.ballZ - a.ballZ) * alpha;
  for (let i = 0; i < PLAYERS * 2; i += 1) {
    const from = a.xy[i] ?? 0;
    out.xy[i] = from + ((b.xy[i] ?? 0) - from) * alpha;
  }
}

export class FrameInterpolator {
  private readonly a = newFrame();
  private readonly b = newFrame();

  constructor(private readonly reader: SnapshotReader) {}

  /**
   * The match time to draw now: one sample interval behind the worker's
   * clock, so the two snapshots around it are already published.
   */
  renderTimeMs(): number {
    return Math.max(0, this.reader.clockMs() - SAMPLE_INTERVAL_MS);
  }

  /**
   * Fills `out` with the state at match time `tMs`. Falls back to the
   * newest snapshot when `tMs` is outside what the ring still holds (high
   * speeds, or a tab that was hidden). False when nothing is published.
   */
  at(tMs: number, out: Frame): boolean {
    const reader = this.reader;
    for (let attempt = 0; attempt < 4; attempt += 1) {
      const seq = reader.sequence();
      if (seq === 0) return false;
      const oldest = Math.max(0, seq - RING_SLOTS + 1);
      // Newest snapshot not after `tMs`.
      let n = seq - 1;
      while (n >= oldest && reader.timeOf(n) > tMs) n -= 1;
      if (n < oldest) {
        // Older than everything still in the ring: show the oldest we have.
        if (reader.read(oldest, out)) return true;
        continue;
      }
      if (!reader.read(n, this.a)) continue;
      if (n + 1 >= seq) {
        copy(this.a, out);
        return true;
      }
      if (!reader.read(n + 1, this.b)) continue;
      const span = this.b.tMs - this.a.tMs;
      const alpha = span > 0 ? Math.min(1, Math.max(0, (tMs - this.a.tMs) / span)) : 0;
      lerpFrames(this.a, this.b, alpha, out);
      return true;
    }
    return reader.latest(out);
  }
}
