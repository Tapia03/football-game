// The page's side of the world worker (spec Fase 7B, 7B.4).

import type { Database } from '../save/client';
import type {
  WorldControl,
  WorldError,
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
  /** Kills the worker (what a crash or a closed tab does). */
  terminate(): void;
};

/** How many times the worker is started before giving up, and the pause. */
const START_ATTEMPTS = 5;
const START_RETRY_MS = 250;

/**
 * Starts the world worker and hands it a line to `database`. Resolves once
 * the worker has its WASM module up.
 *
 * The start is retried, like the database worker's: WebKit refuses to load
 * a worker script ("blocked by Cross-Origin-Embedder-Policy") when the same
 * script was loaded a moment before, and loads it fine a second later.
 */
export async function startWorld(database: Database): Promise<WorldHandle> {
  let failure: unknown;
  for (let attempt = 0; attempt < START_ATTEMPTS; attempt += 1) {
    if (attempt > 0) await new Promise((resolve) => setTimeout(resolve, START_RETRY_MS * attempt));
    try {
      return await startOnce(database);
    } catch (err: unknown) {
      failure = err;
    }
  }
  throw failure;
}

function startOnce(database: Database): Promise<WorldHandle> {
  const worker = new Worker(new URL('../workers/world.worker.ts', import.meta.url), {
    type: 'module',
  });
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
    terminate: () => worker.terminate(),
  };
  return new Promise((resolve, reject) => {
    worker.addEventListener('message', (e: MessageEvent<WorldResponse | WorldReady | WorldProgress>) => {
      const message = e.data;
      if ('type' in message) {
        if (message.type === 'ready') resolve(handle);
        else for (const listener of listeners) listener(message);
        return;
      }
      const waiting = pending.get(message.id);
      if (waiting === undefined) return;
      pending.delete(message.id);
      if (message.ok) waiting.resolve(message.result as never);
      else waiting.reject(new WorldOpError(message.error));
    });
    worker.addEventListener('error', (e) => {
      worker.terminate();
      reject(new Error(e.message || 'o Worker de mundo não carregou'));
    });
    const port = database.connect();
    const start: WorldControl = { type: 'start', db: port };
    worker.postMessage(start, [port]);
  });
}
