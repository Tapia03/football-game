import init, {
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

// ---------------------------------------------------------------------------
// Fase 6 (6A): the match runs in the engine worker and publishes snapshots
// into a SharedArrayBuffer; this side only sends commands and reads.

import type { EngineCommand, EngineEvent } from '../workers/engine.worker';
import { SnapshotReader, newSnapshotBuffer } from './sab';

export type MatchHandle = {
  readonly reader: SnapshotReader;
  setSpeed(speed: number): void;
  pause(): void;
  resume(): void;
  stop(): void;
};

/**
 * Starts demo match `seed` in a fresh engine worker at `speed` × real time.
 * Resolves once the worker has written the header and the first snapshot.
 */
export function startMatch(seed: number, speed: number): Promise<MatchHandle> {
  const buffer = newSnapshotBuffer();
  const worker = new Worker(new URL('../workers/engine.worker.ts', import.meta.url), {
    type: 'module',
  });
  const send = (command: EngineCommand): void => worker.postMessage(command);
  const handle: MatchHandle = {
    reader: new SnapshotReader(buffer),
    setSpeed: (value) => send({ type: 'speed', speed: value }),
    pause: () => send({ type: 'pause' }),
    resume: () => send({ type: 'resume' }),
    stop: () => worker.terminate(),
  };
  return new Promise((resolve, reject) => {
    worker.addEventListener('message', (e: MessageEvent<EngineEvent>) => {
      if (e.data.type === 'ready') resolve(handle);
      else reject(new Error(e.data.message));
    });
    worker.addEventListener('error', (e) => reject(new Error(e.message)));
    send({ type: 'start', seed, speed, buffer });
  });
}

export { SnapshotReader };
export type { Frame } from './sab';

// The canvas side (main thread): WebGL2 objects only. Every frame drawn is
// read from the snapshot ring and handed in — no match state lives here.

import { MatchCanvas } from './pkg/fm_wasm.js';

/** Sizes `canvas` to its CSS box × devicePixelRatio (sharp on HiDPI). */
export function fitCanvas(canvas: HTMLCanvasElement): void {
  const dpr = globalThis.devicePixelRatio || 1;
  canvas.width = Math.max(1, Math.round(canvas.clientWidth * dpr));
  canvas.height = Math.max(1, Math.round(canvas.clientHeight * dpr));
}

/** Binds the mesh renderer to `canvas` (throws when WebGL2 is unavailable). */
export async function openCanvas(canvas: HTMLCanvasElement): Promise<MatchCanvas> {
  await ensureInit();
  fitCanvas(canvas);
  return new MatchCanvas(canvas.id);
}

export type { MatchCanvas };
