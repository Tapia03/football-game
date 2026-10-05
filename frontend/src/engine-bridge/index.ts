import init, {
  hello,
  libm_golden_mismatches,
  webgl2_smoke,
  webgl2_smoke_expected,
  webgl2_smoke_matches,
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
    const drawn = webgl2_smoke();
    const pixel = Array.from(drawn);
    // To ±2 a channel: 0.5 reads back as 127 or 128 depending on the GPU.
    return webgl2_smoke_matches(drawn)
      ? { ok: true, pixel }
      : {
          ok: false,
          detail: `pixel ${pixel.join(',')} != esperado ${Array.from(webgl2_smoke_expected()).join(',')} (±2)`,
        };
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
  /** Role code of each of the 22 players (engine order), fixed for the match. */
  readonly roster: readonly number[];
  setSpeed(speed: number): void;
  pause(): void;
  resume(): void;
  /** Runs the match to logical tick `tick` exactly and pauses there. */
  runTo(tick: number): void;
  /**
   * Sets one team's tactics (6C), in force from the next logical tick:
   * `side` 0 home, 1 away; `mentality` 0 Defensive … 4 Attacking;
   * `pressing` 0 Low … 3 UltraHigh. The snapshots show what is in force.
   */
  setTactics(side: number, mentality: number, pressing: number): void;
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
  const handle: Omit<MatchHandle, 'roster'> = {
    reader: new SnapshotReader(buffer),
    setSpeed: (value) => send({ type: 'speed', speed: value }),
    pause: () => send({ type: 'pause' }),
    resume: () => send({ type: 'resume' }),
    runTo: (tick) => send({ type: 'runTo', tick }),
    setTactics: (side, mentality, pressing) => send({ type: 'tactics', side, mentality, pressing }),
    stop: () => worker.terminate(),
  };
  return new Promise((resolve, reject) => {
    worker.addEventListener('message', (e: MessageEvent<EngineEvent>) => {
      if (e.data.type === 'ready') {
        // The page's own module too: the camera functions run on this side.
        const { roster } = e.data;
        ensureInit().then(() => resolve({ ...handle, roster }), reject);
      } else reject(new Error(e.data.message));
    });
    worker.addEventListener('error', (e) => reject(new Error(e.message)));
    send({ type: 'start', seed, speed, buffer });
  });
}

export { SnapshotReader };
export type { Frame } from './sab';

// The canvas side (main thread): WebGL2 objects only. Every frame drawn is
// read from the snapshot ring and handed in — no match state lives here.

import {
  MatchCanvas,
  camera_step,
  camera_target,
  pitch_view,
  reference_slot,
  reference_slot_with,
} from './pkg/fm_wasm.js';

/**
 * Where the camera of `mode` (0 full pitch, 1 half pitch following the
 * ball, 2 tactical) wants to be, as `[centre x, centre y, zoom]`, given the
 * centre it wanted last frame. Pure (call after the canvas is open).
 */
export function cameraTarget(
  mode: number,
  ballX: number,
  ballY: number,
  previousX: number,
  previousY: number,
): Float32Array {
  return camera_target(mode, ballX, ballY, previousX, previousY);
}

/** The camera `dtMs` of real time later on its way from `current` to `target`. */
export function cameraStep(current: Float32Array, target: Float32Array, dtMs: number): Float32Array {
  return camera_step(current, target, dtMs);
}

/**
 * How the pitch is fitted into a `width` × `height` canvas, as
 * `[centre x, centre y, scale x, scale y]`: pitch point `(x, y)` is drawn at
 * clip `((x − cx) · sx, (y − cy) · sy)`. The mesh's own view (call after the
 * canvas is open).
 */
export function pitchView(camera: Float32Array, width: number, height: number): Float32Array {
  return pitch_view(camera, width, height);
}

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

/**
 * One ring slot as a fresh engine produces it for `seed` after `tick` ticks:
 * a pure reference the e2e tests compare the worker's snapshot against.
 */
export async function referenceSlot(
  seed: number,
  tick: number,
  commands?: readonly number[],
): Promise<Uint32Array> {
  await ensureInit();
  // `commands`: `[tick, side, mentality, pressing]` for each tactical
  // command the match received.
  return commands === undefined
    ? reference_slot(seed, tick)
    : reference_slot_with(seed, tick, Uint32Array.from(commands));
}
