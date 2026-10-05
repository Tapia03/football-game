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

// The script loaded: the page may hand over the port now.
postMessage({ type: 'loaded' } satisfies WorldLoaded);

addEventListener('message', (e: MessageEvent<MatchControl>) => {
  const { port } = e.data;
  init().then(
    (wasm) => {
      port.onmessage = (message: MessageEvent<MatchRequest>) => {
        let reply: MatchReply | undefined;
        try {
          reply = serve(message.data, wasm.memory);
        } catch (err: unknown) {
          // A copy that could not follow the world is no copy: forget it.
          host?.free();
          host = undefined;
          reply = { kind: 'failed', message: err instanceof Error ? err.message : String(err) };
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
