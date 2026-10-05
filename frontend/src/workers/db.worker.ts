// Database worker (spec Fase 7A): the only place of the page that opens the
// save files and speaks SQL. SQLite (official WASM build) over the
// `opfs-sahpool` VFS — synchronous access handles of the origin private file
// system, held by this worker alone — or, where there is no OPFS, in memory
// with each file stored whole in IndexedDB (`save/files.ts`). The page and
// the other workers ask for domain operations (`save/protocol.ts`), never
// for SQL.

import sqlite3InitModule, { type Database, type SqlValue } from '@sqlite.org/sqlite-wasm';
import { OpError } from '../save/errors';
import { openIdb, openOpfs, type Files } from '../save/files';
import * as worldDb from '../save/world-db';
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
  SAVE_SCHEMA,
  testMigrations,
  type Migration,
} from '../save/schema';

const CATALOG_FILE = '/catalog.sqlite';
/** Suffix of the copy of a save made before its migration chain runs. */
const BACKUP_SUFFIX = '.bak';

// (`OpError` lives in `../save/errors`.)

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
/** A fresh file name for new content of save `id` (an import). */
const nextFileOf = (id: string): string => `/save-${id}.${crypto.randomUUID().slice(0, 8)}.sqlite`;

/** First bytes of every SQLite file. */
const SQLITE_HEADER = 'SQLite format 3\u0000';

/** Tables a file must have to be a save of ours. */
const SAVE_TABLES = ['clubs', 'competitions', 'matches', 'meta', 'migrations', 'players', 'tactics'];

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
  const { next, broken } = testMigrations();
  switch (options.test?.migrations) {
    case 'next':
      return [...SAVE_SCHEMA, next];
    case 'next-then-broken':
      return [...SAVE_SCHEMA, next, broken];
    case undefined:
      return SAVE_SCHEMA;
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
  await removeOrphans(files, catalog);
  storage = { files, catalog };
  info = detail === undefined ? { backend: files.backend, sqlite } : { backend: files.backend, sqlite, detail };
}

/**
 * Removes every file no row of the catalog points at: what a crash leaves
 * between the commit of a pointer swap and the removal of the old file, a
 * file written aside for an import that never happened, a stray copy. The
 * journal of a file that is referenced stays: SQLite still needs it.
 */
async function removeOrphans(files: Files, catalog: Database): Promise<void> {
  const referenced = new Set([CATALOG_FILE, ...catalog.selectValues('SELECT active_file FROM saves').map(text)]);
  const orphans = (await files.list()).filter(
    (name) => !referenced.has(name) && !referenced.has(name.replace(/-journal$/, '')),
  );
  if (orphans.length > 0) await files.flush([], orphans);
}

/** Never answers: the test kills the worker here (`stopImport`). */
function stopHere(at: 'before-swap' | 'after-swap'): Promise<void> {
  return options.test?.stopImport === at ? new Promise(() => undefined) : Promise.resolve();
}

function hex(bytes: Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
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
    current = { id, name: file, db };
    // What the catalog shows about the world (club, season, day) follows
    // the file: if a crash left the two apart, this puts them together.
    await syncCatalog();
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

  'save.export': async ({ id }) => {
    const { files, catalog } = need();
    const save = findSave(catalog, id);
    // The open save is exported as it is now; a closed one from its file.
    const bytes = current?.id === id ? files.export(current.db) : await files.read(save.activeFile);
    const slug = save.name
      .normalize('NFD')
      .replace(/[^\w\s-]/g, '')
      .trim()
      .replace(/\s+/g, '-')
      .toLowerCase();
    const day = new Date().toISOString().slice(0, 10);
    // A buffer of its own (not a view of the WASM heap), to be transferred.
    const copy = new Uint8Array(bytes).buffer;
    return { fileName: `save-${slug === '' ? id : slug}-${day}.sqlite`, bytes: copy };
  },

  'save.import': async ({ id, bytes }) => {
    const { files, catalog } = need();
    const save = findSave(catalog, id);
    const content = new Uint8Array(bytes);
    const header = new TextDecoder().decode(content.subarray(0, SQLITE_HEADER.length));
    if (header !== SQLITE_HEADER) throw new OpError('not-a-save', 'o arquivo não é um banco SQLite');
    // Written aside: until the catalog points at it, it is nobody's file.
    const file = nextFileOf(id);
    await files.reserve(int(catalog.selectValue('SELECT count(*) FROM saves')) + 1);
    let db: Database | undefined;
    try {
      // Whatever SQLite itself refuses here is a file that is not a sound
      // database: the same answer as a failed integrity check.
      try {
        await files.write(file, content);
        db = await openFile(files, file);
        const sound = db.selectValue('PRAGMA integrity_check');
        if (sound !== 'ok') throw new Error(String(sound));
      } catch (err: unknown) {
        throw new OpError('not-a-save', `o arquivo está corrompido: ${reason(err)}`);
      }
      const migrations = saveMigrations();
      if (schemaVersion(wrap(db)) < 1) throw new OpError('not-a-save', 'o arquivo não é um save deste jogo');
      checkFile(wrap(db), migrations);
      const tables = db.selectValues("SELECT name FROM sqlite_schema WHERE type = 'table'").map(text);
      if (!SAVE_TABLES.every((t) => tables.includes(t))) {
        throw new OpError('not-a-save', 'o arquivo não tem as tabelas de um save');
      }
      // An older save is brought up to date before it becomes the save.
      migrate(wrap(db), migrations);
      const version = schemaVersion(wrap(db));
      await files.flush([{ name: file, db }]);
      db.close();
      db = undefined;
      await stopHere('before-swap');
      // The swap: one transaction of the catalog. Before it the save is the
      // old file, whole; after it, the new one, whole.
      const wasOpen = current?.id === id;
      if (wasOpen) closeCurrent();
      catalog.exec({
        sql: 'UPDATE saves SET active_file = ?, schema_version = ? WHERE id = ?',
        bind: [file, version, id],
      });
      await files.flush([{ name: CATALOG_FILE, db: catalog }]);
      await stopHere('after-swap');
      await files.flush([], [save.activeFile]);
      if (wasOpen) current = { id, name: file, db: await openFile(files, file) };
    } catch (err: unknown) {
      db?.close();
      // Not referenced by the catalog: remove what was written aside.
      if (text(catalog.selectValue('SELECT active_file FROM saves WHERE id = ?', [id])) !== file) {
        await files.flush([], [file]);
      }
      throw err;
    }
    return findSave(catalog, id);
  },

  'save.digest': async () => {
    const { db } = needCurrent();
    const tables = db
      .selectValues("SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
      .map(text);
    const dump = tables.map((table) => {
      // Table names come from the schema of our own file.
      const rows = db.selectArrays(`SELECT * FROM "${table}" ORDER BY 1`);
      const columns = db.selectValues(`SELECT name FROM pragma_table_info('${table}')`).map(text);
      const skip = table === 'migrations' ? columns.indexOf('applied_at') : -1;
      return [
        table,
        rows.map((row) => row.filter((_, i) => i !== skip).map((v) => (v instanceof Uint8Array ? `x${hex(v)}` : v))),
      ];
    });
    const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(JSON.stringify(dump)));
    return hex(new Uint8Array(digest));
  },

  'world.create': async ({ world }) => {
    const open = needCurrent();
    worldDb.createWorld(open.db, world);
    await need().files.flush([open]);
    await syncCatalog();
    return null;
  },

  'world.load': () => worldDb.loadWorld(needCurrent().db),

  'world.commitDay': async ({ commit }) => {
    const open = needCurrent();
    if (options.test?.stopCommitDay === true) {
      worldDb.commitDay(open.db, commit, true);
      return new Promise<never>(() => undefined);
    }
    worldDb.commitDay(open.db, commit);
    await need().files.flush([open]);
    // The file first, then the catalog: two files cannot share a
    // transaction, and the next `save.open` puts them together if needed.
    await syncCatalog();
    return null;
  },

  'world.standings': () => worldDb.standings(needCurrent().db),

  'world.round': ({ round }) => worldDb.round(needCurrent().db, round),

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

  'test.plantFile': async ({ name, bytes }) => {
    testOnly();
    // By default the size of one SQLite page, so that every backend takes
    // it as a file.
    const content = bytes === undefined ? new Uint8Array(4096) : new Uint8Array(bytes);
    if (bytes === undefined) content.set(new TextEncoder().encode(SQLITE_HEADER));
    const { files, catalog } = need();
    await files.reserve(int(catalog.selectValue('SELECT count(*) FROM saves')) + 1);
    await files.write(name, content);
    return null;
  },

  'test.sql': async ({ file, sql }) => {
    testOnly();
    const { files, catalog } = need();
    const plain = (rows: SqlValue[][]): (string | number | null)[][] =>
      rows.map((row) => row.map((v) => (typeof v === 'string' || typeof v === 'number' ? v : null)));
    if (file === CATALOG_FILE) {
      const rows = plain(catalog.selectArrays(sql));
      await files.flush([{ name: CATALOG_FILE, db: catalog }]);
      return rows;
    }
    // The file as it is: opened raw, never migrated.
    const db = await files.open(file);
    try {
      const rows = plain(db.selectArrays(sql));
      await files.flush([{ name: file, db }]);
      return rows;
    } finally {
      db.close();
    }
  },
};

/** Copies the world's club, season and day of the current save to the catalog. */
async function syncCatalog(): Promise<void> {
  if (current === undefined) return;
  const { files, catalog } = need();
  const world = worldDb.summary(current.db);
  if (world !== undefined) {
    catalog.exec({
      sql: 'UPDATE saves SET club_name = ?, season = ?, day = ? WHERE id = ?',
      bind: [world.clubName, world.season, world.day, current.id],
    });
  }
  await files.flush([{ name: CATALOG_FILE, db: catalog }]);
}

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
/** Requests are served one at a time, in the order they arrive. */
let queue: Promise<unknown> = Promise.resolve();

async function serve<O extends DbOp>(request: DbRequest<O>): Promise<DbResponse<O>> {
  try {
    await booted;
    const handler = handlers[request.op] as (args: DbOps[O]['args']) => DbOps[O]['result'] | Promise<DbOps[O]['result']>;
    return { id: request.id, ok: true, result: await handler(request.args) };
  } catch (err: unknown) {
    return { id: request.id, ok: false, error: toError(err) };
  }
}

type Reply = (message: DbResponse | DbReady, transfer?: Transferable[]) => void;

/** Buffers of a response (its top-level fields) that are handed over instead of copied. */
function transferOf(response: DbResponse): Transferable[] {
  if (!response.ok || response.result === null || typeof response.result !== 'object') return [];
  return Object.values(response.result).filter((v): v is ArrayBuffer => v instanceof ArrayBuffer);
}

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
        port.onmessage = listen((m, transfer = []) => port.postMessage(m, transfer));
      }
      return;
    }
    const served = queue.then(() => serve(message));
    queue = served;
    void served.then((response) => reply(response, transferOf(response)));
  };
}

addEventListener(
  'message',
  listen((m, transfer = []) => postMessage(m, { transfer })),
);
