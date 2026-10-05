import { expect, test, type Page } from '@playwright/test';

// Fase 7A: the database worker (SQLite WASM over `opfs-sahpool`). Each test
// has a fresh browser context, so a fresh origin private file system.

type Options = { test?: { migrations?: 'v2' | 'v2-then-broken' } };
type SaveInfo = {
  id: string;
  name: string;
  activeFile: string;
  schemaVersion: number;
  createdAt: string;
  lastOpenedAt: string;
};
type Client = { request(op: string, args: unknown): Promise<unknown> };
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
  await page.evaluate(async (o) => {
    const hooks = globalThis as unknown as Partial<Hooks> & Pick<Hooks, 'fmSave'>;
    hooks.db?.terminate();
    hooks.db = await hooks.fmSave.startDatabase(o);
  }, options);
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

type Storage = { backend: 'opfs' | 'none'; sqlite: string; detail?: string };

/**
 * Opens the quiet page and the database. Where the browser has no usable
 * OPFS the test is skipped, and the log says so: the fallback (IndexedDB)
 * is the next commit's.
 */
async function open(page: Page, project: string): Promise<Storage> {
  await page.goto('/?view=saves');
  await expect(page.getByTestId('saves-placeholder')).toBeVisible();
  await start(page);
  const storage = await ask<Storage>(page, 'storage.info');
  console.log(
    `[7A storage ${project}] backend ${storage.backend}, sqlite ${storage.sqlite}${storage.detail === undefined ? '' : ` — ${storage.detail}`}`,
  );
  test.skip(storage.backend !== 'opfs', `no OPFS in this browser: ${storage.detail ?? ''}`);
  return storage;
}

test.describe('Fase 7A: local persistence (SQLite WASM in the database worker)', () => {
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
});
