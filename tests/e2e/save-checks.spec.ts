import { expect, test, type Page } from '@playwright/test';
import { ask, open, setBackend, start, type Hooks, type SaveInfo } from './support/db';

// Fase 7B (7B.5): the checks a file goes through when it is opened, and the
// context an unexpected error carries. They exist to tell apart, the next
// time a file turns up malformed, "IndexedDB gave back something else" (the
// sum does not match) from "what was stored was already bad, or went bad in
// memory" (the sum matches and SQLite refuses the file).
//
// The files of the IndexedDB fallback are planted from the page: the page
// and the worker share the origin's IndexedDB.

type Failed = { code: string; message: string };
type Stored = { bytes: Uint8Array; sha256?: string } | Uint8Array | undefined;
type Info = {
  backend: string;
  lastOpen?: { file: string; bytes: number; summed: boolean; sumMs: number; integrityMs: number };
};

/** One request; a failed one comes back whole: its code and what it says. */
async function attempt(page: Page, op: string, args: unknown = {}): Promise<Failed | { ok: unknown }> {
  return page.evaluate(
    async ([o, a]) => {
      const { db } = globalThis as unknown as Hooks;
      try {
        return { ok: await db.client.request(o as string, a) };
      } catch (err: unknown) {
        const e = err as { code?: string; message?: string };
        return { code: e.code ?? '', message: e.message ?? String(err) };
      }
    },
    [op, args] as const,
  );
}

/**
 * Rewrites what IndexedDB holds for `file`. `how`: `'wrong-sum'` keeps the
 * bytes and stores a sum that is not theirs; `'bad-page'` ruins the second
 * page of the database and stores the right sum of the ruined bytes;
 * `'no-sum'` stores the bytes alone, as files were stored before 7B.5.
 * Returns what was there before, as `{ bytes, sum }` lengths for the test.
 */
async function tamper(page: Page, file: string, how: 'wrong-sum' | 'bad-page' | 'no-sum') {
  return page.evaluate(
    async ([name, change]) => {
      const idb = await new Promise<IDBDatabase>((resolve, reject) => {
        const opening = indexedDB.open('fm-saves', 1);
        opening.onsuccess = () => resolve(opening.result);
        opening.onerror = () => reject(opening.error ?? new Error('IndexedDB did not open'));
      });
      const stored = await new Promise<Stored>((resolve, reject) => {
        const got = idb.transaction('files').objectStore('files').get(name as string);
        got.onsuccess = () => resolve(got.result as Stored);
        got.onerror = () => reject(got.error ?? new Error('IndexedDB read failed'));
      });
      if (stored === undefined || stored instanceof Uint8Array) throw new Error('not stored with a sum');
      const before = { bytes: stored.bytes.length, sum: stored.sha256 ?? '' };
      let value: Stored;
      if (change === 'wrong-sum') {
        value = { bytes: stored.bytes, sha256: '0'.repeat(64) };
      } else if (change === 'no-sum') {
        value = stored.bytes;
      } else {
        const bytes = new Uint8Array(stored.bytes);
        // The second page (pages are 4096 bytes): a table or an index of
        // the save, turned into noise.
        bytes.fill(0xab, 4096, 8192);
        const digest = await crypto.subtle.digest('SHA-256', bytes);
        const sha256 = Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, '0')).join('');
        value = { bytes, sha256 };
      }
      await new Promise<void>((resolve, reject) => {
        const tx = idb.transaction('files', 'readwrite');
        tx.objectStore('files').put(value, name as string);
        tx.oncomplete = () => resolve();
        tx.onerror = () => reject(tx.error ?? new Error('IndexedDB write failed'));
      });
      idb.close();
      return before;
    },
    [file, how] as const,
  );
}

test.describe('Fase 7B: a file of the IndexedDB fallback is checked when it is opened', () => {
  test.beforeEach(() => {
    setBackend('idb');
  });

  test('a file is stored with the SHA-256 of its bytes, and opens when both checks pass', async ({
    page,
  }, testInfo) => {
    await open(page, testInfo.project.name);
    const save = await ask<SaveInfo>(page, 'save.create', { name: 'Com soma' });
    await ask(page, 'save.open', { id: save.id });
    const info = await ask<Info>(page, 'storage.info');
    expect(info.lastOpen).toMatchObject({ file: save.activeFile, summed: true });
    expect(info.lastOpen?.bytes).toBeGreaterThan(4096);
    // What IndexedDB holds: the bytes and a sum of 64 hex digits.
    const before = await tamper(page, save.activeFile, 'no-sum');
    expect(before.bytes).toBe(info.lastOpen?.bytes);
    expect(before.sum).toMatch(/^[0-9a-f]{64}$/);
  });

  test('a sum that does not match: the file is refused, saying the sum did not match and which file', async ({
    page,
  }, testInfo) => {
    await open(page, testInfo.project.name);
    const save = await ask<SaveInfo>(page, 'save.create', { name: 'Soma errada' });
    await tamper(page, save.activeFile, 'wrong-sum');
    const failed = (await attempt(page, 'save.open', { id: save.id })) as Failed;
    expect(failed.code).toBe('corrupt');
    expect(failed.message).toContain(`save.open (id=${save.id})`);
    expect(failed.message).toContain(`arquivo ${save.activeFile}`);
    expect(failed.message).toContain('a soma de verificação não bate');
    // Nothing was opened, and nothing was thrown away: the file is there.
    expect(await ask(page, 'save.digest')).toEqual({ error: 'no-save-open' });
    expect(await ask<string[]>(page, 'test.files')).toContain(save.activeFile);
    // And the storage itself goes on working.
    const other = await ask<SaveInfo>(page, 'save.create', { name: 'Outro' });
    expect((await attempt(page, 'save.open', { id: other.id })) as { ok: unknown }).toHaveProperty('ok');
  });

  test('the right sum of a malformed database: refused by SQLite, saying the sum matched and which file', async ({
    page,
  }, testInfo) => {
    await open(page, testInfo.project.name);
    const save = await ask<SaveInfo>(page, 'save.create', { name: 'Página ruim' });
    await tamper(page, save.activeFile, 'bad-page');
    const failed = (await attempt(page, 'save.open', { id: save.id })) as Failed;
    expect(failed.code).toBe('corrupt');
    expect(failed.message).toContain(`save.open (id=${save.id})`);
    expect(failed.message).toContain(`arquivo ${save.activeFile}`);
    expect(failed.message).toContain('a soma de verificação confere');
    expect(failed.message).toContain('o banco está malformado');
    expect(await ask(page, 'save.digest')).toEqual({ error: 'no-save-open' });
  });

  test('a file stored before the sum existed opens all the same, and gets its sum at the next write', async ({
    page,
  }, testInfo) => {
    await open(page, testInfo.project.name);
    const save = await ask<SaveInfo>(page, 'save.create', { name: 'De antes' });
    await ask(page, 'save.open', { id: save.id });
    const digest = await ask<string>(page, 'save.digest');
    await ask(page, 'save.close');
    // As the 7A and the 7B up to 7B.4c stored it: the bytes alone.
    await tamper(page, save.activeFile, 'no-sum');
    await start(page);
    expect((await attempt(page, 'save.open', { id: save.id })) as { ok: unknown }).toHaveProperty('ok');
    expect((await ask<Info>(page, 'storage.info')).lastOpen).toMatchObject({ file: save.activeFile, summed: false });
    expect(await ask<string>(page, 'save.digest')).toBe(digest);
    // The first write stores it the new way.
    await ask(page, 'meta.set', { key: 'nota', value: 'gravado de novo' });
    await start(page);
    await ask(page, 'save.open', { id: save.id });
    expect((await ask<Info>(page, 'storage.info')).lastOpen).toMatchObject({ file: save.activeFile, summed: true });
  });

  test('a catalog that fails its checks: no storage, and the reason names the catalog', async ({
    page,
  }, testInfo) => {
    await open(page, testInfo.project.name);
    await ask<SaveInfo>(page, 'save.create', { name: 'Catálogo ruim' });
    await tamper(page, '/catalog.sqlite', 'wrong-sum');
    await start(page);
    const info = await ask<{ backend: string; detail?: string }>(page, 'storage.info');
    expect(info.backend).toBe('none');
    expect(info.detail).toContain('ao abrir o armazenamento');
    expect(info.detail).toContain('arquivo /catalog.sqlite');
    expect(info.detail).toContain('a soma de verificação não bate');
    expect(await ask(page, 'save.list')).toEqual({ error: 'no-storage' });
  });
});

for (const backend of ['opfs', 'idb'] as const) {
  test.describe(`Fase 7B: an unexpected error says where it happened (${backend})`, () => {
    test.beforeEach(() => {
      setBackend(backend);
    });

    test('an error of SQLite carries the operation, its arguments and the state of the open files', async ({
      page,
    }, testInfo) => {
      await open(page, testInfo.project.name);
      const save = await ask<SaveInfo>(page, 'save.create', { name: 'Com contexto' });
      await ask(page, 'save.open', { id: save.id });
      // Asked of the catalog, with a save open beside it.
      const failed = (await attempt(page, 'test.sql', {
        file: '/catalog.sqlite',
        sql: 'SELECT * FROM tabela_que_nao_existe',
      })) as Failed;
      expect(failed.code).toBe('internal');
      // The operation and what it was asked with…
      expect(failed.message).toContain('test.sql (file=/catalog.sqlite, sql=SELECT * FROM tabela_que_nao_existe)');
      // …what SQLite said…
      expect(failed.message).toContain('no such table: tabela_que_nao_existe');
      // …and that neither open file is malformed: it was the statement.
      expect(failed.message).toContain('[catálogo /catalog.sqlite: ok | save aberto ');
      expect(failed.message).toMatch(/save aberto \S+: ok\]$/);
      // The errors a caller tells apart say what they always said.
      expect(await attempt(page, 'save.open', { id: 'nenhum' })).toEqual({
        code: 'not-found',
        message: 'save nenhum não existe',
      });
    });
  });
}
