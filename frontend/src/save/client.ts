// The page's (or another worker's) side of the database worker (spec Fase
// 7A): typed requests over a Worker or a MessagePort, one promise each.

import type {
  DbControl,
  DbError,
  DbOp,
  DbOps,
  DbOptions,
  DbReady,
  DbRequest,
  DbResponse,
} from './protocol';

/** A failed operation: `code` tells the cases apart, `message` is for people. */
export class DbOpError extends Error {
  readonly code: DbError['code'];

  constructor(error: DbError) {
    super(error.message);
    this.code = error.code;
  }
}

type Line = {
  start?(): void;
  postMessage(message: DbRequest | DbControl, transfer?: Transferable[]): void;
  addEventListener(type: 'message', listener: (e: MessageEvent<DbResponse | DbReady>) => void): void;
};

export class DbClient {
  private nextId = 1;
  private readonly pending = new Map<
    number,
    { resolve: (value: never) => void; reject: (reason: DbOpError) => void }
  >();

  /** `line`: the database worker itself, or a port it was handed (`connect`). */
  constructor(private readonly line: Line) {
    // A MessagePort only delivers once started (a Worker has no `start`).
    if ('start' in line && typeof line.start === 'function') line.start();
    line.addEventListener('message', (e) => {
      const message = e.data;
      if ('type' in message) return;
      const waiting = this.pending.get(message.id);
      if (waiting === undefined) return;
      this.pending.delete(message.id);
      if (message.ok) waiting.resolve(message.result as never);
      else waiting.reject(new DbOpError(message.error));
    });
  }

  request<O extends DbOp>(op: O, args: DbOps[O]['args']): Promise<DbOps[O]['result']> {
    const id = this.nextId;
    this.nextId += 1;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.line.postMessage({ id, op, args });
    });
  }
}

export type Database = {
  readonly client: DbClient;
  /**
   * A port to the same database for another worker: transfer it there and
   * wrap it in a `DbClient`. Requests on it never pass through the page.
   */
  connect(): MessagePort;
  /** Kills the worker (what a crash or a closed tab does). */
  terminate(): void;
};

/** Starts the database worker; resolves once it has opened its storage. */
export function startDatabase(options: DbOptions = {}): Promise<Database> {
  const worker = new Worker(new URL('../workers/db.worker.ts', import.meta.url), {
    type: 'module',
  });
  const client = new DbClient(worker);
  return new Promise((resolve, reject) => {
    worker.addEventListener('message', (e: MessageEvent<DbResponse | DbReady>) => {
      if ('type' in e.data) {
        resolve({
          client,
          connect: () => {
            const channel = new MessageChannel();
            const control: DbControl = { type: 'connect', port: channel.port1 };
            worker.postMessage(control, [channel.port1]);
            return channel.port2;
          },
          terminate: () => worker.terminate(),
        });
      }
    });
    worker.addEventListener('error', (e) => reject(new Error(e.message)));
    const init: DbControl = { type: 'init', options };
    worker.postMessage(init);
  });
}
