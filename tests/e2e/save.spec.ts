import { expect, test, type Page } from '@playwright/test';

// Fase 7A: the database worker (SQLite WASM). Every test runs over both
// kinds of storage: `opfs` (the `opfs-sahpool` VFS) and `idb` (the
// IndexedDB fallback, forced). Each test has a fresh browser context, so a
// fresh origin private file system and a fresh IndexedDB.

type Backend = 'opfs' | 'idb';
type Options = {
  storage?: 'idb';
  test?: { migrations?: 'v2' | 'v2-then-broken'; stopImport?: 'before-swap' | 'after-swap' };
};
/** The storage the tests of the running `describe` ask for. */
let wanted: Backend = 'opfs';
type SaveInfo = {
  id: string;
  name: string;
  activeFile: string;
  schemaVersion: number;
  createdAt: string;
  lastOpenedAt: string;
};
type Client = { request(op: string, args: unknown, transfer?: Transferable[]): Promise<unknown> };
type Database = { client: Client; connect(): MessagePort; terminate(): void };
type Hooks = {
  fmSave: {
    startDatabase(options?: Options): Promise<Database>;
    DbClient: new (line: MessagePort) => Client;
  };
  /** The database the test is talking to. */
  db: Database;
};

/** Starts (or restarts, after killing it) the database worker of the page. */
async function start(page: Page, options: Options = { test: {} }): Promise<void> {
  await page.evaluate(
    async (o) => {
      const hooks = globalThis as unknown as Partial<Hooks> & Pick<Hooks, 'fmSave'>;
      hooks.db?.terminate();
      hooks.db = await hooks.fmSave.startDatabase(o);
    },
    wanted === 'idb' ? { ...options, storage: 'idb' as const } : options,
  );
}

/** One request; a failed one comes back as `{ error: code }`. */
async function ask<T>(page: Page, op: string, args: unknown = {}): Promise<T> {
  return page.evaluate(
    async ([o, a]) => {
      const { db } = globalThis as unknown as Hooks;
      try {
        return (await db.client.request(o as string, a)) as never;
      } catch (err: unknown) {
        return { error: (err as { code?: string }).code ?? String(err) } as never;
      }
    },
    [op, args] as const,
  );
}

type Storage = { backend: Backend | 'none'; sqlite: string; detail?: string };

/** Exports save `id`; the bytes travel to the test as plain numbers. */
async function exportSave(page: Page, id: string): Promise<{ fileName: string; bytes: number[] }> {
  return page.evaluate(async (save) => {
    const { db } = globalThis as unknown as Hooks;
    const out = (await db.client.request('save.export', { id: save })) as { fileName: string; bytes: ArrayBuffer };
    return { fileName: out.fileName, bytes: Array.from(new Uint8Array(out.bytes)) };
  }, id);
}

/**
 * Imports `bytes` over save `id` (transferring the buffer). With `wait`
 * false the answer is not awaited: the worker is about to be stopped.
 */
async function importSave(page: Page, id: string, bytes: number[], wait = true): Promise<unknown> {
  return page.evaluate(
    async ([save, content, awaited]) => {
      const { db } = globalThis as unknown as Hooks;
      const buffer = new Uint8Array(content as number[]).buffer;
      const asked = db.client
        .request('save.import', { id: save, bytes: buffer }, [buffer])
        .catch((err: unknown) => ({ error: (err as { code?: string }).code ?? String(err) }));
      if (awaited) return asked;
      // Long enough for the worker to reach the point it stops at.
      await new Promise((resolve) => setTimeout(resolve, 1_000));
      return 'not awaited';
    },
    [id, bytes, wait] as const,
  );
}

/** Opens save `id` and returns the digest of its content. */
async function digestOf(page: Page, id: string): Promise<string> {
  await ask(page, 'save.open', { id });
  return ask<string>(page, 'save.digest');
}

/**
 * Opens the quiet page and the database. Asked for OPFS, a browser without
 * it falls back to IndexedDB by itself: that is checked, logged, and the
 * test skipped (the `idb` run of the same test covers that browser).
 */
async function open(page: Page, project: string): Promise<Storage> {
  await page.goto('/?view=blank');
  await expect(page.getByTestId('blank')).toBeVisible();
  await start(page);
  const storage = await ask<Storage>(page, 'storage.info');
  console.log(
    `[7A storage ${project}] asked ${wanted}, got ${storage.backend}, sqlite ${storage.sqlite}${storage.detail === undefined ? '' : ` — ${storage.detail}`}`,
  );
  if (wanted === 'idb') {
    expect(storage.backend).toBe('idb');
  } else if (storage.backend !== 'opfs') {
    // No OPFS: the worker must have fallen back, saying why.
    expect(storage.backend).toBe('idb');
    expect(storage.detail).toMatch(/^sem OPFS: /);
  }
  test.skip(storage.backend !== wanted, `no OPFS in this browser: ${storage.detail ?? ''}`);
  return storage;
}

for (const backend of ['opfs', 'idb'] as const) {
test.describe(`Fase 7A: local persistence (SQLite WASM in the database worker, ${backend})`, () => {
  test.beforeEach(() => {
    wanted = backend;
  });

  test('saves: create, list, open, delete; the catalog points at one file a save', async ({
    page,
  }, testInfo) => {
    await open(page, testInfo.project.name);
    expect(await ask(page, 'save.list')).toEqual([]);
    const first = await ask<SaveInfo>(page, 'save.create', { name: '  Primeiro  ' });
    const second = await ask<SaveInfo>(page, 'save.create', { name: 'Segundo' });
    expect(first.name).toBe('Primeiro');
    expect(first.schemaVersion).toBe(1);
    expect(first.activeFile).toBe(`/save-${first.id}.sqlite`);
    expect(first.id).not.toBe(second.id);
    expect(await ask(page, 'save.create', { name: '   ' })).toEqual({ error: 'invalid' });

    const listed = await ask<SaveInfo[]>(page, 'save.list');
    expect(listed.map((s) => s.name).sort()).toEqual(['Primeiro', 'Segundo']);
    expect(await ask(page, 'test.files')).toEqual(
      ['/catalog.sqlite', first.activeFile, second.activeFile].sort(),
    );

    // The schema of a save: the six tables of the MVP (the catalog holds
    // `saves`) and the record of the migration that made them.
    const opened = await ask<{ save: SaveInfo; migrated: number[]; tables: string[]; migrations: string[] }>(
      page,
      'save.open',
      { id: first.id },
    );
    expect(opened.tables).toEqual(['clubs', 'competitions', 'matches', 'meta', 'migrations', 'players', 'tactics']);
    expect(opened.migrations).toEqual(['world']);
    expect(opened.migrated).toEqual([]);
    expect(await ask(page, 'meta.get', { key: 'name' })).toBe('Primeiro');
    // The save opened last comes first.
    expect((await ask<SaveInfo[]>(page, 'save.list')).map((s) => s.name)).toEqual(['Primeiro', 'Segundo']);

    // Deleting the open save closes it, removes its row and its file.
    expect(await ask(page, 'save.delete', { id: first.id })).toBeNull();
    expect(await ask(page, 'meta.get', { key: 'name' })).toEqual({ error: 'no-save-open' });
    expect((await ask<SaveInfo[]>(page, 'save.list')).map((s) => s.id)).toEqual([second.id]);
    expect(await ask(page, 'test.files')).toEqual(['/catalog.sqlite', second.activeFile].sort());
    expect(await ask(page, 'save.open', { id: first.id })).toEqual({ error: 'not-found' });
    expect(await ask(page, 'save.delete', { id: first.id })).toEqual({ error: 'not-found' });
  });

  test('data survives reloading the page', async ({ page }, testInfo) => {
    await open(page, testInfo.project.name);
    const save = await ask<SaveInfo>(page, 'save.create', { name: 'Persistente' });
    await ask(page, 'save.open', { id: save.id });
    expect(await ask(page, 'meta.set', { key: 'day', value: 12 })).toBeNull();
    expect(await ask(page, 'meta.set', { key: 'club', value: 'Atlético Sintético' })).toBeNull();
    expect(await ask(page, 'meta.get', { key: 'absent' })).toBeNull();

    await page.reload();
    await start(page);
    const listed = await ask<SaveInfo[]>(page, 'save.list');
    expect(listed.map((s) => [s.id, s.name])).toEqual([[save.id, 'Persistente']]);
    // Nothing is open after a restart.
    expect(await ask(page, 'meta.get', { key: 'day' })).toEqual({ error: 'no-save-open' });
    await ask(page, 'save.open', { id: save.id });
    expect(await ask(page, 'meta.get', { key: 'day' })).toBe(12);
    expect(await ask(page, 'meta.get', { key: 'club' })).toBe('Atlético Sintético');
  });

  test('a worker killed in the middle of a write: the last commit stands', async ({ page }, testInfo) => {
    await open(page, testInfo.project.name);
    const save = await ask<SaveInfo>(page, 'save.create', { name: 'Crash' });
    await ask(page, 'save.open', { id: save.id });
    await ask(page, 'meta.set', { key: 'day', value: 3 });
    // A write that never commits, then the worker dies.
    expect(await ask(page, 'test.writeWithoutCommit', { key: 'day', value: 4 })).toBeNull();
    expect(await ask(page, 'meta.get', { key: 'day' })).toBe(4);
    await start(page); // terminates the old worker first
    await ask(page, 'save.open', { id: save.id });
    expect(await ask(page, 'meta.get', { key: 'day' })).toBe(3);
    // The save goes on working, and the journal of the dead write is gone
    // with the next commit.
    await ask(page, 'meta.set', { key: 'day', value: 5 });
    expect(await ask(page, 'meta.get', { key: 'day' })).toBe(5);
    expect(await ask(page, 'test.files')).toEqual(['/catalog.sqlite', save.activeFile].sort());
  });

  test('schema migrations run in a chain on open, each in its own transaction', async ({ page }, testInfo) => {
    await open(page, testInfo.project.name);
    // A v1 file, written by the game as it is today.
    await start(page, {});
    const save = await ask<SaveInfo>(page, 'save.create', { name: 'Migrar' });
    await ask(page, 'save.open', { id: save.id });
    await ask(page, 'meta.set', { key: 'day', value: 7 });
    expect(await ask(page, 'test.files')).toEqual({ error: 'invalid' }); // not in test mode

    // A game one schema version ahead opens it: v1 → v2, data intact.
    await start(page, { test: { migrations: 'v2' } });
    const opened = await ask<{ save: SaveInfo; migrated: number[]; tables: string[]; migrations: string[] }>(
      page,
      'save.open',
      { id: save.id },
    );
    expect(opened.migrated).toEqual([2]);
    expect(opened.save.schemaVersion).toBe(2);
    expect(opened.tables).toContain('test_v2');
    expect(opened.migrations).toEqual(['world', 'test-v2']);
    expect(await ask(page, 'meta.get', { key: 'day' })).toBe(7);
    // The copy made before the chain is gone once the chain has worked.
    expect(await ask(page, 'test.files')).toEqual(['/catalog.sqlite', save.activeFile].sort());
    // Opening again migrates nothing.
    const again = await ask<{ migrated: number[] }>(page, 'save.open', { id: save.id });
    expect(again.migrated).toEqual([]);

    // A migration that fails half way: the file is back as it was (v2),
    // nothing of the failed one stays, and no copy is left.
    await start(page, { test: { migrations: 'v2-then-broken' } });
    expect(await ask(page, 'save.open', { id: save.id })).toEqual({ error: 'migration-failed' });
    expect(await ask(page, 'test.files')).toEqual(['/catalog.sqlite', save.activeFile].sort());
    await start(page, { test: { migrations: 'v2' } });
    const intact = await ask<{ save: SaveInfo; migrated: number[]; tables: string[]; migrations: string[] }>(
      page,
      'save.open',
      { id: save.id },
    );
    expect(intact.migrated).toEqual([]);
    expect(intact.tables).not.toContain('test_v3');
    expect(intact.migrations).toEqual(['world', 'test-v2']);
    expect(await ask(page, 'meta.get', { key: 'day' })).toBe(7);

    // The game of today meets a file from a newer game: refused, untouched.
    await start(page, { test: {} });
    expect(await ask(page, 'save.open', { id: save.id })).toEqual({ error: 'newer-version' });
    await start(page, { test: { migrations: 'v2' } });
    await ask(page, 'save.open', { id: save.id });
    expect(await ask(page, 'meta.get', { key: 'day' })).toBe(7);
  });

  test('another worker reaches the database through a port, not through the page', async ({ page }, testInfo) => {
    await open(page, testInfo.project.name);
    const save = await ask<SaveInfo>(page, 'save.create', { name: 'Porta' });
    const viaPort = await page.evaluate(async (id) => {
      const { db, fmSave } = globalThis as unknown as Hooks;
      // The port is what a world or engine worker would be handed.
      const client = new fmSave.DbClient(db.connect());
      await client.request('save.open', { id });
      await client.request('meta.set', { key: 'day', value: 21 });
      return client.request('save.list', {});
    }, save.id);
    expect((viaPort as SaveInfo[]).map((s) => s.name)).toEqual(['Porta']);
    // One database behind both lines.
    expect(await ask(page, 'meta.get', { key: 'day' })).toBe(21);
  });

  test('export then import: the save is replaced by the file, content for content', async ({
    page,
  }, testInfo) => {
    await open(page, testInfo.project.name);
    const a = await ask<SaveInfo>(page, 'save.create', { name: 'Série A' });
    const b = await ask<SaveInfo>(page, 'save.create', { name: 'Outro' });
    await ask(page, 'save.open', { id: a.id });
    await ask(page, 'meta.set', { key: 'day', value: 5 });
    const digestA = await ask<string>(page, 'save.digest');
    expect(digestA).toMatch(/^[0-9a-f]{64}$/);
    await ask(page, 'save.open', { id: b.id });
    await ask(page, 'meta.set', { key: 'day', value: 9 });
    expect(await ask<string>(page, 'save.digest')).not.toBe(digestA);

    // The export is a SQLite file, named after the save and the date; the
    // open save and the closed one export alike.
    const exported = await exportSave(page, a.id);
    expect(exported.fileName).toMatch(/^save-serie-a-\d{4}-\d{2}-\d{2}\.sqlite$/);
    expect(String.fromCharCode(...exported.bytes.slice(0, 15))).toBe('SQLite format 3');
    await ask(page, 'save.open', { id: a.id });
    expect((await exportSave(page, a.id)).bytes.length).toBe(exported.bytes.length);

    // Imported over B while B is the open save: B is now A's content, in a
    // new file; the old file is gone and A is untouched.
    await ask(page, 'save.open', { id: b.id });
    const replaced = (await importSave(page, b.id, exported.bytes)) as SaveInfo;
    expect(replaced.id).toBe(b.id);
    expect(replaced.name).toBe('Outro');
    expect(replaced.activeFile).not.toBe(b.activeFile);
    expect(await ask(page, 'meta.get', { key: 'day' })).toBe(5); // still open, on the new file
    expect(await ask<string>(page, 'save.digest')).toBe(digestA);
    expect(await ask(page, 'test.files')).toEqual(['/catalog.sqlite', a.activeFile, replaced.activeFile].sort());
    expect(await digestOf(page, a.id)).toBe(digestA);

    // And it is what is there after a restart.
    await start(page);
    expect(await digestOf(page, b.id)).toBe(digestA);
  });

  test('an invalid file is refused and the save stays exactly as it was', async ({ page }, testInfo) => {
    await open(page, testInfo.project.name);
    const save = await ask<SaveInfo>(page, 'save.create', { name: 'Intacto' });
    await ask(page, 'save.open', { id: save.id });
    await ask(page, 'meta.set', { key: 'day', value: 30 });
    const before = await ask<string>(page, 'save.digest');
    const good = (await exportSave(page, save.id)).bytes;
    const unchanged = async (): Promise<void> => {
      expect(await ask<SaveInfo[]>(page, 'save.list')).toMatchObject([{ id: save.id, activeFile: save.activeFile }]);
      expect(await ask(page, 'test.files')).toEqual(['/catalog.sqlite', save.activeFile].sort());
      expect(await digestOf(page, save.id)).toBe(before);
    };

    // Not a SQLite file at all.
    const text = Array.from(new TextEncoder().encode('isto não é um save'));
    expect(await importSave(page, save.id, text)).toEqual({ error: 'not-a-save' });
    await unchanged();
    // A SQLite header with nothing sound behind it.
    const hollow = [...good.slice(0, 16), ...new Array<number>(4096 - 16).fill(0xab)];
    expect(await importSave(page, save.id, hollow)).toEqual({ error: 'not-a-save' });
    await unchanged();
    // A real save cut in half.
    expect(await importSave(page, save.id, good.slice(0, good.length / 2))).toEqual({ error: 'not-a-save' });
    await unchanged();
    // A save that does not exist.
    expect(await importSave(page, 'no-such-save', good)).toEqual({ error: 'not-found' });
    await unchanged();

    // A file from a newer game: refused. An older one: migrated on import.
    await start(page, { test: { migrations: 'v2' } });
    await ask(page, 'save.open', { id: save.id }); // v1 → v2
    const newer = (await exportSave(page, save.id)).bytes;
    const v2 = await ask<string>(page, 'save.digest');
    await start(page, { test: { migrations: 'v2' } });
    const migratedOnImport = (await importSave(page, save.id, good)) as SaveInfo;
    expect(migratedOnImport.schemaVersion).toBe(2);
    expect(await digestOf(page, save.id)).toBe(v2);
    await ask(page, 'save.close');
    const other = await ask<SaveInfo>(page, 'save.create', { name: 'De hoje' });
    await start(page);
    // (Both saves are v2 now; the game of today only reads the catalog.)
    expect(await importSave(page, other.id, newer)).toEqual({ error: 'newer-version' });
    expect((await ask<SaveInfo[]>(page, 'save.list')).find((s) => s.id === other.id)?.activeFile).toBe(
      other.activeFile,
    );
  });

  test('a crash around the swap: the old save whole, or the new one whole; orphans go at boot', async ({
    page,
  }, testInfo) => {
    await open(page, testInfo.project.name);
    const a = await ask<SaveInfo>(page, 'save.create', { name: 'Fonte' });
    const b = await ask<SaveInfo>(page, 'save.create', { name: 'Alvo' });
    await ask(page, 'save.open', { id: a.id });
    await ask(page, 'meta.set', { key: 'day', value: 77 });
    const digestA = await ask<string>(page, 'save.digest');
    const bytes = (await exportSave(page, a.id)).bytes;
    const digestB = await digestOf(page, b.id);

    // Dies after the new file is written, before the catalog points at it:
    // the save is still the old file, and the new one is an orphan.
    await start(page, { test: { stopImport: 'before-swap' } });
    expect(await importSave(page, b.id, bytes, false)).toBe('not awaited');
    await start(page);
    expect((await ask<SaveInfo[]>(page, 'save.list')).find((s) => s.id === b.id)?.activeFile).toBe(b.activeFile);
    expect(await ask(page, 'test.files')).toEqual(['/catalog.sqlite', a.activeFile, b.activeFile].sort());
    expect(await digestOf(page, b.id)).toBe(digestB);

    // Dies after the catalog points at the new file, before the old one is
    // removed: the save is the new file, and the old one is the orphan.
    await start(page, { test: { stopImport: 'after-swap' } });
    expect(await importSave(page, b.id, bytes, false)).toBe('not awaited');
    await start(page);
    const swapped = (await ask<SaveInfo[]>(page, 'save.list')).find((s) => s.id === b.id);
    expect(swapped?.activeFile).not.toBe(b.activeFile);
    expect(await ask(page, 'test.files')).toEqual(
      ['/catalog.sqlite', a.activeFile, swapped?.activeFile ?? ''].sort(),
    );
    expect(await digestOf(page, b.id)).toBe(digestA);

    // Any other file nobody references is removed at boot too; the files of
    // the saves are not.
    await ask(page, 'test.plantFile', { name: '/save-ghost.sqlite' });
    await ask(page, 'test.plantFile', { name: '/import.tmp' });
    expect(await ask<string[]>(page, 'test.files')).toContain('/save-ghost.sqlite');
    await start(page);
    expect(await ask(page, 'test.files')).toEqual(
      ['/catalog.sqlite', a.activeFile, swapped?.activeFile ?? ''].sort(),
    );
    expect(await digestOf(page, a.id)).toBe(digestA);
  });
});
}
