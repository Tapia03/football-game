import init, {
  spike_render,
  hello,
  libm_golden_mismatches,
  webgl2_smoke,
  webgl2_smoke_expected,
} from './pkg/fm_wasm.js';

/** Result of drawing one shader pixel through glow/WebGL2 in this browser. */
export type WebGl2Status =
  | { readonly ok: true; readonly pixel: readonly number[] }
  | { readonly ok: false; readonly detail: string };

export type EngineInfo = {
  readonly greeting: string;
  readonly libmMismatches: number;
  readonly crossOriginIsolated: boolean;
  readonly webgl2: WebGl2Status;
};

let ready: Promise<void> | undefined;

/** Instantiates the WASM module once; later calls reuse the same promise. */
function ensureInit(): Promise<void> {
  ready ??= init().then(() => undefined);
  return ready;
}

function probeWebGl2(): WebGl2Status {
  try {
    const pixel = Array.from(webgl2_smoke());
    const expected = Array.from(webgl2_smoke_expected());
    const same = pixel.length === expected.length && pixel.every((v, i) => v === expected[i]);
    return same
      ? { ok: true, pixel }
      : { ok: false, detail: `pixel ${pixel.join(',')} != esperado ${expected.join(',')}` };
  } catch (err: unknown) {
    return { ok: false, detail: err instanceof Error ? err.message : String(err) };
  }
}

export async function loadEngineInfo(): Promise<EngineInfo> {
  await ensureInit();
  return {
    greeting: hello(),
    libmMismatches: libm_golden_mismatches(),
    crossOriginIsolated: globalThis.crossOriginIsolated,
    webgl2: probeWebGl2(),
  };
}

/** SPIKE (branch `spike-render`): draws one static frame on `canvas`. Returns
 * the number of vertices drawn, or throws with the browser's error. */
export async function renderSpike(canvas: HTMLCanvasElement): Promise<number> {
  await ensureInit();
  const dpr = globalThis.devicePixelRatio || 1;
  canvas.width = Math.round(canvas.clientWidth * dpr);
  canvas.height = Math.round(canvas.clientHeight * dpr);
  return spike_render(canvas.id, canvas.width, canvas.height);
}
