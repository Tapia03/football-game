// Protocol between the world worker and a match worker of the pool (spec
// Fase 7B, 7B.4b). A match worker holds a copy of the world: loaded once
// from the pieces of the save, brought in step with the day before each
// round, and asked for one match at a time. It keeps nothing of a match it
// played; all it ever changes is its own copy, overwritten by the next
// `sync`.

/** World worker → match worker, over the port the page made for them. */
export type MatchRequest =
  /** The whole world, as `WorldHost.load` takes it. Answered by `loaded`. */
  | {
      readonly kind: 'load';
      readonly seed: string;
      readonly day: number;
      readonly userClub: number;
      readonly clubRows: Uint8Array;
      readonly lineups: Uint8Array;
      readonly sheets: Uint8Array;
      readonly dynamics: Uint8Array;
      readonly fixtureRows: Uint32Array;
      readonly matchSeeds: Uint8Array;
    }
  /** What changes from one day to the next (`WorldHost.sync_day`). No answer. */
  | {
      readonly kind: 'sync';
      readonly day: number;
      readonly dynamics: Uint8Array;
      readonly clubRows: Uint8Array;
      readonly lineups: Uint8Array;
    }
  /**
   * Play match `id` of the day. Answered twice: by `started` as soon as it
   * arrives, before the match is played, and by `played` with the result.
   */
  | { readonly kind: 'play'; readonly id: number };

export type MatchReply =
  /** `wasmBytes`: size of the worker's WASM memory, for the record. */
  | { readonly kind: 'loaded'; readonly wasmBytes: number }
  /**
   * The `play` of match `id` arrived and is about to be played. It tells a
   * match that never got to the worker from one that never came back.
   */
  | { readonly kind: 'started'; readonly id: number }
  /** `row`: the result row of `WorldHost.play`; `ms`: how long the match took. */
  | { readonly kind: 'played'; readonly id: number; readonly row: Uint32Array; readonly ms: number }
  /** The request before this could not be served. */
  | { readonly kind: 'failed'; readonly message: string };

/** Page → match worker. */
export type MatchControl =
  /** The worker's end of the line to the world worker. */
  | { readonly type: 'start'; readonly port: MessagePort }
  /** Test hook: the worker throws, as a defect in it would. */
  | { readonly type: 'crash' };

export type MatchReady = { readonly type: 'ready' };
