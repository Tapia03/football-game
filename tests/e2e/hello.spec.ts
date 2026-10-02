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

  test('WebGL2 draws through glow (renderer smoke test)', async ({ page, browserName }) => {
    // Known CI gap (docs/SPEC.md §0.1): headless Firefox on the GPU-less Linux
    // runner cannot create any GL context, even with webgl.force-enabled and
    // Mesa llvmpipe/EGL installed. Real Firefox with a GPU is unaffected.
    test.skip(browserName === 'firefox', 'No WebGL in headless Firefox on CI (SPEC §0.1)');
    await page.goto('/');
    // On failure the cell shows the browser's error, so the CI log says why.
    await expect(page.getByTestId('webgl2')).toHaveText('OK');
    const renderer = await page.evaluate(() => {
      const gl = document.createElement('canvas').getContext('webgl2');
      if (gl === null) return 'none';
      // Unmasked name shows whether CI runs on SwiftShader/llvmpipe (software).
      const dbg = gl.getExtension('WEBGL_debug_renderer_info');
      return String(gl.getParameter(dbg === null ? gl.RENDERER : dbg.UNMASKED_RENDERER_WEBGL));
    });
    console.log(`WebGL2 renderer: ${renderer}`);
  });

  test('serves COOP/COEP headers', async ({ request }) => {
    const res = await request.get('/');
    expect(res.headers()['cross-origin-opener-policy']).toBe('same-origin');
    expect(res.headers()['cross-origin-embedder-policy']).toBe('require-corp');
  });
});
