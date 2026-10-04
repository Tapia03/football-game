import { expect, test, type Page } from '@playwright/test';

/** What the page exposes for these tests (see App.svelte). */
type Reader = {
  header(index: number): number;
  ready(): boolean;
  sequence(): number;
  rawSlot(n: number): Uint32Array | undefined;
};
type Hooks = {
  fmMatch: { reader: Reader; pause(): void };
  fmLatency: { frames: number; sumMs: number; maxMs: number };
  fmReferenceSlot(seed: number, tick: number): Promise<Uint32Array>;
};

/** "mm:ss" → seconds. */
function seconds(clock: string): number {
  const [m, s] = clock.split(':').map(Number);
  return (m ?? 0) * 60 + (s ?? 0);
}

async function openMatch(page: Page): Promise<void> {
  await page.goto('/?seed=7');
  await expect(page.getByTestId('match-status')).toHaveText('ao vivo');
}

test.describe('Fase 6 (6A): the match runs in the engine worker', () => {
  test('clock advances, speed control works, nothing fails', async ({
    page,
    browserName,
  }, testInfo) => {
    await openMatch(page);
    const clock = page.getByTestId('match-clock');
    const t0 = seconds(await clock.innerText());
    await page.waitForTimeout(2_000);
    const t1 = seconds(await clock.innerText());
    // 10× by default: ~20 match-seconds in 2 s (timer pacing varies in CI).
    expect(t1 - t0).toBeGreaterThanOrEqual(8);

    await page.getByRole('button', { name: '60×' }).click();
    await page.waitForTimeout(2_000);
    const t2 = seconds(await clock.innerText());
    expect(t2 - t1).toBeGreaterThan(t1 - t0);
    await expect(page.getByTestId('match-score')).toHaveText(/^\d+ × \d+$/);
    await expect(page.getByTestId('match-status')).not.toContainText('FALHOU');

    // Drawing needs WebGL2: headless Firefox on the CI runner has none
    // (docs/SPEC.md §0.1). The match itself must run there all the same.
    if (browserName !== 'firefox') {
      await expect(page.getByTestId('match-render')).toHaveText('ok');
      // The whole pitch and the controls fit the viewport (no scrolling).
      const box = await page.locator('#match-canvas').boundingBox();
      const viewport = page.viewportSize();
      expect(box).not.toBeNull();
      expect(viewport).not.toBeNull();
      if (box !== null && viewport !== null) {
        expect(box.y + box.height).toBeLessThanOrEqual(viewport.height);
      }
    }

    // Evidence for the phase report (pixel goldens start in 6B, Chromium only).
    const shot = await page.screenshot({
      path: `evidence/phase-6a/match-${testInfo.project.name}.png`,
    });
    await testInfo.attach(`match-${testInfo.project.name}`, { body: shot, contentType: 'image/png' });
  });

  test('the main thread has no engine: it reads what the worker publishes', async ({ page }) => {
    await openMatch(page);
    const header = await page.evaluate(() => {
      const { reader } = (globalThis as unknown as Hooks).fmMatch;
      return {
        magic: reader.header(0),
        slots: reader.header(2),
        slotBytes: reader.header(3),
        seed: reader.header(6),
        ready: reader.ready(),
        isolated: globalThis.crossOriginIsolated,
      };
    });
    expect(header).toEqual({
      magic: 0x464d_0001,
      slots: 16,
      slotBytes: 208,
      seed: 7,
      ready: true,
      isolated: true,
    });
  });

  test('determinism through the ring: a published tick equals a fresh engine', async ({ page }) => {
    await openMatch(page);
    await page.waitForTimeout(1_500);
    // Pause the worker, then take the newest snapshot that sits exactly on
    // a logical tick (the first of its six 60 Hz samples).
    const published = await page.evaluate(async () => {
      const match = (globalThis as unknown as Hooks).fmMatch;
      match.pause();
      await new Promise((resolve) => setTimeout(resolve, 200));
      const seq = match.reader.sequence();
      for (let n = seq - 1; n >= Math.max(0, seq - 15); n -= 1) {
        const words = match.reader.rawSlot(n);
        if (words !== undefined && (words[1] ?? 1) % 100 === 0) {
          return { tick: words[0] ?? 0, words: Array.from(words) };
        }
      }
      return undefined;
    });
    expect(published).toBeDefined();
    if (published === undefined) return;
    expect(published.tick).toBeGreaterThan(50);

    // The same tick from a fresh engine, run on the page (a pure function:
    // nothing of it stays on the main thread).
    const reference = await page.evaluate(
      async (tick) => Array.from(await (globalThis as unknown as Hooks).fmReferenceSlot(7, tick)),
      published.tick,
    );
    expect(reference).toEqual(published.words);
  });

  test('tick-to-draw latency stays under 50 ms at 1×', async ({ page }, testInfo) => {
    await openMatch(page);
    await page.getByRole('button', { name: '1×', exact: true }).click();
    await page.evaluate(() => {
      const stats = (globalThis as unknown as Hooks).fmLatency;
      stats.frames = 0;
      stats.sumMs = 0;
      stats.maxMs = 0;
    });
    await page.waitForTimeout(3_000);
    const stats = await page.evaluate(() => ({ ...(globalThis as unknown as Hooks).fmLatency }));
    expect(stats.frames).toBeGreaterThan(30);
    const mean = stats.sumMs / stats.frames;
    testInfo.annotations.push({
      type: 'latency',
      description: `mean ${mean.toFixed(1)} ms, max ${stats.maxMs.toFixed(1)} ms over ${stats.frames} frames`,
    });
    console.log(
      `[6A latency ${testInfo.project.name}] mean ${mean.toFixed(1)} ms, max ${stats.maxMs.toFixed(1)} ms, ${stats.frames} frames`,
    );
    expect(mean).toBeLessThan(50);
  });
});
