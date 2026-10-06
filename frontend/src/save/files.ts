// Where the SQLite files of the database worker live (spec Fase 7A). Two
// backends behind one interface:
//
//  - `opfs`: the `opfs-sahpool` VFS — real SQLite files in the origin
//    private file system, journalled by SQLite itself.
//  - `idb`: the fallback where OPFS is missing (private Safari, the WebKit
//    of the test runner). Databases live in memory and the whole file is
//    stored as one blob in IndexedDB when a change is flushed; a file is a
//    few hundred kilobytes, and an IndexedDB transaction is atomic.
//
// A file of the `idb` backend is checked when it is opened (spec Fase 7B,
// 7B.5): the SHA-256 written beside its bytes says whether IndexedDB gave
// back what was stored, and `PRAGMA integrity_check` says whether what was
// stored is a sound database. A file that fails either is refused, saying
// which of the two it failed.

import type { Database, SAHPoolUtil, Sqlite3Static } from '@sqlite.org/sqlite-wasm';
import { FileCorruptError } from './errors';
import type { OpenCheck, StorageBackend } from './protocol';

export type OpenFile = { readonly name: string; readonly db: Database };

export type Files = {
  readonly backend: Exclude<StorageBackend, 'none'>;
  /** Names of the files that exist, sorted. */
  list(): Promise<string[]>;
  /** Opens (creating it when absent) the file `name`. */
  open(name: string): Promise<Database>;
  /**
   * Makes the committed state of `changed` durable and removes `remove`,
   * as one step where the backend can (IndexedDB: one transaction). With
   * `opfs` every commit is already on disk; only the removals happen.
   */
  flush(changed: readonly OpenFile[], remove?: readonly string[]): Promise<void>;
  /** Copies the closed file `from` over `to`. */
  copy(from: string, to: string): Promise<void>;
  /** The bytes of the closed file `name` as last made durable. */
  read(name: string): Promise<Uint8Array>;
  /** Creates (or replaces) the closed file `name` with `bytes`, durably. */
  write(name: string, bytes: Uint8Array): Promise<void>;
  /** The bytes of an open database: the SQLite file it would be on disk. */
  export(db: Database): Uint8Array;
  /** Makes room for `saves` saves (a no-op where files need no slots). */
  reserve(saves: number): Promise<void>;
  /** What checking the file opened last found and cost (`idb` only). */
  lastOpen(): OpenCheck | undefined;
};

/** Pool slots kept free beyond what the saves use (journals, copies). */
const SPARE_SLOTS = 6;

/**
 * The `opfs-sahpool` backend. Rejects when the browser has no usable OPFS
 * (or another tab holds the pool).
 */
export async function openOpfs(sqlite3: Sqlite3Static): Promise<Files> {
  let pool: SAHPoolUtil | undefined;
  let failure: unknown;
  // The handles of a worker that was just terminated (a reload, a crash)
  // take a moment to be released: retry before giving up. A browser with
  // no OPFS at all fails the same way every time, so that case is quick.
  for (let attempt = 0; attempt < 20 && pool === undefined; attempt += 1) {
    try {
      pool = await sqlite3.installOpfsSAHPoolVfs({
        name: 'fm-saves',
        directory: '.fm-saves',
        initialCapacity: SPARE_SLOTS + 4,
      });
    } catch (err: unknown) {
      failure = err;
      if (err instanceof Error && /missing required opfs/i.test(err.message)) break;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
  }
  if (pool === undefined) throw failure instanceof Error ? failure : new Error(String(failure));
  const sah = pool;
  const remove = (name: string): void => {
    sah.unlink(name);
    sah.unlink(`${name}-journal`);
  };
  return {
    backend: 'opfs',
    list: () => Promise.resolve(sah.getFileNames().sort()),
    open: (name) => Promise.resolve(new sah.OpfsSAHPoolDb(name)),
    flush: (_changed, removed = []) => {
      removed.forEach(remove);
      return Promise.resolve();
    },
    copy: (from, to) => {
      const bytes = sah.exportFile(from);
      remove(to);
      sah.importDb(to, bytes);
      return Promise.resolve();
    },
    read: (name) => Promise.resolve(sah.exportFile(name)),
    write: async (name, bytes) => {
      remove(name);
      await sah.importDb(name, bytes);
    },
    export: (db) => sqlite3.capi.sqlite3_js_db_export(db),
    reserve: async (saves) => {
      // A save takes two slots (file and journal).
      await sah.reserveMinimumCapacity(2 * (saves + 1) + SPARE_SLOTS);
    },
    lastOpen: () => undefined,
  };
}

const IDB_NAME = 'fm-saves';
const IDB_STORE = 'files';

function request<T>(r: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(r.error ?? new Error('IndexedDB request failed'));
  });
}

function done(tx: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error ?? new Error('IndexedDB transaction failed'));
    tx.onabort = () => reject(tx.error ?? new Error('IndexedDB transaction aborted'));
  });
}

/**
 * A file as IndexedDB holds it: its bytes and their SHA-256 (hex). A file
 * stored before the sum existed (Fase 7A, 7B up to 7B.4c) is the bytes
 * alone: it has no sum to be checked against, which is not a fault.
 */
type Stored = { readonly bytes: Uint8Array; readonly sha256?: string };

async function sha256(bytes: Uint8Array): Promise<string> {
  // A copy with a buffer of its own: what is hashed cannot be a view of
  // memory somebody else may change meanwhile.
  const digest = await crypto.subtle.digest('SHA-256', new Uint8Array(bytes));
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, '0')).join('');
}

/** The value kept in IndexedDB for `bytes`: with their sum. */
async function summed(bytes: Uint8Array): Promise<Stored> {
  return { bytes, sha256: await sha256(bytes) };
}

/** The IndexedDB fallback. Rejects when IndexedDB is unusable too. */
export async function openIdb(sqlite3: Sqlite3Static): Promise<Files> {
  const opening = indexedDB.open(IDB_NAME, 1);
  opening.onupgradeneeded = () => {
    opening.result.createObjectStore(IDB_STORE);
  };
  const idb = await request(opening);
  let lastOpen: OpenCheck | undefined;
  /** What is stored under `name`, in either form it may have been stored in. */
  const read = async (name: string): Promise<Stored | undefined> => {
    const value: unknown = await request(idb.transaction(IDB_STORE).objectStore(IDB_STORE).get(name));
    if (value instanceof Uint8Array) return { bytes: value };
    if (typeof value !== 'object' || value === null) return undefined;
    const { bytes, sha256: sum } = value as { bytes?: unknown; sha256?: unknown };
    if (!(bytes instanceof Uint8Array)) return undefined;
    return typeof sum === 'string' ? { bytes, sha256: sum } : { bytes };
  };
  return {
    backend: 'idb',
    list: async () => {
      const keys = await request(idb.transaction(IDB_STORE).objectStore(IDB_STORE).getAllKeys());
      return keys.map(String).sort();
    },
    open: async (name) => {
      const stored = await read(name);
      const db = new sqlite3.oo1.DB(':memory:');
      if (stored === undefined || stored.bytes.length === 0) return db;
      const { bytes } = stored;
      // First: are these the bytes that were stored? The sum is computed by
      // the browser, outside SQLite, so it holds whatever state SQLite is in.
      const beforeSum = performance.now();
      const sum = stored.sha256 === undefined ? undefined : await sha256(bytes);
      const sumMs = performance.now() - beforeSum;
      if (sum !== stored.sha256) {
        db.close();
        throw new FileCorruptError(
          name,
          'sum',
          `a soma de verificação não bate: o IndexedDB devolveu ${bytes.length} bytes com SHA-256 ${sum?.slice(0, 16)}…, e a soma gravada com eles é ${stored.sha256?.slice(0, 16)}…`,
        );
      }
      // SQLite takes ownership of the copy (freed when the db closes).
      const p = sqlite3.wasm.allocFromTypedArray(bytes);
      const { capi } = sqlite3;
      const rc = capi.sqlite3_deserialize(
        db,
        'main',
        p,
        bytes.length,
        bytes.length,
        capi.SQLITE_DESERIALIZE_FREEONCLOSE | capi.SQLITE_DESERIALIZE_RESIZEABLE,
      );
      db.checkRc(rc);
      // Then: are they a sound database? `sqlite3_deserialize` takes any
      // bytes; without this a bad page is only found by the query that
      // happens to touch it.
      const said = stored.sha256 === undefined ? 'o arquivo não tem soma gravada' : 'a soma de verificação confere';
      const beforeCheck = performance.now();
      let sound: string;
      try {
        sound = db.selectValues('PRAGMA integrity_check').map(String).join('; ');
      } catch (err: unknown) {
        sound = err instanceof Error ? err.message : String(err);
      }
      const integrityMs = performance.now() - beforeCheck;
      lastOpen = { file: name, bytes: bytes.length, summed: stored.sha256 !== undefined, sumMs, integrityMs };
      if (sound !== 'ok') {
        db.close();
        throw new FileCorruptError(
          name,
          'integrity',
          `${said} (${bytes.length} bytes), mas o banco está malformado — integrity_check: ${sound.slice(0, 400)}`,
        );
      }
      return db;
    },
    flush: async (changed, removed = []) => {
      // The sums first: an IndexedDB transaction does not wait for anything
      // but its own requests.
      const files = await Promise.all(
        changed.map(async ({ name, db }) => ({ name, value: await summed(sqlite3.capi.sqlite3_js_db_export(db)) })),
      );
      const tx = idb.transaction(IDB_STORE, 'readwrite');
      const store = tx.objectStore(IDB_STORE);
      for (const { name, value } of files) store.put(value, name);
      for (const name of removed) store.delete(name);
      await done(tx);
    },
    copy: async (from, to) => {
      // As it is stored, sum and all: a copy is checked when it is opened.
      const stored = await read(from);
      const tx = idb.transaction(IDB_STORE, 'readwrite');
      if (stored === undefined) tx.objectStore(IDB_STORE).delete(to);
      else tx.objectStore(IDB_STORE).put(stored, to);
      await done(tx);
    },
    read: async (name) => (await read(name))?.bytes ?? new Uint8Array(0),
    write: async (name, bytes) => {
      const value = await summed(bytes);
      const tx = idb.transaction(IDB_STORE, 'readwrite');
      tx.objectStore(IDB_STORE).put(value, name);
      await done(tx);
    },
    export: (db) => sqlite3.capi.sqlite3_js_db_export(db),
    reserve: () => Promise.resolve(),
    lastOpen: () => lastOpen,
  };
}
