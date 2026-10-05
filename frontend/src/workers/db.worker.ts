// Database worker (spec Fase 7A): the only place of the page that opens the
// save files and speaks SQL. SQLite (official WASM build) over the
// `opfs-sahpool` VFS: synchronous access handles of the origin private file
// system, held by this worker alone. The page and the other workers ask for
// domain operations (`save/protocol.ts`), never for SQL.

import sqlite3InitModule, {
  type Database,
  type SAHPoolUtil,
  type SqlValue,
} from '@sqlite.org/sqlite-wasm';
import {
  MigrationFailedError,
  NewerVersionError,
  NotOursError,
  checkFile,
  migrate,
  schemaVersion,
  type MigratableDb,
} from '../save/migrate';
import type {
  DbControl,
  DbError,
  DbOp,
  DbOps,
  DbOptions,
  DbReady,
  DbRequest,
  DbResponse,
  MetaValue,
  OpenedSave,
  SaveInfo,
  StorageInfo,
} from '../save/protocol';
import {
  CATALOG_MIGRATIONS,
  SAVE_MIGRATIONS,
  TEST_MIGRATION_V2,
  TEST_MIGRATION_V3_BROKEN,
  type Migration,
} from '../save/schema';

const CATALOG_FILE = '/catalog.sqlite';
/** Suffix of the copy of a save made before its migration chain runs. */
const BACKUP_SUFFIX = '.bak';
/** Pool slots kept free beyond what the saves use (journals, copies). */
const SPARE_SLOTS = 6;

/** An error with a code the caller can act on. */
class OpError extends Error {
  constructor(
    readonly code: DbError['code'],
    message: string,
  ) {
    super(message);
  }
}

type Storage = {
  readonly pool: SAHPoolUtil;
  readonly catalog: Database;
};

let options: DbOptions = {};
let info: StorageInfo = { backend: 'none', sqlite: '', detail: 'not started' };
let storage: Storage | undefined;
/** The save that is open, if any. */
let current: { readonly id: string; readonly db: Database } | undefined;

const fileOf = (id: string): string => `/save-${id}.sqlite`;

function wrap(db: Database): MigratableDb {
  return {
    exec: (sql) => {
      db.exec(sql);
    },
    run: (sql, bind) => {
      db.exec({ sql, bind: [...bind] });
    },
    value: (sql) => Number(db.selectValue(sql) ?? 0),
  };
}

function saveMigrations(): readonly Migration[] {
  switch (options.test?.migrations) {
    case 'v2':
      return [...SAVE_MIGRATIONS, TEST_MIGRATION_V2];
    case 'v2-then-broken':
      return [...SAVE_MIGRATIONS, TEST_MIGRATION_V2, TEST_MIGRATION_V3_BROKEN];
    case undefined:
      return SAVE_MIGRATIONS;
  }
}

function openFile(pool: SAHPoolUtil, name: string): Database {
  const db = new pool.OpfsSAHPoolDb(name);
  db.exec('PRAGMA foreign_keys = ON');
  return db;
}

/** Removes a file of the pool and its rollback journal, if they exist. */
function removeFile(pool: SAHPoolUtil, name: string): void {
  pool.unlink(name);
  pool.unlink(`${name}-journal`);
}

/** Copies `from` over `to` (both closed). */
function copyFile(pool: SAHPoolUtil, from: string, to: string): void {
  const bytes = pool.exportFile(from);
  removeFile(pool, to);
  pool.importDb(to, bytes);
}

/**
 * A `.bak` left behind means a migration chain did not finish: the copy is
 * the save as it was, so it goes back in place.
 */
function restoreBackups(pool: SAHPoolUtil): void {
  for (const name of pool.getFileNames()) {
    if (!name.endsWith(BACKUP_SUFFIX)) continue;
    copyFile(pool, name, name.slice(0, -BACKUP_SUFFIX.length));
    removeFile(pool, name);
  }
}

async function boot(): Promise<void> {
  const sqlite3 = await sqlite3InitModule();
  const sqlite = sqlite3.version.libVersion;
  let pool: SAHPoolUtil | undefined;
  let failure: unknown;
  // The handles of a worker that was just terminated (a reload, a crash)
  // take a moment to be released: retry before giving up.
  for (let attempt = 0; attempt < 20 && pool === undefined; attempt += 1) {
    try {
      pool = await sqlite3.installOpfsSAHPoolVfs({
        name: 'fm-saves',
        directory: '.fm-saves',
        initialCapacity: SPARE_SLOTS + 4,
      });
    } catch (err: unknown) {
      failure = err;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
  }
  if (pool === undefined) {
    info = {
      backend: 'none',
      sqlite,
      detail: failure instanceof Error ? failure.message : String(failure),
    };
    return;
  }
  restoreBackups(pool);
  const catalog = openFile(pool, CATALOG_FILE);
  migrate(wrap(catalog), CATALOG_MIGRATIONS);
  storage = { pool, catalog };
  info = { backend: 'opfs', sqlite };
}

function need(): Storage {
  if (storage === undefined) {
    throw new OpError('no-storage', `sem armazenamento: ${info.detail ?? 'desconhecido'}`);
  }
  return storage;
}

function needCurrent(): Database {
  if (current === undefined) throw new OpError('no-save-open', 'nenhum save aberto');
  return current.db;
}

const text = (v: SqlValue | undefined): string => (typeof v === 'string' ? v : '');
const int = (v: SqlValue | undefined): number => (typeof v === 'number' ? v : Number(v ?? 0));

function saveInfo(row: Record<string, SqlValue>): SaveInfo {
  return {
    id: text(row['id']),
    name: text(row['name']),
    createdAt: text(row['created_at']),
    lastOpenedAt: text(row['last_opened_at']),
    clubName: text(row['club_name']),
    season: int(row['season']),
    day: int(row['day']),
    activeFile: text(row['active_file']),
    schemaVersion: int(row['schema_version']),
  };
}

function findSave(catalog: Database, id: string): SaveInfo {
  const [row] = catalog.selectObjects('SELECT * FROM saves WHERE id = ?', [id]);
  if (row === undefined) throw new OpError('not-found', `save ${id} não existe`);
  return saveInfo(row);
}

function closeCurrent(): void {
  current?.db.close();
  current = undefined;
}

function setMeta(db: Database, key: string, value: MetaValue): void {
  db.exec({
    sql: 'INSERT INTO meta (key, value) VALUES (?, ?) ON CONFLICT (key) DO UPDATE SET value = excluded.value',
    bind: [key, value],
  });
}

const handlers: { [O in DbOp]: (args: DbOps[O]['args']) => DbOps[O]['result'] | Promise<DbOps[O]['result']> } = {
  'storage.info': () => info,

  'save.list': () =>
    need().catalog.selectObjects('SELECT * FROM saves ORDER BY last_opened_at DESC, id').map(saveInfo),

  'save.create': async ({ name }) => {
    const { pool, catalog } = need();
    const trimmed = name.trim();
    if (trimmed === '') throw new OpError('invalid', 'o save precisa de um nome');
    const count = int(catalog.selectValue('SELECT count(*) FROM saves'));
    // A save takes two slots (file and journal); copies need a few more.
    await pool.reserveMinimumCapacity(2 * (count + 2) + SPARE_SLOTS);
    const id = crypto.randomUUID();
    const file = fileOf(id);
    const db = openFile(pool, file);
    let version: number;
    try {
      migrate(wrap(db), saveMigrations());
      setMeta(db, 'name', trimmed);
      version = schemaVersion(wrap(db));
    } finally {
      db.close();
    }
    const now = new Date().toISOString();
    catalog.exec({
      sql: `INSERT INTO saves (id, name, created_at, last_opened_at, active_file, schema_version)
            VALUES (?, ?, ?, ?, ?, ?)`,
      bind: [id, trimmed, now, now, file, version],
    });
    return findSave(catalog, id);
  },

  'save.open': ({ id }) => {
    const { pool, catalog } = need();
    const save = findSave(catalog, id);
    closeCurrent();
    const migrations = saveMigrations();
    const file = save.activeFile;
    const backup = `${file}${BACKUP_SUFFIX}`;
    let db = openFile(pool, file);
    let migrated: number[] = [];
    try {
      // Ours, and not from a newer game: checked before anything is touched.
      checkFile(wrap(db), migrations);
      if (schemaVersion(wrap(db)) < (migrations.at(-1)?.version ?? 0)) {
        // The save as it is, kept until the whole chain has worked.
        db.close();
        copyFile(pool, file, backup);
        db = openFile(pool, file);
        migrated = migrate(wrap(db), migrations);
        removeFile(pool, backup);
      }
    } catch (err: unknown) {
      db.close();
      if (pool.getFileNames().includes(backup)) {
        // The chain stopped half way: back to the save as it was.
        copyFile(pool, backup, file);
        removeFile(pool, backup);
      }
      throw err;
    }
    const tables = db
      .selectValues("SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
      .map(text);
    const applied = db.selectValues('SELECT name FROM migrations ORDER BY version').map(text);
    catalog.exec({
      sql: 'UPDATE saves SET last_opened_at = ?, schema_version = ? WHERE id = ?',
      bind: [new Date().toISOString(), schemaVersion(wrap(db)), id],
    });
    current = { id, db };
    const opened: OpenedSave = { save: findSave(catalog, id), migrated, tables, migrations: applied };
    return opened;
  },

  'save.close': () => {
    closeCurrent();
    return null;
  },

  'save.delete': ({ id }) => {
    const { pool, catalog } = need();
    const save = findSave(catalog, id);
    if (current?.id === id) closeCurrent();
    // The row first: a file nobody points at is an orphan, not a save.
    catalog.exec({ sql: 'DELETE FROM saves WHERE id = ?', bind: [id] });
    removeFile(pool, save.activeFile);
    return null;
  },

  'meta.get': ({ key }) => {
    const value = needCurrent().selectValue('SELECT value FROM meta WHERE key = ?', [key]);
    return typeof value === 'string' || typeof value === 'number' ? value : null;
  },

  'meta.set': ({ key, value }) => {
    const db = needCurrent();
    db.transaction(() => setMeta(db, key, value));
    return null;
  },

  'test.writeWithoutCommit': ({ key, value }) => {
    testOnly();
    const db = needCurrent();
    db.exec('BEGIN IMMEDIATE');
    setMeta(db, key, value);
    return null;
  },

  'test.files': () => {
    testOnly();
    return need().pool.getFileNames().sort();
  },
};

function testOnly(): void {
  if (options.test === undefined) throw new OpError('invalid', 'operação só de teste');
}

function toError(err: unknown): DbError {
  if (err instanceof OpError) return { code: err.code, message: err.message };
  if (err instanceof NewerVersionError) return { code: 'newer-version', message: err.message };
  if (err instanceof NotOursError) return { code: 'not-a-save', message: err.message };
  if (err instanceof MigrationFailedError) return { code: 'migration-failed', message: err.message };
  return { code: 'internal', message: err instanceof Error ? err.message : String(err) };
}

let booted: Promise<void> | undefined;

async function serve<O extends DbOp>(request: DbRequest<O>): Promise<DbResponse<O>> {
  try {
    await booted;
    const handler = handlers[request.op] as (args: DbOps[O]['args']) => DbOps[O]['result'] | Promise<DbOps[O]['result']>;
    return { id: request.id, ok: true, result: await handler(request.args) };
  } catch (err: unknown) {
    return { id: request.id, ok: false, error: toError(err) };
  }
}

type Reply = (message: DbResponse | DbReady) => void;

function listen(reply: Reply): (e: MessageEvent<DbControl | DbRequest>) => void {
  return (e) => {
    const message = e.data;
    if ('type' in message) {
      if (message.type === 'init') {
        options = message.options;
        booted ??= boot().catch((err: unknown) => {
          info = { backend: 'none', sqlite: info.sqlite, detail: toError(err).message };
        });
        void booted.then(() => reply({ type: 'ready' }));
      } else {
        // Another worker's line to the database: served like the page's.
        const { port } = message;
        port.onmessage = listen((m) => port.postMessage(m));
      }
      return;
    }
    void serve(message).then(reply);
  };
}

addEventListener(
  'message',
  listen((m) => postMessage(m)),
);
