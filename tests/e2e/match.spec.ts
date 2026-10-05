import { expect, test, type Page } from '@playwright/test';

/** What the page exposes for these tests (see App.svelte). */
type Reader = {
  header(index: number): number;
  ready(): boolean;
  sequence(): number;
  rawSlot(n: number): Uint32Array | undefined;
};
type Hooks = {
  fmMatch: { reader: Reader; pause(): void; runTo(tick: number): void };
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
      magic: 0x464d_0003,
      slots: 16,
      slotBytes: 288,
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

  test('HUD shows what the snapshot carries: half, cards, possession', async ({ page }) => {
    await page.goto('/?seed=3');
    await expect(page.getByTestId('match-status')).toHaveText('ao vivo');
    // Tick 30 000 = 50:00, second half; seed 3 has cards on both sides.
    await page.evaluate(() => (globalThis as unknown as Hooks).fmMatch.runTo(30_000));
    await expect(page.getByTestId('match-status')).toHaveText('pausado');
    await expect(page.getByTestId('match-clock')).toHaveText('49:59');
    await expect(page.getByTestId('hud-half')).toHaveText('2º tempo');
    await expect(page.getByTestId('match-score')).toHaveText(/^\d+ × \d+$/);

    // The newest snapshot, straight from the ring: cards are word 52 (one
    // byte each), held-ball ticks are words 53 and 54.
    const ring = await page.evaluate(() => {
      const { reader } = (globalThis as unknown as Hooks).fmMatch;
      const words = reader.rawSlot(reader.sequence() - 1);
      if (words === undefined) return undefined;
      const cards = words[52] ?? 0;
      return {
        cards: [cards & 0xff, (cards >> 8) & 0xff, (cards >> 16) & 0xff, (cards >>> 24) & 0xff],
        held: [words[53] ?? 0, words[54] ?? 0],
      };
    });
    expect(ring).toBeDefined();
    if (ring === undefined) return;
    const shown = [
      'hud-home-yellows',
      'hud-home-reds',
      'hud-away-yellows',
      'hud-away-reds',
    ].map((id) => page.getByTestId(id).innerText());
    expect((await Promise.all(shown)).map(Number)).toEqual(ring.cards);
    expect(ring.cards.reduce((a, b) => a + b, 0)).toBeGreaterThan(0);

    const [home, away] = ring.held;
    expect(home).toBeGreaterThan(0);
    expect(away).toBeGreaterThan(0);
    const share = Math.round((100 * (home ?? 0)) / ((home ?? 0) + (away ?? 0)));
    await expect(page.getByTestId('hud-possession-home')).toHaveText(`${share}%`);
    await expect(page.getByTestId('hud-possession-away')).toHaveText(`${100 - share}%`);
  });

  test('golden: the match at a fixed tick (pixels, Chromium only)', async ({ page, browserName }) => {
    // Pixel goldens only where rendering is deterministic: the software
    // renderer of headless Chromium (docs/SPEC.md, Fase 6). Firefox and
    // WebKit run every other test of this file without comparing pixels.
    test.skip(browserName !== 'chromium', 'Pixel goldens are Chromium-only (SPEC Fase 6)');
    // The reference is what the CI's Chromium renders (Linux fonts and
    // rasteriser); another machine differs by more than the tolerance.
    // FM_GOLDEN=1 runs it anyway.
    test.skip(
      process.env['CI'] === undefined && process.env['FM_GOLDEN'] === undefined,
      'Pixel goldens are rendered by the CI (set FM_GOLDEN=1 to compare locally)',
    );
    await openMatch(page);
    await page.evaluate(() => (globalThis as unknown as Hooks).fmMatch.runTo(6_000));
    await expect(page.getByTestId('match-status')).toHaveText('pausado');
    await expect(page.getByTestId('match-clock')).toHaveText('09:59');
    await expect(page.getByTestId('match-render')).toHaveText('ok');
    await expect(page).toHaveScreenshot('match-hud.png', { maxDiffPixelRatio: 0.01 });
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
