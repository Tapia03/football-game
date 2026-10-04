<script lang="ts">
  import { loadEngineInfo, startMatch, type EngineInfo, type MatchHandle } from '../engine-bridge';
  import {
    SAMPLE_INTERVAL_MS,
    STATE_FINISHED,
    STATE_PAUSED,
    STATE_RUNNING,
    newFrame,
  } from '../engine-bridge/sab';
  import { FrameInterpolator } from '../render/interpolate';

  type State =
    | { readonly kind: 'loading' }
    | { readonly kind: 'ready'; readonly info: EngineInfo }
    | { readonly kind: 'error'; readonly message: string };

  let boot: State = $state({ kind: 'loading' });

  // Fase 6 (6A): the match runs in the engine worker; the page only reads
  // the snapshot ring.
  const seed = Number(new URLSearchParams(location.search).get('seed') ?? '7') || 7;
  const speeds = [1, 10, 30, 60] as const;
  let speed: number = $state(10);
  let clock = $state('00:00');
  let score = $state('0 × 0');
  let status = $state('carregando…');
  let match: MatchHandle | undefined = $state();

  function formatClock(ms: number): string {
    const s = Math.floor(ms / 1000);
    const pad = (v: number): string => String(v).padStart(2, '0');
    return `${pad(Math.floor(s / 60))}:${pad(s % 60)}`;
  }

  $effect(() => {
    let raf = 0;
    let handle: MatchHandle | undefined;
    let stopped = false;
    const frame = newFrame();
    let interpolator: FrameInterpolator | undefined;
    // Tick-to-draw latency (spec Fase 6, 6A): the frame drawn is one sample
    // interval behind the worker's clock, plus however stale that clock is.
    const latency = { frames: 0, sumMs: 0, maxMs: 0 };
    (globalThis as { fmLatency?: typeof latency }).fmLatency = latency;
    const tick = (): void => {
      if (handle === undefined || interpolator === undefined) return;
      const s = handle.reader.state();
      if (interpolator.at(interpolator.renderTimeMs(), frame)) {
        clock = formatClock(frame.tMs);
        score = `${frame.homeGoals} × ${frame.awayGoals}`;
        if (s === STATE_RUNNING) {
          const ms = SAMPLE_INTERVAL_MS / speed + handle.reader.stalenessMs();
          latency.frames += 1;
          latency.sumMs += ms;
          latency.maxMs = Math.max(latency.maxMs, ms);
        }
      }
      status = s === STATE_FINISHED ? 'fim de jogo' : s === STATE_PAUSED ? 'pausado' : 'ao vivo';
      raf = requestAnimationFrame(tick);
    };
    startMatch(seed, 10).then(
      (h) => {
        if (stopped) {
          h.stop();
          return;
        }
        handle = h;
        match = h;
        interpolator = new FrameInterpolator(h.reader);
        raf = requestAnimationFrame(tick);
      },
      (err: unknown) => {
        status = `FALHOU: ${err instanceof Error ? err.message : String(err)}`;
      },
    );
    return () => {
      stopped = true;
      cancelAnimationFrame(raf);
      handle?.stop();
    };
  });

  function setSpeed(value: number): void {
    speed = value;
    match?.setSpeed(value);
  }

  $effect(() => {
    loadEngineInfo().then(
      (info) => {
        boot = { kind: 'ready', info };
      },
      (err: unknown) => {
        boot = { kind: 'error', message: err instanceof Error ? err.message : String(err) };
      },
    );
  });
</script>

<section class="match" data-testid="match">
  <header>
    <span class="score" data-testid="match-score">{score}</span>
    <span class="clock" data-testid="match-clock">{clock}</span>
  </header>
  <footer>
    <span data-testid="match-status">{status}</span>
    <span class="speeds">
      {#each speeds as s (s)}
        <button class:active={speed === s} onclick={() => setSpeed(s)}>{s}×</button>
      {/each}
    </span>
    <span class="seed">seed {seed}</span>
  </footer>
</section>

<main>
  {#if boot.kind === 'loading'}
    <p data-testid="status">Carregando engine…</p>
  {:else if boot.kind === 'error'}
    <p data-testid="status" class="err">Falha ao carregar WASM: {boot.message}</p>
  {:else}
    <h1 data-testid="hello">{boot.info.greeting}</h1>
    <dl>
      <dt>libm parity (golden nativo vs WASM)</dt>
      <dd data-testid="libm-parity" class={boot.info.libmMismatches === 0 ? 'ok' : 'err'}>
        {boot.info.libmMismatches === 0 ? 'OK' : `${boot.info.libmMismatches} divergências`}
      </dd>
      <dt>crossOriginIsolated (SharedArrayBuffer)</dt>
      <dd data-testid="coi" class={boot.info.crossOriginIsolated ? 'ok' : 'err'}>
        {boot.info.crossOriginIsolated ? 'true' : 'false'}
      </dd>
      <dt>WebGL2 (glow, shader)</dt>
      <dd data-testid="webgl2" class={boot.info.webgl2.ok ? 'ok' : 'err'}>
        {boot.info.webgl2.ok ? 'OK' : `FALHOU: ${boot.info.webgl2.detail}`}
      </dd>
    </dl>
  {/if}
</main>

<style>
  .match {
    max-width: 48rem;
    margin: 0 auto;
    padding: 1rem;
  }
  .match header,
  .match footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
  }
  .score,
  .clock {
    font-size: 1.5rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .match footer {
    color: var(--muted);
    font-size: 0.875rem;
    margin-top: 0.5rem;
  }
  .speeds button {
    background: transparent;
    color: inherit;
    border: 1px solid var(--muted);
    border-radius: 4px;
    padding: 0.125rem 0.5rem;
    margin: 0 0.125rem;
    cursor: pointer;
  }
  .speeds button.active {
    background: var(--muted);
    color: var(--bg);
  }
  main {
    max-width: 48rem;
    margin: 0 auto;
    padding: 3rem 1rem;
  }
  h1 {
    font-size: 1.75rem;
    margin: 0 0 1.5rem;
  }
  dl {
    display: grid;
    grid-template-columns: auto auto;
    gap: 0.5rem 1.5rem;
    justify-content: start;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    font-weight: 600;
  }
  .ok {
    color: var(--ok);
  }
  .err {
    color: var(--err);
  }
</style>
