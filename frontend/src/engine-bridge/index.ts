import init, { hello, libm_golden_mismatches } from './pkg/fm_wasm.js';

export type EngineInfo = {
  readonly greeting: string;
  readonly libmMismatches: number;
  readonly crossOriginIsolated: boolean;
};

let ready: Promise<void> | undefined;

/** Instantiates the WASM module once; later calls reuse the same promise. */
function ensureInit(): Promise<void> {
  ready ??= init().then(() => undefined);
  return ready;
}

export async function loadEngineInfo(): Promise<EngineInfo> {
  await ensureInit();
  return {
    greeting: hello(),
    libmMismatches: libm_golden_mismatches(),
    crossOriginIsolated: globalThis.crossOriginIsolated,
  };
}
