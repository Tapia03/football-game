import { expect, test, type Page } from '@playwright/test';

// Shared by the specs that drive the database worker directly (Fase 7A on).
// Fase 7A: the database worker (SQLite WASM). Every test runs over both
// kinds of storage: `opfs` (the `opfs-sahpool` VFS) and `idb` (the
// IndexedDB fallback, forced). Each test has a fresh browser context, so a
// fresh origin private file system and a fresh IndexedDB.

export type Backend = 'opfs' | 'idb';
export type Options = {
  storage?: 'idb';
  test?: {
    migrations?: 'next' | 'next-then-broken';
    stopImport?: 'before-swap' | 'after-swap';
    stopCommitDay?: boolean;
  };
};
/** The storage the tests of the running `describe` ask for. */
let wanted: Backend = 'opfs';

/** Chooses the storage the next tests ask for. */
export function setBackend(backend: Backend): void {
  wanted = backend;
}
export type SaveInfo = {
  id: string;
  name: string;
  activeFile: string;
  schemaVersion: number;
  createdAt: string;
  lastOpenedAt: string;
};
export type Client = { request(op: string, args: unknown, transfer?: Transferable[]): Promise<unknown> };
export type Database = { client: Client; connect(): MessagePort; terminate(): void };
export type Hooks = {
  fmSave: {
    startDatabase(options?: Options): Promise<Database>;
    DbClient: new (line: MessagePort) => Client;
  };
  /** The database the test is talking to. */
  db: Database;
};

/** Starts (or restarts, after killing it) the database worker of the page. */
export async function start(page: Page, options: Options = { test: {} }): Promise<void> {
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
export async function ask<T>(page: Page, op: string, args: unknown = {}): Promise<T> {
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

export type Storage = { backend: Backend | 'none'; sqlite: string; detail?: string };

/** Exports save `id`; the bytes travel to the test as plain numbers. */
export async function exportSave(page: Page, id: string): Promise<{ fileName: string; bytes: number[] }> {
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
export async function importSave(page: Page, id: string, bytes: number[], wait = true): Promise<unknown> {
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
export async function digestOf(page: Page, id: string): Promise<string> {
  await ask(page, 'save.open', { id });
  return ask<string>(page, 'save.digest');
}

/**
 * Opens the quiet page and the database. Asked for OPFS, a browser without
 * it falls back to IndexedDB by itself: that is checked, logged, and the
 * test skipped (the `idb` run of the same test covers that browser).
 */
export async function open(page: Page, project: string): Promise<Storage> {
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

