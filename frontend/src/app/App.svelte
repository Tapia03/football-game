<script lang="ts">
  import { fitCanvas, loadEngineInfo, openMatch, type EngineInfo, type MatchView } from '../engine-bridge';

  type State =
    | { readonly kind: 'loading' }
    | { readonly kind: 'ready'; readonly info: EngineInfo }
    | { readonly kind: 'error'; readonly message: string };

  const SPEEDS = [1, 10, 30, 60] as const;
  const seed = Number(new URLSearchParams(location.search).get('seed') ?? '7') || 7;

  let engine: State = $state({ kind: 'loading' });
  let canvas: HTMLCanvasElement | undefined = $state();
  let speed: number = $state(10);
  let clock = $state('00:00');
  let score = $state('0 × 0');
  let status = $state('carregando…');

  function formatClock(ms: number): string {
    const s = Math.floor(ms / 1000);
    const mm = String(Math.floor(s / 60)).padStart(2, '0');
    const ss = String(s % 60).padStart(2, '0');
    return `${mm}:${ss}`;
  }

  $effect(() => {
    if (canvas === undefined) return;
    const el = canvas;
    let view: MatchView | undefined;
    let raf = 0;
    let last = performance.now();
    const onResize = () => fitCanvas(el);
    const frame = (now: number) => {
      if (view === undefined) return;
      // Cap the step so a hidden tab does not fast-forward on return.
      const dt = Math.min(now - last, 250);
      last = now;
      try {
        view.advance(dt * speed);
        view.render();
        clock = formatClock(view.clock_ms());
        score = `${view.home_goals()} × ${view.away_goals()}`;
        status = view.finished() ? 'fim de jogo' : 'ao vivo';
      } catch (err: unknown) {
        status = `FALHOU: ${err instanceof Error ? err.message : String(err)}`;
        return;
      }
      if (!view.finished()) raf = requestAnimationFrame(frame);
    };
    openMatch(el, seed).then(
      (v) => {
        view = v;
        status = 'ao vivo';
        last = performance.now();
        raf = requestAnimationFrame(frame);
      },
      (err: unknown) => {
        status = `FALHOU: ${err instanceof Error ? err.message : String(err)}`;
      },
    );
    addEventListener('resize', onResize);
    return () => {
      cancelAnimationFrame(raf);
      removeEventListener('resize', onResize);
      view?.free();
    };
  });

  $effect(() => {
    loadEngineInfo().then(
      (info) => {
        engine = { kind: 'ready', info };
      },
      (err: unknown) => {
        engine = { kind: 'error', message: err instanceof Error ? err.message : String(err) };
      },
    );
  });
</script>

<section class="match">
  <header class="scoreboard">
    <span class="team home">Casa</span>
    <span class="score" data-testid="match-score">{score}</span>
    <span class="team away">Visitante</span>
    <span class="clock" data-testid="match-clock">{clock}</span>
  </header>
  <canvas id="match-canvas" bind:this={canvas}></canvas>
  <footer>
    <span data-testid="match-status">{status}</span>
    <span class="speeds">
      {#each SPEEDS as s (s)}
        <button class:active={speed === s} onclick={() => (speed = s)}>{s}×</button>
      {/each}
    </span>
    <span class="seed">seed {seed}</span>
  </footer>
</section>

<main>
  {#if engine.kind === 'loading'}
    <p data-testid="status">Carregando engine…</p>
  {:else if engine.kind === 'error'}
    <p data-testid="status" class="err">Falha ao carregar WASM: {engine.message}</p>
  {:else}
    <h2 data-testid="hello">{engine.info.greeting}</h2>
    <dl>
      <dt>libm parity (golden nativo vs WASM)</dt>
      <dd data-testid="libm-parity" class={engine.info.libmMismatches === 0 ? 'ok' : 'err'}>
        {engine.info.libmMismatches === 0 ? 'OK' : `${engine.info.libmMismatches} divergências`}
      </dd>
      <dt>crossOriginIsolated (SharedArrayBuffer)</dt>
      <dd data-testid="coi" class={engine.info.crossOriginIsolated ? 'ok' : 'err'}>
        {engine.info.crossOriginIsolated ? 'true' : 'false'}
      </dd>
      <dt>WebGL2 (glow, shader)</dt>
      <dd data-testid="webgl2" class={engine.info.webgl2.ok ? 'ok' : 'err'}>
        {engine.info.webgl2.ok ? 'OK' : `FALHOU: ${engine.info.webgl2.detail}`}
      </dd>
    </dl>
  {/if}
</main>

<style>
  .match {
    max-width: 1280px;
    margin: 0 auto;
    padding: 1rem 1rem 0;
  }
  .scoreboard {
    display: grid;
    grid-template-columns: 1fr auto 1fr auto;
    align-items: center;
    gap: 1rem;
    padding: 0.5rem 1rem;
    font-weight: 700;
    font-size: 1.25rem;
  }
  .home {
    color: #e05555;
    text-align: right;
  }
  .away {
    color: #3a9be6;
  }
  .score {
    font-variant-numeric: tabular-nums;
    font-size: 1.5rem;
  }
  .clock {
    font-variant-numeric: tabular-nums;
    color: var(--muted);
  }
  canvas {
    display: block;
    /* Whole pitch visible without scrolling: as wide as fits both the
       column and the viewport height left by the score bar and footer. */
    width: min(100%, calc((100vh - 7rem) * 113 / 76));
    margin: 0 auto;
    aspect-ratio: 113 / 76;
    border-radius: 6px;
  }
  footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
    padding: 0.5rem 0;
    color: var(--muted);
    font-size: 0.9rem;
  }
  button {
    margin-left: 0.25rem;
    padding: 0.2rem 0.6rem;
    border-radius: 4px;
    border: 1px solid var(--muted);
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
  button.active {
    background: var(--muted);
    color: #000;
  }
  main {
    max-width: 48rem;
    margin: 0 auto;
    padding: 2rem 1rem;
    font-size: 0.9rem;
  }
  h2 {
    font-size: 1.1rem;
    margin: 0 0 1rem;
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
