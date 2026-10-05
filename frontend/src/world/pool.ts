// The pool of match workers, as the world worker sees it (spec Fase 7B,
// 7B.4b). It plays the matches of a day wherever there is a free player
// and hands back their results; with no players — one core, or a pool that
// did not come up — the world worker's own host plays them, by the same
// function. Which of the two played a match never shows: `play` is pure.

import type { WorldHost } from '../engine-bridge/pkg/fm_wasm.js';
import type { MatchReply, MatchRequest } from './match-protocol';

/** One match worker, over its port: one request in flight at a time. */
class Player {
  /** Size of the worker's WASM memory, as it last reported it. */
  wasmBytes = 0;
  private waiting: { resolve: (reply: MatchReply) => void; reject: (err: Error) => void; timer: number } | undefined;

  constructor(private readonly port: MessagePort) {
    port.onmessage = (e: MessageEvent<MatchReply>) => {
      const waiting = this.waiting;
      if (waiting === undefined) return;
      this.waiting = undefined;
      clearTimeout(waiting.timer);
      waiting.resolve(e.data);
    };
  }

  /** A message that is not answered (`sync`). */
  tell(request: MatchRequest): void {
    this.port.postMessage(request);
  }

  /** A message and its answer, or an error when none comes in `timeoutMs`. */
  ask(request: MatchRequest, timeoutMs: number): Promise<MatchReply> {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.waiting = undefined;
        reject(new Error(`o Worker de partida não respondeu em ${(timeoutMs / 1000).toFixed(1)} s`));
      }, timeoutMs) as unknown as number;
      this.waiting = { resolve, reject, timer };
      this.port.postMessage(request);
    });
  }

  close(): void {
    this.port.close();
  }
}

export type DayPlayed = {
  /** The result rows of the day's matches (in no particular order). */
  readonly rows: Uint32Array[];
  /** How long the matches took, each by whoever played it (ms, summed). */
  readonly matchMs: number;
};

export class MatchPool {
  private readonly players: Player[];
  /** The players' copies of the world are not the world any more. */
  private stale = true;
  /** Players dropped since the pool was made, and why (for the record). */
  readonly dropped: string[] = [];

  constructor(
    ports: readonly MessagePort[],
    /** How long a player may take to answer one request (ms). */
    private readonly timeoutMs: number,
  ) {
    this.players = ports.map((port) => new Player(port));
  }

  /** Players the pool has right now. */
  get size(): number {
    return this.players.length;
  }

  /** Size of each player's WASM memory. */
  wasmBytes(): number[] {
    return this.players.map((p) => p.wasmBytes);
  }

  /** The world worker has another world: the copies must be loaded again. */
  markStale(): void {
    this.stale = true;
  }

  private drop(player: Player, why: string): void {
    const at = this.players.indexOf(player);
    if (at >= 0) this.players.splice(at, 1);
    player.close();
    this.dropped.push(why);
  }

  /** Gives every player the whole world of `host`. A player that fails is dropped. */
  private async load(host: WorldHost): Promise<void> {
    const world: MatchRequest = {
      kind: 'load',
      seed: host.seed(),
      day: host.day(),
      userClub: host.user_club(),
      clubRows: host.club_rows(),
      lineups: host.lineups(),
      sheets: host.sheets(),
      dynamics: host.dynamics(),
      fixtureRows: host.fixture_rows(),
      matchSeeds: host.match_seeds(),
    };
    await Promise.all(
      [...this.players].map(async (player) => {
        try {
          const reply = await player.ask(world, this.timeoutMs);
          if (reply.kind !== 'loaded') throw new Error(reply.kind === 'failed' ? reply.message : reply.kind);
          player.wasmBytes = reply.wasmBytes;
        } catch (err: unknown) {
          this.drop(player, `load: ${err instanceof Error ? err.message : String(err)}`);
        }
      }),
    );
    this.stale = false;
  }

  /**
   * Plays `matches`, the matches of the day `host` is on. Each free player
   * takes the next match; what no player played — there is none, or one
   * failed — `host` plays itself. `onPlayed` is called as each match ends.
   *
   * `stop()` turning true stops new matches from being dealt; the ones
   * under way end. Returns `undefined` when the day was left incomplete
   * that way (nothing of it is kept anywhere).
   */
  async playDay(
    host: WorldHost,
    matches: Uint32Array,
    onPlayed: (done: number) => void,
    stop: () => boolean,
    /** Gives the event loop a turn (so that a cancel is heard). */
    pause: () => Promise<void>,
  ): Promise<DayPlayed | undefined> {
    const rows: Uint32Array[] = [];
    let matchMs = 0;
    const record = (row: Uint32Array, ms: number): void => {
      rows.push(row);
      matchMs += ms;
      onPlayed(rows.length);
    };
    if (matches.length === 0) return { rows, matchMs };

    if (this.players.length > 0) {
      if (this.stale) await this.load(host);
      const day: MatchRequest = {
        kind: 'sync',
        day: host.day(),
        dynamics: host.dynamics(),
        clubRows: host.club_rows(),
        lineups: host.lineups(),
      };
      for (const player of this.players) player.tell(day);
      // One queue for everybody: a player takes the next match when free.
      let next = 0;
      await Promise.all(
        [...this.players].map(async (player) => {
          while (!stop()) {
            const id = matches[next];
            if (id === undefined) return;
            next += 1;
            try {
              const reply = await player.ask({ kind: 'play', id }, this.timeoutMs);
              if (reply.kind !== 'played' || reply.id !== id) {
                throw new Error(reply.kind === 'failed' ? reply.message : `resposta inesperada: ${reply.kind}`);
              }
              record(reply.row, reply.ms);
            } catch (err: unknown) {
              // The match is not lost: `host` plays it below.
              this.drop(player, `play ${id}: ${err instanceof Error ? err.message : String(err)}`);
              return;
            }
          }
        }),
      );
    }

    // What nobody played: everything, without a pool; what a failed player
    // left, with one.
    const played = new Set(rows.map((row) => row[0]));
    for (const id of matches) {
      if (played.has(id)) continue;
      // After the last match the day is ended all the same: cancelling on
      // the eve of the commit would save a few milliseconds and nothing else.
      if (stop()) return undefined;
      const before = performance.now();
      const row = host.play(id);
      record(row, performance.now() - before);
      await pause();
    }
    return rows.length === matches.length ? { rows, matchMs } : undefined;
  }
}
