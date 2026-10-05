// Database worker (spec Fase 7A): the only place of the page that opens the
// save files and speaks SQL. SQLite (official WASM build) over the
// `opfs-sahpool` VFS — synchronous access handles of the origin private file
// system, held by this worker alone — or, where there is no OPFS, in memory
// with each file stored whole in IndexedDB (`save/files.ts`). The page and
// the other workers ask for domain operations (`save/protocol.ts`), never
// for SQL.

import sqlite3InitModule, { type Database, type SqlValue } from '@sqlite.org/sqlite-wasm';
import { openIdb, openOpfs, type Files } from '../save/files';
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
  readonly files: Files;
  readonly catalog: Database;
};

let options: DbOptions = {};
let info: StorageInfo = { backend: 'none', sqlite: '', detail: 'not started' };
let storage: Storage | undefined;
/** The save that is open, if any. */
let current: { readonly id: string; readonly name: string; readonly db: Database } | undefined;

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

async function openFile(files: Files, name: string): Promise<Database> {
  const db = await files.open(name);
  db.exec('PRAGMA foreign_keys = ON');
  return db;
}

/**
 * A `.bak` left behind means a migration chain did not finish: the copy is
 * the save as it was, so it goes back in place.
 */
async function restoreBackups(files: Files): Promise<void> {
  for (const name of await files.list()) {
    if (!name.endsWith(BACKUP_SUFFIX)) continue;
    await files.copy(name, name.slice(0, -BACKUP_SUFFIX.length));
    await files.flush([], [name]);
  }
}

const reason = (err: unknown): string => (err instanceof Error ? err.message : String(err));

/** OPFS when the browser has it, IndexedDB otherwise (or when asked for). */
async function openFiles(
  sqlite3: Awaited<ReturnType<typeof sqlite3InitModule>>,
): Promise<{ files: Files; detail?: string }> {
  if (options.storage === 'idb') return { files: await openIdb(sqlite3), detail: 'IndexedDB pedido' };
  try {
    return { files: await openOpfs(sqlite3) };
  } catch (err: unknown) {
    return { files: await openIdb(sqlite3), detail: `sem OPFS: ${reason(err)}` };
  }
}

async function boot(): Promise<void> {
  const sqlite3 = await sqlite3InitModule();
  const sqlite = sqlite3.version.libVersion;
  info = { backend: 'none', sqlite, detail: 'not started' };
  const { files, detail } = await openFiles(sqlite3);
  await restoreBackups(files);
  const catalog = await openFile(files, CATALOG_FILE);
  if (migrate(wrap(catalog), CATALOG_MIGRATIONS).length > 0) {
    await files.flush([{ name: CATALOG_FILE, db: catalog }]);
  }
  storage = { files, catalog };
  info = detail === undefined ? { backend: files.backend, sqlite } : { backend: files.backend, sqlite, detail };
}

function need(): Storage {
  if (storage === undefined) {
    throw new OpError('no-storage', `sem armazenamento: ${info.detail ?? 'desconhecido'}`);
  }
  return storage;
}

function needCurrent(): { readonly name: string; readonly db: Database } {
  if (current === undefined) throw new OpError('no-save-open', 'nenhum save aberto');
  return current;
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
    const { files, catalog } = need();
    const trimmed = name.trim();
    if (trimmed === '') throw new OpError('invalid', 'o save precisa de um nome');
    await files.reserve(int(catalog.selectValue('SELECT count(*) FROM saves')) + 1);
    const id = crypto.randomUUID();
    const file = fileOf(id);
    const db = await openFile(files, file);
    try {
      migrate(wrap(db), saveMigrations());
      setMeta(db, 'name', trimmed);
      const now = new Date().toISOString();
      catalog.exec({
        sql: `INSERT INTO saves (id, name, created_at, last_opened_at, active_file, schema_version)
              VALUES (?, ?, ?, ?, ?, ?)`,
        bind: [id, trimmed, now, now, file, schemaVersion(wrap(db))],
      });
      // The file and the row that points at it, together.
      await files.flush([
        { name: file, db },
        { name: CATALOG_FILE, db: catalog },
      ]);
    } finally {
      db.close();
    }
    return findSave(catalog, id);
  },

  'save.open': async ({ id }) => {
    const { files, catalog } = need();
    const save = findSave(catalog, id);
    closeCurrent();
    const migrations = saveMigrations();
    const file = save.activeFile;
    const backup = `${file}${BACKUP_SUFFIX}`;
    let db = await openFile(files, file);
    let migrated: number[] = [];
    try {
      // Ours, and not from a newer game: checked before anything is touched.
      checkFile(wrap(db), migrations);
      if (schemaVersion(wrap(db)) < (migrations.at(-1)?.version ?? 0)) {
        // The save as it is, kept until the whole chain has worked.
        db.close();
        await files.copy(file, backup);
        db = await openFile(files, file);
        migrated = migrate(wrap(db), migrations);
        await files.flush([{ name: file, db }], [backup]);
      }
    } catch (err: unknown) {
      db.close();
      if ((await files.list()).includes(backup)) {
        // The chain stopped half way: back to the save as it was.
        await files.copy(backup, file);
        await files.flush([], [backup]);
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
    await files.flush([{ name: CATALOG_FILE, db: catalog }]);
    current = { id, name: file, db };
    const opened: OpenedSave = { save: findSave(catalog, id), migrated, tables, migrations: applied };
    return opened;
  },

  'save.close': () => {
    closeCurrent();
    return null;
  },

  'save.delete': async ({ id }) => {
    const { files, catalog } = need();
    const save = findSave(catalog, id);
    if (current?.id === id) closeCurrent();
    // The row first: a file nobody points at is an orphan, not a save.
    catalog.exec({ sql: 'DELETE FROM saves WHERE id = ?', bind: [id] });
    await files.flush([{ name: CATALOG_FILE, db: catalog }], [save.activeFile]);
    return null;
  },

  'meta.get': ({ key }) => {
    const value = needCurrent().db.selectValue('SELECT value FROM meta WHERE key = ?', [key]);
    return typeof value === 'string' || typeof value === 'number' ? value : null;
  },

  'meta.set': async ({ key, value }) => {
    const open = needCurrent();
    open.db.transaction(() => setMeta(open.db, key, value));
    await need().files.flush([open]);
    return null;
  },

  'test.writeWithoutCommit': ({ key, value }) => {
    testOnly();
    const { db } = needCurrent();
    db.exec('BEGIN IMMEDIATE');
    setMeta(db, key, value);
    return null;
  },

  'test.files': () => {
    testOnly();
    return need().files.list();
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
