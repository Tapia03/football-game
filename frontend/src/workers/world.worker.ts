// World worker (spec Fase 7B, 7B.4): the only place of the page where the
// world lives. It holds the `WorldHost` (WASM), simulates each day one
// match at a time in LOD Abstract, and asks the database worker — over the
// port the page hands it — to write the world and to end each day. The
// database is always the truth: what this worker holds is the world as last
// committed, plus the day being lived.

import init, { WorldHost } from '../engine-bridge/pkg/fm_wasm.js';
import { DbClient, DbOpError } from '../save/client';
import type { DayCommit, WorldSave } from '../save/protocol';
import type {
  DayTiming,
  WorldControl,
  WorldError,
  WorldOp,
  WorldOps,
  WorldProgress,
  WorldReady,
  WorldRequest,
  WorldResponse,
  WorldSummary,
} from '../world/protocol';

// Row layouts of `WorldHost` (crates/fm-wasm/src/world.rs).
const CLUB_ROW = 6;
const PLAYER_ROW = 8;
const FIXTURE_ROW = 11;
const RESULT_ROW = 7;
/** `club` of a player without one. */
const NO_CLUB = 0xffff_ffff;

/** A failed operation of this worker, with a code the page can act on. */
class WorldOpError extends Error {
  constructor(
    readonly code: WorldError['code'],
    message: string,
  ) {
    super(message);
  }
}

let wasm: Promise<unknown> | undefined;
let db: DbClient | undefined;
let host: WorldHost | undefined;
/** How long the database may take to answer a request (ms). */
let dbTimeoutMs = 10_000;
/** An advance is under way; a cancel was asked for during it. */
let advancing = false;
let cancelRequested = false;

/**
 * The line to the database, with a deadline on every request: a database
 * worker that died never answers, and an advance must not hang on it.
 */
function database(): Pick<DbClient, 'request'> {
  const client = db;
  if (client === undefined) throw new WorldOpError('internal', 'o Worker de mundo não recebeu a porta do banco');
  return {
    request: (op, args, transfer) =>
      new Promise((resolve, reject) => {
        const deadline = setTimeout(() => {
          reject(
            new WorldOpError('db-timeout', `o banco não respondeu a ${op} em ${(dbTimeoutMs / 1000).toFixed(1)} s`),
          );
        }, dbTimeoutMs);
        client.request(op, args, transfer).then(
          (value) => {
            clearTimeout(deadline);
            resolve(value);
          },
          (err: unknown) => {
            clearTimeout(deadline);
            reject(err instanceof Error ? err : new Error(String(err)));
          },
        );
      }),
  };
}

function world(): WorldHost {
  if (host === undefined) {
    throw new WorldOpError('no-world-loaded', 'nenhum mundo carregado: crie ou abra um antes');
  }
  return host;
}

function summary(h: WorldHost): WorldSummary {
  return {
    seed: h.seed(),
    day: h.day(),
    seasonYear: h.season_year(),
    userClub: h.user_club(),
    nextRound: h.next_round(),
    matchesToday: h.matches_today().length,
    finished: h.finished(),
  };
}

/** The six numbers of a result, from six words of a row. */
function result(row: Uint32Array, at: number): WorldSave['fixtures'][number]['result'] & object {
  return {
    homeGoals: row[at] ?? 0,
    awayGoals: row[at + 1] ?? 0,
    homeShots: row[at + 2] ?? 0,
    awayShots: row[at + 3] ?? 0,
    homeOnTarget: row[at + 4] ?? 0,
    awayOnTarget: row[at + 5] ?? 0,
  };
}

/** The world as the database takes it: rows as objects, blobs as buffers. */
function saveOf(h: WorldHost): { world: WorldSave; transfer: ArrayBuffer[] } {
  const clubRows = h.club_rows();
  const clubNames = h.club_names().split('\n');
  const clubShort = h.club_short_names().split('\n');
  const playerRows = h.player_rows();
  const playerNames = h.player_names().split('\n');
  const fixtureRows = h.fixture_rows();
  // Each array the WASM side returns is a fresh copy with a buffer of its
  // own: it can be handed over whole.
  const buffers = {
    lineups: h.lineups().buffer as ArrayBuffer,
    sheets: h.sheets().buffer as ArrayBuffer,
    dynamics: h.dynamics().buffer as ArrayBuffer,
    matchSeeds: h.match_seeds().buffer as ArrayBuffer,
  };
  const save: WorldSave = {
    seed: h.seed(),
    day: h.day(),
    seasonYear: h.season_year(),
    userClub: h.user_club(),
    clubs: clubNames.map((name, c) => ({
      name,
      shortName: clubShort[c] ?? '',
      strength: clubRows[c * CLUB_ROW] ?? 0,
      formation: clubRows[c * CLUB_ROW + 1] ?? 0,
      mentality: clubRows[c * CLUB_ROW + 2] ?? 0,
      pressing: clubRows[c * CLUB_ROW + 3] ?? 0,
      width: clubRows[c * CLUB_ROW + 4] ?? 0,
      lineHeight: clubRows[c * CLUB_ROW + 5] ?? 0,
    })),
    players: playerNames.map((name, p) => {
      const at = p * PLAYER_ROW;
      const club = playerRows[at] ?? NO_CLUB;
      return {
        club: club === NO_CLUB ? null : club,
        name,
        position: playerRows[at + 1] ?? 0,
        birthYear: playerRows[at + 2] ?? 0,
        overall: playerRows[at + 3] ?? 0,
        potential: playerRows[at + 4] ?? 0,
        condition: playerRows[at + 5] ?? 0,
        morale: playerRows[at + 6] ?? 0,
        injuryWeeks: playerRows[at + 7] ?? 0,
      };
    }),
    fixtures: Array.from({ length: fixtureRows.length / FIXTURE_ROW }, (_, m) => {
      const at = m * FIXTURE_ROW;
      return {
        round: fixtureRows[at] ?? 0,
        day: fixtureRows[at + 1] ?? 0,
        home: fixtureRows[at + 2] ?? 0,
        away: fixtureRows[at + 3] ?? 0,
        result: (fixtureRows[at + 4] ?? 0) === 0 ? null : result(fixtureRows, at + 5),
      };
    }),
    ...buffers,
  };
  return { world: save, transfer: Object.values(buffers) };
}

/** The host a save of the database holds. */
function hostOf(save: WorldSave): WorldHost {
  const clubRows = new Uint8Array(save.clubs.length * CLUB_ROW);
  save.clubs.forEach((c, i) => {
    clubRows.set([c.strength, c.formation, c.mentality, c.pressing, c.width, c.lineHeight], i * CLUB_ROW);
  });
  const fixtureRows = new Uint32Array(save.fixtures.length * FIXTURE_ROW);
  save.fixtures.forEach((f, i) => {
    const r = f.result;
    fixtureRows.set(
      [
        f.round,
        f.day,
        f.home,
        f.away,
        r === null ? 0 : 1,
        r?.homeGoals ?? 0,
        r?.awayGoals ?? 0,
        r?.homeShots ?? 0,
        r?.awayShots ?? 0,
        r?.homeOnTarget ?? 0,
        r?.awayOnTarget ?? 0,
      ],
      i * FIXTURE_ROW,
    );
  });
  try {
    return WorldHost.load(
      BigInt(save.seed),
      save.day,
      save.userClub,
      clubRows,
      new Uint8Array(save.lineups),
      new Uint8Array(save.sheets),
      new Uint8Array(save.dynamics),
      fixtureRows,
      new Uint8Array(save.matchSeeds),
    );
  } catch (err: unknown) {
    // The Rust side refused the pieces: this save is not a world.
    throw new WorldOpError('invalid', `o save não é um mundo desta liga: ${String(err)}`);
  }
}

/** The end of the day the host has just finished, as the database takes it. */
function commitOf(h: WorldHost): { commit: DayCommit; transfer: ArrayBuffer[] } {
  const rows = h.day_results();
  const playerRows = h.player_rows();
  const dynamics = h.dynamics().buffer as ArrayBuffer;
  const commit: DayCommit = {
    day: h.day(),
    results: Array.from({ length: rows.length / RESULT_ROW }, (_, i) => ({
      id: rows[i * RESULT_ROW] ?? 0,
      ...result(rows, i * RESULT_ROW + 1),
    })),
    dynamics,
    players: Array.from({ length: playerRows.length / PLAYER_ROW }, (_, p) => ({
      condition: playerRows[p * PLAYER_ROW + 5] ?? 0,
      morale: playerRows[p * PLAYER_ROW + 6] ?? 0,
      injuryWeeks: playerRows[p * PLAYER_ROW + 7] ?? 0,
    })),
  };
  return { commit, transfer: [dynamics] };
}

// Giving the event loop a turn between two matches, so that a message that
// arrived meanwhile is handled. A message channel, not `setTimeout`, which
// browsers hold back for 4 ms once calls nest.
const turn = new MessageChannel();
let resume: (() => void) | undefined;
turn.port1.onmessage = () => resume?.();
function yieldTurn(): Promise<void> {
  return new Promise((resolve) => {
    resume = resolve;
    turn.port2.postMessage(0);
  });
}

/** Matches played and time spent playing them in the advance under way. */
type Pace = { matches: number; ms: number };

/** Loads the world of the open save from the database. */
async function open(): Promise<WorldHost> {
  const loaded = hostOf(await database().request('world.load', {}));
  host?.free();
  host = loaded;
  return loaded;
}

/**
 * Lives one day. If the database refuses the end of the day, the world in
 * memory is one day ahead of the truth: it is thrown away and loaded again.
 *
 * `'cancelled'`: a cancel arrived with matches of the day still to play.
 * The day was abandoned — `play` only reads the world, so nothing changed
 * and nothing went to the database.
 */
async function liveDay(h: WorldHost, daysLeft: number, pace: Pace): Promise<DayTiming | undefined | 'cancelled'> {
  const day = h.day();
  const round = h.next_round();
  const matches = h.matches_today();
  const started = performance.now();
  let simulating = 0;
  for (const [index, id] of matches.entries()) {
    const before = performance.now();
    h.play(id);
    const took = performance.now() - before;
    simulating += took;
    pace.matches += 1;
    pace.ms += took;
    const done = index + 1;
    const progress: WorldProgress = {
      type: 'progress',
      day,
      round,
      done,
      total: matches.length,
      daysLeft,
      etaMs: (pace.ms / pace.matches) * (matches.length - done),
    };
    postMessage(progress);
    // One match at a time: the page hears about each as it ends, and a
    // cancel sent meanwhile is heard here.
    await yieldTurn();
    // After the last match the day is ended all the same: cancelling on the
    // eve of the commit would save a few milliseconds and nothing else.
    if (cancelRequested && done < matches.length) {
      h.abandon_day();
      return 'cancelled';
    }
  }
  const simulated = started + simulating;
  h.finish_day();
  const { commit, transfer } = commitOf(h);
  const committing = performance.now();
  try {
    await database().request('world.commitDay', { commit }, transfer);
  } catch (err: unknown) {
    host = undefined;
    h.free();
    await open().catch(() => undefined);
    throw err;
  }
  const ended = performance.now();
  if (matches.length === 0) return undefined;
  return {
    day,
    round,
    matches: matches.length,
    simulateMs: simulating,
    commitMs: ended - committing,
    totalMs: ended - started,
  };
}

const handlers: { [O in WorldOp]: (args: WorldOps[O]['args']) => Promise<WorldOps[O]['result']> } = {
  'world.new': async ({ seed, userClub }) => {
    if (!/^\d{1,20}$/.test(seed)) throw new WorldOpError('invalid', 'a seed é um inteiro sem sinal em decimal');
    if (!Number.isInteger(userClub) || userClub < 0 || userClub > 19) {
      throw new WorldOpError('invalid', 'o clube do usuário é um dos 20 da liga');
    }
    const made = new WorldHost(BigInt.asUintN(64, BigInt(seed)), userClub);
    const { world: save, transfer } = saveOf(made);
    try {
      await database().request('world.create', { world: save }, transfer);
    } catch (err: unknown) {
      made.free();
      throw err;
    }
    host?.free();
    host = made;
    return summary(made);
  },

  'world.open': async () => summary(await open()),

  'world.advance': async ({ days }) => {
    if (!Number.isInteger(days) || days < 1) throw new WorldOpError('invalid', 'avançar pede ao menos um dia');
    const timing: DayTiming[] = [];
    const pace: Pace = { matches: 0, ms: 0 };
    let daysLived = 0;
    cancelRequested = false;
    advancing = true;
    try {
      while (daysLived < days && !world().finished() && !cancelRequested) {
        const lived = await liveDay(world(), days - daysLived - 1, pace);
        if (lived === 'cancelled') break;
        if (lived !== undefined) timing.push(lived);
        daysLived += 1;
      }
    } finally {
      advancing = false;
    }
    const cancelled = cancelRequested && daysLived < days && !world().finished();
    cancelRequested = false;
    return { summary: summary(world()), daysLived, cancelled, timing };
  },
};

function toError(err: unknown): WorldError {
  if (err instanceof WorldOpError) return { code: err.code, message: err.message };
  // What the database said passes on as it came (`no-world`, `world-exists`…).
  if (err instanceof DbOpError) return { code: err.code, message: err.message };
  return { code: 'internal', message: err instanceof Error ? err.message : String(err) };
}

/** Operations are served one at a time, in the order they arrive. */
let queue: Promise<unknown> = Promise.resolve();

async function serve<O extends WorldOp>(request: WorldRequest<O>): Promise<WorldResponse<O>> {
  try {
    await wasm;
    const handler = handlers[request.op] as (args: WorldOps[O]['args']) => Promise<WorldOps[O]['result']>;
    return { id: request.id, ok: true, result: await handler(request.args) };
  } catch (err: unknown) {
    return { id: request.id, ok: false, error: toError(err) };
  }
}

addEventListener('message', (e: MessageEvent<WorldControl | WorldRequest>) => {
  const message = e.data;
  if ('type' in message) {
    if (message.type === 'cancel') {
      // Heard between two matches; only an advance under way can be stopped.
      if (advancing) cancelRequested = true;
      return;
    }
    db = new DbClient(message.db);
    if (message.dbTimeoutMs !== undefined) dbTimeoutMs = message.dbTimeoutMs;
    wasm ??= init();
    void wasm.then(
      () => postMessage({ type: 'ready' } satisfies WorldReady),
      (err: unknown) => {
        throw err;
      },
    );
    return;
  }
  const served = queue.then(() => serve(message));
  queue = served;
  void served.then((response) => postMessage(response));
});
