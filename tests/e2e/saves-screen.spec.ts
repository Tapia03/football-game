import { readFile } from 'node:fs/promises';
import { expect, test, type Page } from '@playwright/test';

// Fase 7A: the minimal saves screen, over whatever storage the browser has
// (OPFS, or IndexedDB where there is none) and over the fallback forced.

/**
 * Replaces `navigator.storage.persist()` / `persisted()` with a browser
 * that grants (or denies) persistence and remembers it, counting the calls
 * across page loads. Does nothing where the browser has no StorageManager
 * (the WebKit of the test runner): the page must cope with that by itself.
 */
async function spyOnPersist(page: Page, grant: boolean): Promise<void> {
  await page.addInitScript((granted) => {
    const manager = navigator.storage as StorageManager | undefined;
    if (manager === undefined) return;
    manager.persisted = () => Promise.resolve(localStorage.getItem('test.persisted') === 'yes');
    manager.persist = () => {
      localStorage.setItem('test.persist.calls', String(Number(localStorage.getItem('test.persist.calls') ?? '0') + 1));
      if (granted) localStorage.setItem('test.persisted', 'yes');
      return Promise.resolve(granted);
    };
  }, grant);
}

const persistCalls = (page: Page): Promise<number> =>
  page.evaluate(() => Number(localStorage.getItem('test.persist.calls') ?? '0'));

/** Whether this browser can be asked for persistence at all. */
const canPersist = (page: Page): Promise<boolean> =>
  page.evaluate(() => (navigator.storage as StorageManager | undefined)?.persist !== undefined);

/** What the screen shows and how often it asked, for a browser that answers `granted`. */
async function expectPersistence(page: Page, granted: boolean): Promise<void> {
  if (await canPersist(page)) {
    await expect(page.getByTestId('saves-persistent')).toHaveText(granted ? 'sim' : 'não');
    expect(await persistCalls(page)).toBe(1);
  } else {
    // No StorageManager: nothing to ask, and the screen says it does not know.
    await expect(page.getByTestId('saves-persistent')).toHaveText('desconhecido');
    expect(await persistCalls(page)).toBe(0);
  }
}

async function openScreen(page: Page, url: string): Promise<void> {
  await page.goto(url);
  await expect(page.getByTestId('saves-storage')).toBeVisible();
}

async function createSave(page: Page, name: string): Promise<void> {
  await page.getByTestId('saves-new-name').fill(name);
  await page.getByTestId('saves-create').click();
  await expect(page.getByTestId('saves-message')).toHaveText(`Save "${name}" criado.`);
}

const row = (page: Page, name: string) =>
  page.getByTestId('saves-row').filter({ has: page.getByTestId('saves-name').getByText(name, { exact: true }) });

for (const url of ['/?view=saves', '/?view=saves&storage=idb']) {
  test.describe(`Fase 7A: saves screen (${url})`, () => {
    test('create, open, export, import, delete — and it is all there after a reload', async ({
      page,
    }, testInfo) => {
      await spyOnPersist(page, true);
      await openScreen(page, url);
      const backend = await page.getByTestId('saves-backend').innerText();
      console.log(`[7A screen ${testInfo.project.name}] ${url}: ${backend}`);
      if (url.includes('storage=idb')) expect(backend).toBe('IndexedDB');
      await expect(page.getByTestId('saves-empty')).toBeVisible();
      await expect(page.getByTestId('saves-create')).toBeDisabled();

      await createSave(page, 'Primeiro');
      await createSave(page, 'Segundo');
      await expect(page.getByTestId('saves-name')).toHaveText(['Segundo', 'Primeiro']);

      // Opening shows the schema and the digest of the content.
      await row(page, 'Primeiro').getByTestId('saves-open').click();
      await expect(page.getByTestId('saves-opened-name')).toHaveText('Primeiro');
      await expect(page.getByTestId('saves-opened')).toContainText('Schema v2');
      await expect(page.getByTestId('saves-opened')).toContainText('players');
      const digest = page.getByTestId('saves-digest');
      await expect(digest).toHaveText(/^[0-9a-f]{64}$/);
      const first = await digest.innerText();
      await row(page, 'Segundo').getByTestId('saves-open').click();
      await expect(page.getByTestId('saves-opened-name')).toHaveText('Segundo');
      // Two saves differ by their name, kept in the file.
      await expect(digest).not.toHaveText(first);

      // Export downloads the SQLite file of the save…
      const downloading = page.waitForEvent('download');
      await row(page, 'Primeiro').getByTestId('saves-export').click();
      const download = await downloading;
      expect(download.suggestedFilename()).toMatch(/^save-primeiro-\d{4}-\d{2}-\d{2}\.sqlite$/);
      const bytes = await readFile(await download.path());
      expect(bytes.subarray(0, 15).toString('latin1')).toBe('SQLite format 3');

      // …and importing it over the other save (the open one) replaces it.
      await row(page, 'Segundo')
        .getByTestId('saves-import')
        .setInputFiles({ name: 'exportado.sqlite', mimeType: 'application/vnd.sqlite3', buffer: bytes });
      await expect(page.getByTestId('saves-message')).toHaveText('Save "Segundo" substituído por exportado.sqlite.');
      await expect(digest).toHaveText(first);

      // A file that is not a save is refused, and says so.
      await row(page, 'Segundo')
        .getByTestId('saves-import')
        .setInputFiles({ name: 'notas.txt', mimeType: 'text/plain', buffer: Buffer.from('não é um save') });
      await expect(page.getByTestId('saves-message')).toContainText('Falhou (not-a-save)');
      await expect(digest).toHaveText(first);

      // After a reload the saves are there; nothing is open.
      await page.reload();
      await expect(page.getByTestId('saves-storage')).toBeVisible();
      await expect(page.getByTestId('saves-name')).toHaveText(['Segundo', 'Primeiro']);
      await expect(page.getByTestId('saves-opened')).toHaveCount(0);
      await row(page, 'Segundo').getByTestId('saves-open').click();
      await expect(digest).toHaveText(first);

      // Deleting takes two clicks.
      const remove = row(page, 'Primeiro').getByTestId('saves-delete');
      await remove.click();
      await expect(remove).toHaveText('Confirmar');
      await expect(page.getByTestId('saves-name')).toHaveCount(2);
      await remove.click();
      await expect(page.getByTestId('saves-message')).toHaveText('Save "Primeiro" apagado.');
      await expect(page.getByTestId('saves-name')).toHaveText(['Segundo']);

      // `persist()` was asked for once, on the first visit, not on the reload.
      await expectPersistence(page, true);
    });
  });
}

test.describe('Fase 7A: saves screen, one tab and persistence', () => {
  test('a second tab is told the game is already open; it works once the first is gone', async ({
    page,
    context,
  }) => {
    await openScreen(page, '/?view=saves');
    await createSave(page, 'Da primeira aba');

    const second = await context.newPage();
    await second.goto('/?view=saves');
    await expect(second.getByTestId('saves-other-tab')).toContainText('já está aberto em outra aba');
    await expect(second.getByTestId('saves-storage')).toHaveCount(0);
    // The first tab is not disturbed.
    await createSave(page, 'Ainda funciona');

    await page.close();
    await second.reload();
    await expect(second.getByTestId('saves-storage')).toBeVisible();
    await expect(second.getByTestId('saves-name')).toHaveText(['Ainda funciona', 'Da primeira aba']);
  });

  test('persist() denied: asked once all the same, and the screen says it is not persistent', async ({
    page,
  }) => {
    await spyOnPersist(page, false);
    await openScreen(page, '/?view=saves');
    await expectPersistence(page, false);
    await page.reload();
    await expect(page.getByTestId('saves-storage')).toBeVisible();
    await expectPersistence(page, false);
  });
});

test.describe('Fase 7A: which screen a URL opens', () => {
  test('?view=saves is the saves screen; a view nobody knows says so instead of showing the match', async ({
    page,
  }) => {
    await page.goto('/?view=save');
    await expect(page.getByTestId('unknown-view')).toContainText('Tela "save" não existe');
    await expect(page.locator('#match-canvas')).toHaveCount(0);
    await page.goto('/?view=saves');
    await expect(page.getByTestId('saves-storage')).toBeVisible();
    await expect(page.locator('#match-canvas')).toHaveCount(0);
    // And from the saves screen back to the match.
    await page.getByTestId('saves-to-match').click();
    await expect(page.locator('#match-canvas')).toBeVisible();
  });
});
