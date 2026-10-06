// The page's side of the world worker (spec Fase 7B, 7B.4) and of its pool
// of match workers (7B.4b). The page creates every worker; the world
// worker gets a port to the database and one to each match worker.

import type { Database } from '../save/client';
import type { MatchControl, MatchReady } from './match-protocol';
import type {
  WorldControl,
  WorldError,
  WorldLoaded,
  WorldOp,
  WorldOps,
  WorldProgress,
  WorldReady,
  WorldRequest,
  WorldResponse,
} from './protocol';

/** A failed operation: `code` tells the cases apart, `message` is for people. */
export class WorldOpError extends Error {
  readonly code: WorldError['code'];

  constructor(error: WorldError) {
    super(error.message);
    this.code = error.code;
  }
}

export type WorldHandle = {
  request<O extends WorldOp>(op: O, args: WorldOps[O]['args']): Promise<WorldOps[O]['result']>;
  /** Calls `listener` after every match of an advance; returns how to stop listening. */
  onProgress(listener: (progress: WorldProgress) => void): () => void;
  /** Stops the advance under way after the matches being played. */
  cancel(): void;
  /** Match workers started for the pool (0: the world worker plays alone). */
  readonly players: number;
  /** Kills match worker `index` (what a crash does): the pool must go on without it. */
  killPlayer(index: number): void;
  /** Test hook: makes match worker `index` throw, as a defect in it would. */
  crashPlayer(index: number): void;
  /** Kills the world worker and its pool (what a crash or a closed tab does). */
  terminate(): void;
};

export type WorldOptions = {
  /** How long the database may take to answer the world worker (ms; 10 s by default). */
  readonly dbTimeoutMs?: number;
  /**
   * Match workers in the pool. By default one for each core, ten at most,
   * and none on a single core — there the world worker plays the matches
   * itself, as it does whenever the pool is empty.
   */
  readonly players?: number;
  /**
   * The most a match worker is ever waited for before it is dropped (ms;
   * 30 s by default). The wait for a match is usually far shorter: it
   * follows the time the last matches took.
   */
  readonly matchTimeoutMs?: number;
};

/** The size of the pool on this machine. */
export function defaultPlayers(): number {
  const cores = navigator.hardwareConcurrency || 1;
  return cores <= 1 ? 0 : Math.min(cores, 10);
}

/** How many times a worker is started before giving up, and the pause. */
const START_ATTEMPTS = 5;
const START_RETRY_MS = 250;

/**
 * Starts a worker and waits for the `loaded` it posts as its script runs.
 *
 * The start is retried, like the database worker's: WebKit refuses to load
 * a worker script ("blocked by Cross-Origin-Embedder-Policy") when the same
 * script was loaded a moment before — a reload, a worker just terminated,
 * or simply the other workers of the pool — and loads it fine a moment
 * later. Nothing is handed to a worker before it has said `loaded`: a port
 * transferred to a worker that never loads is lost.
 */
async function spawn(create: () => Worker, what: string): Promise<Worker> {
  let failure: unknown;
  for (let attempt = 0; attempt < START_ATTEMPTS; attempt += 1) {
    if (attempt > 0) await new Promise((resolve) => setTimeout(resolve, START_RETRY_MS * attempt));
    try {
      return await new Promise<Worker>((resolve, reject) => {
        const worker = create();
        const loaded = (e: MessageEvent<WorldLoaded>): void => {
          if (e.data.type !== 'loaded') return;
          worker.removeEventListener('message', loaded);
          resolve(worker);
        };
        worker.addEventListener('message', loaded);
        worker.addEventListener('error', (e) => {
          worker.terminate();
          reject(new Error(e.message || `${what} não carregou`));
        });
      });
    } catch (err: unknown) {
      failure = err;
    }
  }
  throw failure;
}

/** A match worker of the pool, as the page holds it. */
type StartedPlayer = {
  readonly worker: Worker;
  readonly port: MessagePort;
  /** A failure seen before there was anyone to tell. */
  failed?: string;
  /** Tells the world worker that this worker failed (set once it can be told). */
  report?: (why: string) => void;
};

/**
 * A match worker, started and holding its end of a line to the world
 * worker. The page goes on listening to it for as long as it lives: an
 * `error` of a worker and a message of it that cannot be read are only ever
 * heard here, and the world worker has to be told.
 */
async function startPlayer(): Promise<StartedPlayer> {
  const worker = await spawn(
    () => new Worker(new URL('../workers/match.worker.ts', import.meta.url), { type: 'module' }),
    'o Worker de partida',
  );
  const channel = new MessageChannel();
  const player: StartedPlayer = { worker, port: channel.port2 };
  await new Promise<void>((resolve, reject) => {
    let ready = false;
    const failed = (why: string): void => {
      if (!ready) reject(new Error(why));
      else if (player.report !== undefined) player.report(why);
      else player.failed ??= why;
    };
    worker.addEventListener('message', (e: MessageEvent<MatchReady>) => {
      if (e.data.type !== 'ready') return;
      ready = true;
      resolve();
    });
    worker.addEventListener('error', (e) => failed(`o Worker falhou: ${e.message || 'erro sem mensagem'}`));
    worker.addEventListener('messageerror', () => failed('messageerror na página'));
    const start: MatchControl = { type: 'start', port: channel.port1 };
    worker.postMessage(start, [channel.port1]);
  });
  return player;
}

/**
 * Starts the world worker — with a line to `database` and to each match
 * worker of its pool — and resolves once it has its WASM module up. A
 * match worker that does not start is left out: the pool is smaller, or
 * empty, and the world is lived all the same.
 */
export async function startWorld(database: Database, options: WorldOptions = {}): Promise<WorldHandle> {
  const worker = await spawn(
    () => new Worker(new URL('../workers/world.worker.ts', import.meta.url), { type: 'module' }),
    'o Worker de mundo',
  );
  const wanted = Math.max(0, Math.floor(options.players ?? defaultPlayers()));
  const started = await Promise.all(
    Array.from({ length: wanted }, () => startPlayer().catch(() => undefined)),
  );
  const players = started.filter((p) => p !== undefined);

  let nextId = 1;
  const pending = new Map<number, { resolve: (value: never) => void; reject: (reason: WorldOpError) => void }>();
  const listeners = new Set<(progress: WorldProgress) => void>();
  const handle: WorldHandle = {
    onProgress: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    request: (op, args) => {
      const id = nextId;
      nextId += 1;
      return new Promise((resolve, reject) => {
        pending.set(id, { resolve, reject });
        const request: WorldRequest = { id, op, args };
        worker.postMessage(request);
      });
    },
    cancel: () => {
      const cancel: WorldControl = { type: 'cancel' };
      worker.postMessage(cancel);
    },
    players: players.length,
    killPlayer: (index) => players[index]?.worker.terminate(),
    crashPlayer: (index) => {
      const crash: MatchControl = { type: 'crash' };
      players[index]?.worker.postMessage(crash);
    },
    terminate: () => {
      worker.terminate();
      for (const player of players) player.worker.terminate();
    },
  };
  return new Promise((resolve, reject) => {
    worker.addEventListener('message', (e: MessageEvent<WorldResponse | WorldReady | WorldProgress>) => {
      const message = e.data;
      if ('type' in message) {
        if (message.type === 'ready') resolve(handle);
        else if (message.type === 'progress') for (const listener of listeners) listener(message);
        return;
      }
      const waiting = pending.get(message.id);
      if (waiting === undefined) return;
      pending.delete(message.id);
      if (message.ok) waiting.resolve(message.result as never);
      else waiting.reject(new WorldOpError(message.error));
    });
    worker.addEventListener('error', (e) => {
      handle.terminate();
      reject(new Error(e.message || 'o Worker de mundo falhou'));
    });
    const db = database.connect();
    const ports = players.map((p) => p.port);
    const start: WorldControl = {
      type: 'start',
      db,
      players: ports,
      ...(options.dbTimeoutMs === undefined ? {} : { dbTimeoutMs: options.dbTimeoutMs }),
      ...(options.matchTimeoutMs === undefined ? {} : { matchTimeoutMs: options.matchTimeoutMs }),
    };
    worker.postMessage(start, [db, ...ports]);
    // From here on the world worker knows the players: a failure of one —
    // seen meanwhile, or yet to come — is passed on.
    players.forEach((player, index) => {
      player.report = (why) => {
        const failed: WorldControl = { type: 'player-failed', player: index, why };
        worker.postMessage(failed);
      };
      if (player.failed !== undefined) player.report(player.failed);
    });
  });
}
