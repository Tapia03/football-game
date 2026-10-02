import { expect, test } from '@playwright/test';

test.describe('Phase 0 bootstrap', () => {
  test('loads WASM and shows libm pi', async ({ page }, testInfo) => {
    await page.goto('/');

    await expect(page.getByTestId('hello')).toHaveText(
      'Hello from Rust (libm: 3.141592653589793)',
    );

    // The release WASM bundle recomputes the libm golden table in this
    // browser and must match the table produced by native `cargo test`.
    await expect(page.getByTestId('libm-parity')).toHaveText('OK');

    // Evidence screenshot for the phase report (not a golden gate yet:
    // pixel goldens start in Phase 6 together with the renderer).
    const shot = await page.screenshot({
      fullPage: true,
      path: `evidence/phase-0/hello-${testInfo.project.name}.png`,
    });
    await testInfo.attach(`hello-${testInfo.project.name}`, {
      body: shot,
      contentType: 'image/png',
    });
  });

  test('page is cross-origin isolated (SharedArrayBuffer available)', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByTestId('coi')).toHaveText('true');
    const hasSab = await page.evaluate(() => typeof SharedArrayBuffer === 'function');
    expect(hasSab).toBe(true);
  });

  test('serves COOP/COEP headers', async ({ request }) => {
    const res = await request.get('/');
    expect(res.headers()['cross-origin-opener-policy']).toBe('same-origin');
    expect(res.headers()['cross-origin-embedder-policy']).toBe('require-corp');
  });
});
