<script lang="ts">
  import { loadEngineInfo, renderSpike, type EngineInfo } from '../engine-bridge';

  type State =
    | { readonly kind: 'loading' }
    | { readonly kind: 'ready'; readonly info: EngineInfo }
    | { readonly kind: 'error'; readonly message: string };

  let engine: State = $state({ kind: 'loading' });
  let spike: string = $state('…');
  let canvas: HTMLCanvasElement | undefined = $state();

  $effect(() => {
    if (canvas === undefined) return;
    renderSpike(canvas).then(
      (verts) => {
        spike = `OK (${verts} vértices)`;
      },
      (err: unknown) => {
        spike = `FALHOU: ${err instanceof Error ? err.message : String(err)}`;
      },
    );
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

<section class="spike">
  <canvas id="spike-canvas" bind:this={canvas}></canvas>
  <p data-testid="spike-status">Spike de render (glow/WebGL2): {spike}</p>
</section>

<main>
  {#if engine.kind === 'loading'}
    <p data-testid="status">Carregando engine…</p>
  {:else if engine.kind === 'error'}
    <p data-testid="status" class="err">Falha ao carregar WASM: {engine.message}</p>
  {:else}
    <h1 data-testid="hello">{engine.info.greeting}</h1>
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
  .spike {
    max-width: 1280px;
    margin: 0 auto;
    padding: 1rem 1rem 0;
  }
  .spike canvas {
    display: block;
    width: 100%;
    aspect-ratio: 113 / 76;
    border-radius: 6px;
  }
  .spike p {
    color: var(--muted);
    font-size: 0.9rem;
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
