<script lang="ts">
  import { loadEngineInfo, type EngineInfo } from '../engine-bridge';

  type State =
    | { readonly kind: 'loading' }
    | { readonly kind: 'ready'; readonly info: EngineInfo }
    | { readonly kind: 'error'; readonly message: string };

  let state: State = $state({ kind: 'loading' });

  $effect(() => {
    loadEngineInfo().then(
      (info) => {
        state = { kind: 'ready', info };
      },
      (err: unknown) => {
        state = { kind: 'error', message: err instanceof Error ? err.message : String(err) };
      },
    );
  });
</script>

<main>
  {#if state.kind === 'loading'}
    <p data-testid="status">Carregando engine…</p>
  {:else if state.kind === 'error'}
    <p data-testid="status" class="err">Falha ao carregar WASM: {state.message}</p>
  {:else}
    <h1 data-testid="hello">{state.info.greeting}</h1>
    <dl>
      <dt>libm parity (golden nativo vs WASM)</dt>
      <dd data-testid="libm-parity" class={state.info.libmMismatches === 0 ? 'ok' : 'err'}>
        {state.info.libmMismatches === 0 ? 'OK' : `${state.info.libmMismatches} divergências`}
      </dd>
      <dt>crossOriginIsolated (SharedArrayBuffer)</dt>
      <dd data-testid="coi" class={state.info.crossOriginIsolated ? 'ok' : 'err'}>
        {state.info.crossOriginIsolated ? 'true' : 'false'}
      </dd>
    </dl>
  {/if}
</main>

<style>
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
