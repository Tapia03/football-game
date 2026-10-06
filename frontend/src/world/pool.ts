// The pool of match workers, as the world worker sees it (spec Fase 7B,
// 7B.4b). It plays the matches of a day wherever there is a free player
// and hands back their results; with no players — one core, or a pool that
// did not come up — the world worker's own host plays them, by the same
// function. Which of the two played a match never shows: `play` is pure.

import type { WorldHost } from '../engine-bridge/pkg/fm_wasm.js';
import type { MatchReply, MatchRequest } from './match-protocol';

/** The request in flight to a match worker. */
type Waiting = {
  resolve: (reply: MatchReply) => void;
  reject: (err: Error) => void;
  timer: number;
  /**
   * A `play` whose `started` has not come yet: the match, and how long its
   * result may take once it has.
   */
  unconfirmed: { readonly id: number; readonly resultMs: number } | undefined;
};

const seconds = (ms: number): string => `${(ms / 1000).toFixed(1)} s`;

/** One match worker, over its port: one request in flight at a time. */
class Player {
  /** Size of the worker's WASM memory, as it last reported it. */
  wasmBytes = 0;
  /** Why this worker cannot be used any more, once that is known. */
  private broken: string | undefined;
  private waiting: Waiting | undefined;

  constructor(
    private readonly port: MessagePort,
    /** Called when the worker fails with nothing asked of it. */
    private readonly onIdleFailure: (player: Player, why: string) => void,
  ) {
    port.onmessage = (e: MessageEvent<MatchReply>) => {
      const waiting = this.waiting;
      if (waiting === undefined) return;
      const reply = e.data;
      const unconfirmed = waiting.unconfirmed;
      if (unconfirmed !== undefined && reply.kind === 'started' && reply.id === unconfirmed.id) {
        // The match got there: from here on what is awaited is its result.
        clearTimeout(waiting.timer);
        waiting.unconfirmed = undefined;
        waiting.timer = this.deadline(unconfirmed.resultMs, 'confirmado, sem resultado em');
        return;
      }
      this.waiting = undefined;
      clearTimeout(waiting.timer);
      waiting.resolve(reply);
    };
    // A message of the worker that could not be read: whatever it said is lost.
    port.onmessageerror = () => this.fail('messageerror');
  }

  /** Fails the request in flight with `what` and the time, when `ms` go by. */
  private deadline(ms: number, what: string): number {
    return setTimeout(() => {
      const waiting = this.waiting;
      this.waiting = undefined;
      waiting?.reject(new Error(`${what} ${seconds(ms)}`));
    }, ms) as unknown as number;
  }

  /**
   * The worker is known to have failed (`why`): the request in flight fails
   * with it, at once; with none in flight, the pool is told.
   */
  fail(why: string): void {
    if (this.broken !== undefined) return;
    this.broken = why;
    const waiting = this.waiting;
    if (waiting === undefined) {
      this.onIdleFailure(this, why);
      return;
    }
    this.waiting = undefined;
    clearTimeout(waiting.timer);
    waiting.reject(new Error(why));
  }

  /** A message that is not answered (`sync`). */
  tell(request: MatchRequest): void {
    this.port.postMessage(request);
  }

  /** A message and its answer, or an error when none comes in `timeoutMs`. */
  ask(request: MatchRequest, timeoutMs: number): Promise<MatchReply> {
    return new Promise((resolve, reject) => {
      const timer = this.deadline(timeoutMs, 'o Worker de partida não respondeu em');
      this.waiting = { resolve, reject, timer, unconfirmed: undefined };
      this.port.postMessage(request);
    });
  }

  /**
   * Match `id`, in two waits: `started` — the match got to the worker — in
   * `confirmMs`, then its result in `resultMs`. The error says which of the
   * two did not come.
   */
  play(id: number, confirmMs: number, resultMs: number): Promise<MatchReply> {
    return new Promise((resolve, reject) => {
      const timer = this.deadline(confirmMs, 'sem confirmação em');
      this.waiting = { resolve, reject, timer, unconfirmed: { id, resultMs } };
      const request: MatchRequest = { kind: 'play', id };
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
  /** Every player the pool was made with, by the index the page knows it by. */
  private readonly all: readonly Player[];
  /** The players' copies of the world are not the world any more. */
  private stale = true;
  /** Players dropped since the pool was made, and why (for the record). */
  readonly dropped: string[] = [];

  constructor(
    ports: readonly MessagePort[],
    /** How long a player may take to answer one request (ms). */
    private readonly timeoutMs: number,
  ) {
    this.all = ports.map((port) => new Player(port, (player, why) => this.drop(player, `fora de partida: ${why}`)));
    this.players = [...this.all];
  }

  /**
   * The page saw match worker `index` fail (`why`). With a request in
   * flight to it, that request fails now and the player is dropped there;
   * otherwise it is dropped here. A player already dropped stays so.
   */
  fail(index: number, why: string): void {
    const player = this.all[index];
    if (player !== undefined && this.players.includes(player)) player.fail(why);
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
              const reply = await player.play(id, this.timeoutMs, this.timeoutMs);
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
