// Engine worker (spec Fase 6, 6A): the only place of the page where the
// match runs. It keeps the match clock (real time × speed), runs the
// logical ticks and publishes snapshots into the SharedArrayBuffer ring.
// The main thread never calls the engine; it sends control messages here
// and reads the ring.

import init, { EngineHost } from '../engine-bridge/pkg/fm_wasm.js';
import {
  H_SPEED,
  H_STATE,
  H_STEP_WALL,
  STATE_PAUSED,
  STATE_RUNNING,
  wallClockMs,
} from '../engine-bridge/sab';

export type EngineCommand =
  | { readonly type: 'start'; readonly seed: number; readonly speed: number; readonly buffer: SharedArrayBuffer }
  | { readonly type: 'speed'; readonly speed: number }
  | { readonly type: 'pause' }
  | { readonly type: 'resume' }
  /** Runs the match to logical tick `tick` exactly and pauses there. */
  | { readonly type: 'runTo'; readonly tick: number };

export type EngineEvent =
  /** `roster`: role code of each of the 22 players, fixed for the match. */
  | { readonly type: 'ready'; readonly roster: readonly number[] }
  | { readonly type: 'error'; readonly message: string };

/** How often the worker advances the match (ms of real time). */
const STEP_MS = 4;
/** Largest real-time step taken at once (a throttled worker does not jump). */
const MAX_STEP_MS = 250;

let host: EngineHost | undefined;
let header: Int32Array | undefined;
let speed = 1;
let running = false;
let last = 0;

function send(event: EngineEvent): void {
  postMessage(event);
}

function setSpeed(value: number): void {
  speed = value;
  if (header !== undefined) Atomics.store(header, H_SPEED, Math.round(value * 1000));
}

function setRunning(value: boolean): void {
  running = value;
  last = performance.now();
  if (header !== undefined && host !== undefined && !host.finished()) {
    Atomics.store(header, H_STATE, value ? STATE_RUNNING : STATE_PAUSED);
  }
}

function step(): void {
  if (host === undefined || !running) return;
  const now = performance.now();
  const dt = Math.min(now - last, MAX_STEP_MS);
  last = now;
  host.advance(dt * speed);
  if (header !== undefined) Atomics.store(header, H_STEP_WALL, wallClockMs());
  if (host.finished()) running = false;
}

async function start(seed: number, initialSpeed: number, buffer: SharedArrayBuffer): Promise<void> {
  await init();
  header = new Int32Array(buffer);
  host = new EngineHost(seed, buffer);
  setSpeed(initialSpeed);
  setRunning(true);
  setInterval(step, STEP_MS);
  send({ type: 'ready', roster: Array.from(host.roster()) });
}

addEventListener('message', (e: MessageEvent<EngineCommand>) => {
  const msg = e.data;
  switch (msg.type) {
    case 'start':
      start(msg.seed, msg.speed, msg.buffer).catch((err: unknown) => {
        send({ type: 'error', message: err instanceof Error ? err.message : String(err) });
      });
      break;
    case 'speed':
      setSpeed(msg.speed);
      break;
    case 'pause':
      setRunning(false);
      break;
    case 'resume':
      setRunning(true);
      break;
    case 'runTo':
      if (host !== undefined) {
        host.run_to(msg.tick);
        if (header !== undefined) Atomics.store(header, H_STEP_WALL, wallClockMs());
        setRunning(false);
      }
      break;
  }
});
