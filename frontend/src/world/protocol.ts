// Protocol of the world worker (spec Fase 7B, 7B.4). The world worker is
// the only place the world lives: it simulates the days and asks the
// database worker to write them. The page sends it operations in the same
// envelope the database worker uses:
//
//   request:  { id, op, args }
//   response: { id, ok: true, result } | { id, ok: false, error }

import type { DbError } from '../save/protocol';

/** Where the world is. */
export type WorldSummary = {
  /** The world seed: a u64 in decimal. */
  readonly seed: string;
  /** Days of the season already lived: today is this day. */
  readonly day: number;
  readonly seasonYear: number;
  readonly userClub: number;
  /** Round (0-based) of the next match to be played; 38 when there is none. */
  readonly nextRound: number;
  /** Matches to be played today (0 on a day off). */
  readonly matchesToday: number;
  /** The season is over. */
  readonly finished: boolean;
};

/** How long a day with matches took (real milliseconds), for measurement. */
export type DayTiming = {
  /** The day that was lived (the world was on it before). */
  readonly day: number;
  readonly round: number;
  readonly matches: number;
  /** The matches being played, from the first dealt to the last back. */
  readonly simulateMs: number;
  /**
   * The time the matches took, each as whoever played it measured it,
   * summed: with a pool this is more than `simulateMs`.
   */
  readonly matchMs: number;
  /** Match workers that played the day (0: the world worker played alone). */
  readonly players: number;
  /** Ending the day in the database (`world.commitDay`), round trip. */
  readonly commitMs: number;
  /** The whole day: matches, ending it, the commit. */
  readonly totalMs: number;
};

export type WorldOps = {
  /**
   * Makes the world of `seed` and writes it into the save that is open in
   * the database worker. Refused, as the database refuses it, when the
   * save already has a world.
   */
  'world.new': { args: { readonly seed: string; readonly userClub: number }; result: WorldSummary };
  /**
   * Loads the world of the open save as last committed. A save without a
   * world fails with `no-world`, as the database says it.
   */
  'world.open': { args: Record<string, never>; result: WorldSummary };
  /**
   * Lives `days` days, one at a time: the matches of the day, the end of
   * the day, its commit to the database, then the next day. Stops early
   * when the season ends, or when cancelled (`WorldControl` `cancel`):
   * the match being played ends, the day under way is abandoned — nothing
   * of it was written — and the world stays on the last day committed.
   */
  'world.advance': {
    args: { readonly days: number };
    result: {
      readonly summary: WorldSummary;
      /** Days lived by this call. */
      readonly daysLived: number;
      /** Stopped by a cancel before living all the days asked for. */
      readonly cancelled: boolean;
      /** One entry for each day lived that had matches. */
      readonly timing: readonly DayTiming[];
    };
  };
  /** The pool and the memory, as they are now. */
  'world.stats': { args: Record<string, never>; result: WorldStats };
};

/** What the world worker and its pool are made of, for measurement. */
export type WorldStats = {
  /** Match workers in the pool right now. */
  readonly players: number;
  /** Match workers dropped since the start, and why. */
  readonly dropped: readonly string[];
  /** Size of the WASM memory of the world worker and of each match worker (bytes). */
  readonly wasmBytes: { readonly world: number; readonly players: readonly number[] };
};

export type WorldOp = keyof WorldOps;

export type WorldRequest<O extends WorldOp = WorldOp> = {
  readonly id: number;
  readonly op: O;
  readonly args: WorldOps[O]['args'];
};

/**
 * `no-world-loaded`: an operation that needs a world before one was made
 * or opened. `db-timeout`: the database worker did not answer in time.
 */
export type WorldError = {
  readonly code: DbError['code'] | 'no-world-loaded' | 'db-timeout';
  readonly message: string;
};

export type WorldResponse<O extends WorldOp = WorldOp> =
  | { readonly id: number; readonly ok: true; readonly result: WorldOps[O]['result'] }
  | { readonly id: number; readonly ok: false; readonly error: WorldError };

/** Control messages of the worker, besides requests. */
export type WorldControl =
  /**
   * The line to the database worker (`Database.connect()`); sent first.
   * `dbTimeoutMs`: how long a request to the database may take before the
   * operation fails with `db-timeout` (10 s when not given).
   */
  | {
      readonly type: 'start';
      readonly db: MessagePort;
      readonly dbTimeoutMs?: number;
      /**
       * The pool: a port to each match worker (none: the world worker plays
       * the matches itself). `matchTimeoutMs`: how long a match worker may
       * take to answer before it is dropped (30 s when not given).
       */
      readonly players?: readonly MessagePort[];
      readonly matchTimeoutMs?: number;
    }
  /**
   * Stops the advance under way after the match being played. Between the
   * last match of a day and the commit of that day it is not honoured for
   * that day (the window is a few milliseconds): the day ends, and the
   * cancel holds from the next day on. Ignored when nothing is advancing.
   */
  | { readonly type: 'cancel' };

export type WorldReady = { readonly type: 'ready' };

/**
 * Posted by a worker of the world (the world worker, a match worker) as its
 * script runs, before anything is asked of it: the script loaded. Only then
 * is the worker handed its ports.
 */
export type WorldLoaded = { readonly type: 'loaded' };

/**
 * Sent after every match of a `world.advance`, outside the request /
 * response envelope: what the progress bar shows.
 */
export type WorldProgress = {
  readonly type: 'progress';
  /** The day being lived and its round (0-based). */
  readonly day: number;
  readonly round: number;
  /** Matches of the day already played, and how many the day has. */
  readonly done: number;
  readonly total: number;
  /** Days of this advance still to be lived after this one. */
  readonly daysLeft: number;
  /**
   * Estimate of what is left of this day's matches (ms), from the mean
   * time of the matches this advance has played.
   */
  readonly etaMs: number;
};
