// DIAGNÓSTICO TEMPORÁRIO (fase-3): será revertido antes do PR.
import { test } from '@playwright/test';

test('DIAG: why WebGL2 context creation fails', async ({ page, browserName }, testInfo) => {
  await page.goto('/');
  const report = await page.evaluate(() => {
    const out: Record<string, string> = {};
    for (const kind of ['webgl2', 'webgl'] as const) {
      const canvas = document.createElement('canvas');
      let status = '(no webglcontextcreationerror event)';
      canvas.addEventListener('webglcontextcreationerror', (e) => {
        status = (e as WebGLContextEvent).statusMessage || '(empty statusMessage)';
      });
      const gl = canvas.getContext(kind);
      out[kind] = gl === null ? `null — ${status}` : 'ok';
    }
    return out;
  });
  console.log(`DIAG [${testInfo.project.name}/${browserName}] ${JSON.stringify(report)}`);
});
