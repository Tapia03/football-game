// Where the SQLite files of the database worker live (spec Fase 7A). Two
// backends behind one interface:
//
//  - `opfs`: the `opfs-sahpool` VFS — real SQLite files in the origin
//    private file system, journalled by SQLite itself.
//  - `idb`: the fallback where OPFS is missing (private Safari, the WebKit
//    of the test runner). Databases live in memory and the whole file is
//    stored as one blob in IndexedDB when a change is flushed; a file is a
//    few hundred kilobytes, and an IndexedDB transaction is atomic.

import type { Database, SAHPoolUtil, Sqlite3Static } from '@sqlite.org/sqlite-wasm';
import type { StorageBackend } from './protocol';

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
  /** Makes room for `saves` saves (a no-op where files need no slots). */
  reserve(saves: number): Promise<void>;
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
    reserve: async (saves) => {
      // A save takes two slots (file and journal).
      await sah.reserveMinimumCapacity(2 * (saves + 1) + SPARE_SLOTS);
    },
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

/** The IndexedDB fallback. Rejects when IndexedDB is unusable too. */
export async function openIdb(sqlite3: Sqlite3Static): Promise<Files> {
  const opening = indexedDB.open(IDB_NAME, 1);
  opening.onupgradeneeded = () => {
    opening.result.createObjectStore(IDB_STORE);
  };
  const idb = await request(opening);
  const read = async (name: string): Promise<Uint8Array | undefined> => {
    const bytes: unknown = await request(idb.transaction(IDB_STORE).objectStore(IDB_STORE).get(name));
    return bytes instanceof Uint8Array ? bytes : undefined;
  };
  return {
    backend: 'idb',
    list: async () => {
      const keys = await request(idb.transaction(IDB_STORE).objectStore(IDB_STORE).getAllKeys());
      return keys.map(String).sort();
    },
    open: async (name) => {
      const bytes = await read(name);
      const db = new sqlite3.oo1.DB(':memory:');
      if (bytes !== undefined && bytes.length > 0) {
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
      }
      return db;
    },
    flush: async (changed, removed = []) => {
      const tx = idb.transaction(IDB_STORE, 'readwrite');
      const store = tx.objectStore(IDB_STORE);
      for (const { name, db } of changed) store.put(sqlite3.capi.sqlite3_js_db_export(db), name);
      for (const name of removed) store.delete(name);
      await done(tx);
    },
    copy: async (from, to) => {
      const bytes = await read(from);
      const tx = idb.transaction(IDB_STORE, 'readwrite');
      if (bytes === undefined) tx.objectStore(IDB_STORE).delete(to);
      else tx.objectStore(IDB_STORE).put(bytes, to);
      await done(tx);
    },
    reserve: () => Promise.resolve(),
  };
}
