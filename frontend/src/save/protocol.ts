// Protocol of the database worker (spec Fase 7A). The database worker is
// the only place that opens the save files and speaks SQL; everybody else —
// the page, and later the world and engine workers through a MessagePort —
// asks for domain operations with these messages.
//
//   request:  { id, op, args }
//   response: { id, ok: true, result } | { id, ok: false, error }
//
// Large payloads travel as transferred ArrayBuffers, never copied.

/** Where the save files live. */
export type StorageBackend =
  /** SQLite files in the origin private file system (`opfs-sahpool`). */
  | 'opfs'
  /** Fallback: databases in memory, each file stored whole in IndexedDB. */
  | 'idb'
  /** No usable storage: every other operation fails. */
  | 'none';

export type StorageInfo = {
  readonly backend: StorageBackend;
  /** SQLite version of the WASM build. */
  readonly sqlite: string;
  /** Why this is not `'opfs'`, when it is not. */
  readonly detail?: string;
};

/** One row of the catalog: a save and the file that holds it. */
export type SaveInfo = {
  readonly id: string;
  readonly name: string;
  /** ISO timestamps. */
  readonly createdAt: string;
  readonly lastOpenedAt: string;
  readonly clubName: string;
  readonly season: number;
  readonly day: number;
  /** Name of the file in the pool that holds this save right now. */
  readonly activeFile: string;
  readonly schemaVersion: number;
};

/** What `save.open` reports about the file it opened. */
export type OpenedSave = {
  readonly save: SaveInfo;
  /** Schema versions the file went through in this open (empty: none). */
  readonly migrated: readonly number[];
  /** Tables of the file, in name order. */
  readonly tables: readonly string[];
  /** Names of the migrations recorded in the file, in version order. */
  readonly migrations: readonly string[];
};

export type MetaValue = string | number | null;

/** Operations: `args` → `result`. */
export type DbOps = {
  'storage.info': { args: Record<string, never>; result: StorageInfo };
  'save.list': { args: Record<string, never>; result: readonly SaveInfo[] };
  'save.create': { args: { readonly name: string }; result: SaveInfo };
  /** Opens the save (migrating its file if needed) and makes it current. */
  'save.open': { args: { readonly id: string }; result: OpenedSave };
  /** Closes the current save, if any. */
  'save.close': { args: Record<string, never>; result: null };
  'save.delete': { args: { readonly id: string }; result: null };
  /** Reads a key of the current save's `meta` table (`null` when absent). */
  'meta.get': { args: { readonly key: string }; result: MetaValue };
  /** Writes a key of the current save's `meta` table, in one transaction. */
  'meta.set': { args: { readonly key: string; readonly value: MetaValue }; result: null };
  /**
   * Test only (`DbOptions.test`): writes a `meta` key inside a transaction
   * that is never committed, and answers. Killing the worker afterwards is
   * a crash in the middle of a write.
   */
  'test.writeWithoutCommit': {
    args: { readonly key: string; readonly value: MetaValue };
    result: null;
  };
  /** Test only: the names of the files in the pool, sorted. */
  'test.files': { args: Record<string, never>; result: readonly string[] };
};

export type DbOp = keyof DbOps;

export type DbRequest<O extends DbOp = DbOp> = {
  readonly id: number;
  readonly op: O;
  readonly args: DbOps[O]['args'];
};

export type DbResponse<O extends DbOp = DbOp> =
  | { readonly id: number; readonly ok: true; readonly result: DbOps[O]['result'] }
  | { readonly id: number; readonly ok: false; readonly error: DbError };

/** Errors a caller can tell apart; `message` is for people. */
export type DbError = {
  readonly code:
    | 'no-storage'
    | 'not-found'
    | 'no-save-open'
    | 'newer-version'
    | 'not-a-save'
    | 'migration-failed'
    | 'invalid'
    | 'internal';
  readonly message: string;
};

/** Start-up options of the worker (first message it receives). */
export type DbOptions = {
  /** `'idb'`: skip OPFS and use the IndexedDB fallback (tests, diagnosis). */
  readonly storage?: 'idb';
  /**
   * Test mode: enables the `test.*` operations and, with `migrations`,
   * appends test migrations to the save schema so that the chain can be
   * exercised before the game has a second schema version.
   *  - `'v2'`: one more migration (v1 → v2, adds a table).
   *  - `'v2-then-broken'`: that one and a v2 → v3 that fails half way.
   */
  readonly test?: { readonly migrations?: 'v2' | 'v2-then-broken' };
};

/** Control messages of the worker, besides requests. */
export type DbControl =
  | { readonly type: 'init'; readonly options: DbOptions }
  /** Hands the worker a port: requests arriving on it are served like the page's. */
  | { readonly type: 'connect'; readonly port: MessagePort };

export type DbReady = { readonly type: 'ready' };
