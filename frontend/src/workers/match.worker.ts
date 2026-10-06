// Match worker (spec Fase 7B, 7B.4b): one player of the pool. It holds a
// copy of the world and plays the matches the world worker deals it, in LOD
// Abstract. `play` only reads the world, so a copy in step with the world
// worker's gives exactly the result the world worker would get: which
// worker played a match never shows in the save.

import init, { WorldHost } from '../engine-bridge/pkg/fm_wasm.js';
import type { MatchControl, MatchReady, MatchReply, MatchRequest } from '../world/match-protocol';
import type { WorldLoaded } from '../world/protocol';

let host: WorldHost | undefined;

function serve(request: MatchRequest, memory: WebAssembly.Memory): MatchReply | undefined {
  switch (request.kind) {
    case 'load': {
      host?.free();
      host = WorldHost.load(
        BigInt(request.seed),
        request.day,
        request.userClub,
        request.clubRows,
        request.lineups,
        request.sheets,
        request.dynamics,
        request.fixtureRows,
        request.matchSeeds,
      );
      return { kind: 'loaded', wasmBytes: memory.buffer.byteLength };
    }
    case 'sync': {
      if (host === undefined) throw new Error('no world loaded');
      host.sync_day(request.day, request.dynamics, request.clubRows, request.lineups);
      return undefined;
    }
    case 'play': {
      if (host === undefined) throw new Error('no world loaded');
      const before = performance.now();
      const row = host.play(request.id);
      return { kind: 'played', id: request.id, row, ms: performance.now() - before };
    }
  }
}

/**
 * An error as it is worth reading far from here: its kind, its message and
 * the top of its stack (the frames say which call into the WASM it was in).
 */
function describe(err: unknown): string {
  if (!(err instanceof Error)) return String(err);
  const stack = (err.stack ?? '').split('\n').slice(0, 12).join(' < ').slice(0, 1500);
  return `${err.name}: ${err.message}${stack === '' ? '' : ` [${stack}]`}`;
}

// The script loaded: the page may hand over the port now.
postMessage({ type: 'loaded' } satisfies WorldLoaded);

addEventListener('message', (e: MessageEvent<MatchControl>) => {
  if (e.data.type === 'crash') throw new Error('falha pedida por um teste');
  const { port } = e.data;
  init().then(
    (wasm) => {
      // A request that could not be read is answered all the same: the
      // world worker must not wait for the answer to what never arrived.
      port.onmessageerror = () => {
        port.postMessage({ kind: 'failed', message: 'messageerror' } satisfies MatchReply);
      };
      port.onmessage = (message: MessageEvent<MatchRequest>) => {
        let reply: MatchReply | undefined;
        try {
          // Said before the match is played: it arrived.
          if (message.data.kind === 'play') {
            port.postMessage({ kind: 'started', id: message.data.id } satisfies MatchReply);
          }
          reply = serve(message.data, wasm.memory);
        } catch (err: unknown) {
          // What failed is said first, whole, and with the request it failed
          // in — before anything else here can fail and hide it.
          const request = message.data;
          const where = request.kind === 'play' ? `play ${request.id}` : request.kind;
          port.postMessage({ kind: 'failed', message: `${where}: ${describe(err)}` } satisfies MatchReply);
          // A copy that could not follow the world is no copy: forget it.
          // When the call that failed left the host borrowed (a trap inside
          // it), the host cannot be freed: freeing throws, and thrown from
          // here that error was all anyone ever saw (SPEC, 7B.4c). The copy
          // is then left to the worker's memory.
          try {
            host?.free();
          } catch {
            // Said already: the request failed, and why.
          }
          host = undefined;
          return;
        }
        if (reply !== undefined) port.postMessage(reply);
      };
      postMessage({ type: 'ready' } satisfies MatchReady);
    },
    (err: unknown) => {
      throw err;
    },
  );
});
