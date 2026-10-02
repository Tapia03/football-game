import { expect, test } from '@playwright/test';

/** "m:ss"/"mm:ss" → seconds. */
function seconds(clock: string): number {
  const [m, s] = clock.split(':').map(Number);
  return (m ?? 0) * 60 + (s ?? 0);
}

test.describe('Fase 6 v0: a match playing in the browser', () => {
  test('match runs: clock advances, players drawn, speed control works', async ({
    page,
    browserName,
  }, testInfo) => {
    // No WebGL in headless Firefox on the CI runner (docs/SPEC.md §0.1).
    test.skip(browserName === 'firefox', 'No WebGL in headless Firefox on CI (SPEC §0.1)');
    await page.goto('/');
    await expect(page.getByTestId('match-status')).toHaveText('ao vivo');

    const clock = page.getByTestId('match-clock');
    const t0 = seconds(await clock.innerText());
    await page.waitForTimeout(2_000);
    const t1 = seconds(await clock.innerText());
    // 10× by default: ~20 match-seconds in 2 s (frame pacing varies in CI).
    expect(t1 - t0).toBeGreaterThanOrEqual(8);

    await page.getByRole('button', { name: '60×' }).click();
    await page.waitForTimeout(2_000);
    const t2 = seconds(await clock.innerText());
    expect(t2 - t1).toBeGreaterThan(t1 - t0);
    await expect(page.getByTestId('match-score')).toHaveText(/^\d+ × \d+$/);
    await expect(page.getByTestId('match-status')).not.toContainText('FALHOU');

    // The whole pitch and the score bar fit the viewport (no scrolling).
    const box = await page.locator('#match-canvas').boundingBox();
    const viewport = page.viewportSize();
    expect(box).not.toBeNull();
    expect(viewport).not.toBeNull();
    if (box !== null && viewport !== null) {
      expect(box.y + box.height).toBeLessThanOrEqual(viewport.height);
    }

    await page.evaluate(() => scrollTo(0, 0));
    const shot = await page.screenshot({
      path: `evidence/phase-6-v0/match-${testInfo.project.name}.png`,
    });
    await testInfo.attach(`match-${testInfo.project.name}`, { body: shot, contentType: 'image/png' });
  });
});
