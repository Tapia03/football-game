<script lang="ts">
  import {
    cameraStep,
    cameraTarget,
    fitCanvas,
    loadEngineInfo,
    openCanvas,
    pitchView,
    referenceSlot,
    startMatch,
    type EngineInfo,
    type MatchCanvas,
    type MatchHandle,
  } from '../engine-bridge';
  import {
    SAMPLE_INTERVAL_MS,
    STATE_FINISHED,
    STATE_PAUSED,
    STATE_RUNNING,
    copyStats,
    mentalityOf,
    newFrame,
    pressingOf,
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
  const speeds = [1, 2, 5, 10, 30, 60] as const;
  let speed: number = $state(10);
  let clock = $state('00:00');
  let score = $state('0 × 0');
  // HUD (6B-1): what the snapshot carries besides positions. Plain DOM —
  // readable by tests and screen readers, no font atlas in WebGL.
  let half = $state('1º tempo');
  let cards = $state({ homeYellows: 0, homeReds: 0, awayYellows: 0, awayReds: 0 });
  /** Home share of the held-ball time so far, 0..100 (50 before any). */
  let possession = $state(50);
  // Statistics panel (6B-2): the team counters of the snapshot.
  const noStats = { shots: 0, onTarget: 0, xg: 0, passes: 0, passesCompleted: 0, tackles: 0, fouls: 0 };
  let stats = $state({ home: { ...noStats }, away: { ...noStats } });
  const passing = (s: typeof noStats): string =>
    s.passes === 0
      ? '0 / 0'
      : `${s.passesCompleted} / ${s.passes} (${Math.round((100 * s.passesCompleted) / s.passes)}%)`;
  const rows = $derived([
    { id: 'shots', label: 'Chutes (no alvo)', value: (s: typeof noStats) => `${s.shots} (${s.onTarget})` },
    { id: 'xg', label: 'xG', value: (s: typeof noStats) => s.xg.toFixed(2) },
    { id: 'passes', label: 'Passes certos / tentados', value: passing },
    { id: 'tackles', label: 'Botes', value: (s: typeof noStats) => String(s.tackles) },
    { id: 'fouls', label: 'Faltas', value: (s: typeof noStats) => String(s.fouls) },
  ].map((row) => ({ id: row.id, label: row.label, home: row.value(stats.home), away: row.value(stats.away) })));
  // Tactical panel (6C). It shows what the snapshot says is in force, not
  // what was clicked: a command that did not arrive never looks applied.
  const MENTALITIES = ['Defensiva', 'Cautelosa', 'Equilibrada', 'Positiva', 'Ofensiva'];
  const PRESSINGS = ['Baixa', 'Média', 'Alta', 'Muito alta'];
  /** Tactics words of the snapshot, `[home, away]` (Balanced + Medium). */
  let tactics = $state([0x0102, 0x0102]);
  /** Team the panel controls: 0 home, 1 away. */
  let tacticsSide = $state(0);
  const mentality = $derived(mentalityOf(tactics[tacticsSide] ?? 0));
  const pressing = $derived(pressingOf(tactics[tacticsSide] ?? 0));
  /** Sends the command and puts the control back on what is in force. */
  function command(e: Event, to: (value: number) => [number, number]): void {
    const select = e.currentTarget as HTMLSelectElement;
    const [m, p] = to(Number(select.value));
    match?.setTactics(tacticsSide, m, p);
    select.value = String(select.dataset['inForce']);
  }
  let status = $state('carregando…');
  let match: MatchHandle | undefined = $state();
  let canvasEl: HTMLCanvasElement | undefined = $state();
  // Toggles (6B-2): F1 labels every player (number + position; the project
  // has no player names yet), F2 draws velocity arrows. Keys and buttons.
  const ROLE_LABELS = ['GOL', 'ZAG', 'LAT', 'ALA', 'VOL', 'MC', 'ME', 'MEI', 'PTA', 'ATA'];
  const NO_VELOCITIES = new Float32Array(0);
  /** Role code of each player (engine order), for the formation lines. */
  let roster = new Uint8Array(0);
  let showLabels = $state(false);
  let showVectors = $state(false);
  // Overlays (6B-3): F3 the offside line, F4 the formation lines. Both are
  // drawn by the mesh; the bits are `OVERLAY_*` of the WASM side.
  const OVERLAY_OFFSIDE = 1;
  const OVERLAY_FORMATION = 2;
  let showOffside = $state(false);
  let showFormation = $state(false);
  // Camera (6D): presentation only, like the toggles. `camera` is where it
  // is now and `cameraGoal` where the mode wants it, both `[centre x,
  // centre y, zoom]`; every frame it blends towards the goal.
  const CAMERAS = [
    { id: 'full', label: 'Campo', title: 'Campo inteiro (1)' },
    { id: 'half', label: 'Bola', title: 'Meio campo, segue a bola (2)' },
    { id: 'tactical', label: 'Tática', title: 'Tática: rótulos e linhas de formação (3)' },
  ];
  const TACTICAL = 2;
  let cameraMode = $state(0);
  const camera = new Float32Array([52.5, 34, 1]);
  const cameraGoal = new Float32Array([52.5, 34, 1]);
  /** Test hook: the camera and how long the last switch took to settle. */
  const cam = { mode: 0, current: camera, goal: cameraGoal, settled: true, switchedAt: 0, settleMs: 0 };
  (globalThis as { fmCamera?: typeof cam }).fmCamera = cam;
  function setCamera(mode: number): void {
    if (mode === cameraMode) return;
    cameraMode = mode;
    cam.mode = mode;
    cam.settled = false;
    cam.switchedAt = performance.now();
  }
  // The tactical camera shows labels and formation lines while it is on,
  // without touching the F1/F4 toggles.
  const labelsOn = $derived(showLabels || cameraMode === TACTICAL);
  const formationOn = $derived(showFormation || cameraMode === TACTICAL);
  let labelTexts: string[] = $state([]);
  let labelEls: (HTMLSpanElement | undefined)[] = $state([]);

  $effect(() => {
    const onKey = (e: KeyboardEvent): void => {
      // Letters and digits belong to a focused control (a select types ahead).
      const typing = e.target instanceof HTMLSelectElement || e.target instanceof HTMLInputElement;
      if (!typing && !e.ctrlKey && !e.metaKey && !e.altKey) {
        if (e.key === 'c' || e.key === 'C') return setCamera((cameraMode + 1) % CAMERAS.length);
        if (e.key === '1' || e.key === '2' || e.key === '3') return setCamera(Number(e.key) - 1);
      }
      if (e.key === 'F1') showLabels = !showLabels;
      else if (e.key === 'F2') showVectors = !showVectors;
      else if (e.key === 'F3') showOffside = !showOffside;
      else if (e.key === 'F4') showFormation = !showFormation;
      else return;
      // F1 is the browser's help, F3 its search: keep them for the game.
      e.preventDefault();
    };
    addEventListener('keydown', onKey);
    return () => removeEventListener('keydown', onKey);
  });
  /** 'ok' once a frame was drawn; the error when WebGL2 is unavailable. */
  let render = $state('…');

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
    const latency = { frames: 0, sumMs: 0, maxMs: 0, over50: 0 };
    // Main-thread cost of a frame (6B-2): placing the labels and building
    // + drawing the mesh, to compare with the toggles on and off.
    const perf = { frames: 0, labelsMs: 0, drawMs: 0, verts: 0 };
    (globalThis as { fmPerf?: typeof perf }).fmPerf = perf;
    let view: Float32Array = new Float32Array(4);
    let viewFor = '';
    /** Real time of the last camera step (0: none yet, the camera cuts). */
    let cameraAt = 0;
    const moveCamera = (): void => {
      const now = performance.now();
      cameraGoal.set(cameraTarget(cameraMode, frame.ballX, frame.ballY, cameraGoal[0] ?? 0, cameraGoal[1] ?? 0));
      camera.set(cameraAt === 0 ? cameraGoal : cameraStep(camera, cameraGoal, now - cameraAt));
      cameraAt = now;
      const there = camera[0] === cameraGoal[0] && camera[1] === cameraGoal[1] && camera[2] === cameraGoal[2];
      if (there && !cam.settled) cam.settleMs = now - cam.switchedAt;
      cam.settled = there;
    };
    const placeLabels = (): void => {
      if (el === undefined) return;
      const key = `${el.width}x${el.height}:${camera[0]},${camera[1]},${camera[2]}`;
      if (key !== viewFor) {
        view = pitchView(camera, el.width, el.height);
        viewFor = key;
      }
      const [cx = 0, cy = 0, sx = 0, sy = 0] = view;
      const w = el.clientWidth;
      const h = el.clientHeight;
      // Under the player's disc (1.15 m), whatever the zoom.
      const below = 1.15 * Math.abs(sy) * 0.5 * h + 3;
      for (let i = 0; i < labelEls.length; i += 1) {
        const label = labelEls[i];
        if (label === undefined) continue;
        if ((frame.sentOff >> i) & 1) {
          label.style.display = 'none';
          continue;
        }
        // Clip space → CSS pixels of the canvas box (y grows downwards).
        const x = (((frame.xy[2 * i] ?? 0) - cx) * sx + 1) * 0.5 * w;
        const y = (1 - ((frame.xy[2 * i + 1] ?? 0) - cy) * sy) * 0.5 * h + below;
        label.style.display = '';
        label.style.transform = `translate(${x.toFixed(1)}px, ${y.toFixed(1)}px) translate(-50%, 0)`;
      }
    };
    (globalThis as { fmLatency?: typeof latency }).fmLatency = latency;
    // The canvas holds WebGL2 objects only; the match is not here. Without
    // WebGL2 the match still runs and the page says why nothing is drawn.
    const el = canvasEl;
    let canvas: MatchCanvas | undefined;
    const onResize = (): void => {
      if (el !== undefined) fitCanvas(el);
    };
    if (el !== undefined) {
      openCanvas(el).then(
        (c) => {
          canvas = c;
        },
        (err: unknown) => {
          render = `FALHOU: ${err instanceof Error ? err.message : String(err)}`;
        },
      );
      addEventListener('resize', onResize);
    }
    const tick = (): void => {
      if (handle === undefined || interpolator === undefined) return;
      const s = handle.reader.state();
      if (interpolator.at(interpolator.renderTimeMs(), frame)) {
        clock = formatClock(frame.tMs);
        score = `${frame.homeGoals} × ${frame.awayGoals}`;
        half = frame.half === 0 ? '1º tempo' : '2º tempo';
        cards.homeYellows = frame.homeYellows;
        cards.homeReds = frame.homeReds;
        cards.awayYellows = frame.awayYellows;
        cards.awayReds = frame.awayReds;
        const held = frame.homeHeld + frame.awayHeld;
        possession = held === 0 ? 50 : Math.round((100 * frame.homeHeld) / held);
        tactics[0] = frame.homeTactics;
        tactics[1] = frame.awayTactics;
        copyStats(frame.homeStats, stats.home);
        copyStats(frame.awayStats, stats.away);
        const c0 = performance.now();
        moveCamera();
        const cameraMs = performance.now() - c0;
        if (canvas !== undefined) {
          try {
            const t0 = performance.now();
            if (labelsOn) placeLabels();
            const t1 = performance.now();
            perf.verts = canvas.draw(
              frame.xy,
              frame.ballX,
              frame.ballY,
              frame.ballZ,
              frame.sentOff,
              frame.homePhase | (frame.awayPhase << 8) | (frame.half << 16),
              showVectors ? interpolator.velocity : NO_VELOCITIES,
              (showOffside ? OVERLAY_OFFSIDE : 0) | (formationOn ? OVERLAY_FORMATION : 0),
              roster,
              camera,
            );
            perf.frames += 1;
            perf.labelsMs += t1 - t0;
            perf.drawMs += performance.now() - t1 + cameraMs;
            render = 'ok';
          } catch (err: unknown) {
            render = `FALHOU: ${err instanceof Error ? err.message : String(err)}`;
            canvas = undefined;
          }
        }
        if (s === STATE_RUNNING) {
          const ms = SAMPLE_INTERVAL_MS / speed + handle.reader.stalenessMs();
          latency.frames += 1;
          latency.sumMs += ms;
          latency.maxMs = Math.max(latency.maxMs, ms);
          if (ms >= 50) latency.over50 += 1;
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
        // Test hook: lets the e2e tests pause the worker and read the ring.
        (globalThis as { fmMatch?: MatchHandle }).fmMatch = h;
        (globalThis as { fmReferenceSlot?: typeof referenceSlot }).fmReferenceSlot = referenceSlot;
        interpolator = new FrameInterpolator(h.reader);
        roster = Uint8Array.from(h.roster);
        labelTexts = h.roster.map((code, i) => `${(i % 11) + 1} ${ROLE_LABELS[code] ?? '?'}`);
        raf = requestAnimationFrame(tick);
      },
      (err: unknown) => {
        status = `FALHOU: ${err instanceof Error ? err.message : String(err)}`;
      },
    );
    return () => {
      stopped = true;
      cancelAnimationFrame(raf);
      removeEventListener('resize', onResize);
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
  <header class="hud" data-testid="hud">
    <div class="team home">
      <span class="name">Casa</span>
      <span class="card yellow" data-testid="hud-home-yellows" title="Cartões amarelos da casa"
        >{cards.homeYellows}</span
      >
      <span class="card red" data-testid="hud-home-reds" title="Cartões vermelhos da casa"
        >{cards.homeReds}</span
      >
    </div>
    <div class="centre">
      <span class="score" data-testid="match-score">{score}</span>
      <span class="time">
        <span class="clock" data-testid="match-clock">{clock}</span>
        <span data-testid="hud-half">{half}</span>
      </span>
    </div>
    <div class="team away">
      <span class="card yellow" data-testid="hud-away-yellows" title="Cartões amarelos do visitante"
        >{cards.awayYellows}</span
      >
      <span class="card red" data-testid="hud-away-reds" title="Cartões vermelhos do visitante"
        >{cards.awayReds}</span
      >
      <span class="name">Visitante</span>
    </div>
  </header>
  <div class="possession" data-testid="hud-possession" title="Posse de bola">
    <span data-testid="hud-possession-home">{possession}%</span>
    <span class="bar"><span class="home" style:width="{possession}%"></span></span>
    <span data-testid="hud-possession-away">{100 - possession}%</span>
  </div>
  <div class="stage">
    <div class="pitch">
      <canvas id="match-canvas" bind:this={canvasEl}></canvas>
      {#if labelsOn}
        <div class="labels" data-testid="labels">
          {#each labelTexts as text, i (i)}
            <span class="label" bind:this={labelEls[i]}>{text}</span>
          {/each}
        </div>
      {/if}
    </div>
    <aside class="stats" data-testid="stats">
      <h2>Estatísticas</h2>
      <table>
        <thead>
          <tr>
            <th class="home" scope="col">Casa</th>
            <td></td>
            <th class="away" scope="col">Visitante</th>
          </tr>
        </thead>
        <tbody>
          {#each rows as row (row.id)}
            <tr>
              <td class="home" data-testid="stats-{row.id}-home">{row.home}</td>
              <th scope="row">{row.label}</th>
              <td class="away" data-testid="stats-{row.id}-away">{row.away}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      <h2 class="tactics-title">Táticas</h2>
      <div class="tactics" data-testid="tactics">
        <div class="sides" role="group" aria-label="Time controlado">
          <button
            class="home"
            class:active={tacticsSide === 0}
            aria-pressed={tacticsSide === 0}
            data-testid="tactics-side-home"
            onclick={() => (tacticsSide = 0)}>Casa</button
          >
          <button
            class="away"
            class:active={tacticsSide === 1}
            aria-pressed={tacticsSide === 1}
            data-testid="tactics-side-away"
            onclick={() => (tacticsSide = 1)}>Visitante</button
          >
        </div>
        <label>
          <span>Mentalidade</span>
          <select
            data-testid="tactics-mentality"
            data-in-force={mentality}
            value={String(mentality)}
            onchange={(e) => command(e, (m) => [m, pressing])}
          >
            {#each MENTALITIES as name, i (i)}
              <option value={String(i)}>{name}</option>
            {/each}
          </select>
        </label>
        <label title="O motor ainda não suporta">
          <span>Tempo</span>
          <select data-testid="tactics-tempo" disabled title="O motor ainda não suporta">
            <option>Normal</option>
          </select>
        </label>
        <label>
          <span>Pressão</span>
          <select
            data-testid="tactics-pressing"
            data-in-force={pressing}
            value={String(pressing)}
            onchange={(e) => command(e, (p) => [mentality, p])}
          >
            {#each PRESSINGS as name, i (i)}
              <option value={String(i)}>{name}</option>
            {/each}
          </select>
        </label>
      </div>
    </aside>
  </div>
  <footer>
    <span>
      <span data-testid="match-status">{status}</span>
      · render <span data-testid="match-render">{render}</span>
    </span>
    <span class="speeds">
      {#each speeds as s (s)}
        <button class:active={speed === s} onclick={() => setSpeed(s)}>{s}×</button>
      {/each}
    </span>
    <span class="speeds">
      <button
        data-testid="toggle-labels"
        class:active={showLabels}
        aria-pressed={showLabels}
        title="Número e posição dos jogadores (F1)"
        onclick={() => (showLabels = !showLabels)}>F1 rótulos</button
      >
      <button
        data-testid="toggle-vectors"
        class:active={showVectors}
        aria-pressed={showVectors}
        title="Vetores de velocidade (F2)"
        onclick={() => (showVectors = !showVectors)}>F2 vetores</button
      >
      <button
        data-testid="toggle-offside"
        class:active={showOffside}
        aria-pressed={showOffside}
        title="Linha de impedimento (F3)"
        onclick={() => (showOffside = !showOffside)}>F3 impedimento</button
      >
      <button
        data-testid="toggle-formation"
        class:active={showFormation}
        aria-pressed={showFormation}
        title="Linhas de formação (F4)"
        onclick={() => (showFormation = !showFormation)}>F4 formação</button
      >
    </span>
    <span class="speeds" role="group" aria-label="Câmera">
      {#each CAMERAS as c, i (c.id)}
        <button
          data-testid="camera-{c.id}"
          class:active={cameraMode === i}
          aria-pressed={cameraMode === i}
          title={c.title}
          onclick={() => setCamera(i)}>{c.label}</button
        >
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
    /* The pitch plus its margin is 113 × 76 m: as wide as fits with the
       score bar and the controls still inside the viewport. */
    width: min(100% - 2rem, calc((100vh - 11rem) * 113 / 76 + 16rem));
    margin: 0 auto;
    padding: 1rem;
  }
  /* The pitch and, to its right, the statistics panel. */
  .stage {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 15rem;
    gap: 1rem;
    align-items: start;
    margin-top: 0.5rem;
  }
  .pitch {
    position: relative;
  }
  canvas {
    display: block;
    width: 100%;
    aspect-ratio: 113 / 76;
  }
  /* Player labels: DOM over the canvas, moved by transform every frame. */
  .labels {
    position: absolute;
    inset: 0;
    overflow: hidden;
    pointer-events: none;
  }
  .label {
    position: absolute;
    left: 0;
    top: 0;
    padding: 0 0.25rem;
    border-radius: 3px;
    background: rgb(0 0 0 / 55%);
    color: #fff;
    font-size: 0.625rem;
    font-weight: 700;
    line-height: 1.3;
    white-space: nowrap;
    will-change: transform;
  }
  .stats h2 {
    margin: 0 0 0.5rem;
    font-size: 0.875rem;
    font-weight: 700;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .stats table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.8125rem;
    font-variant-numeric: tabular-nums;
  }
  .stats thead th {
    padding-bottom: 0.25rem;
    font-weight: 700;
  }
  .stats tbody tr {
    display: grid;
    grid-template-columns: 1fr 1fr;
    padding: 0.375rem 0;
    border-top: 1px solid rgb(255 255 255 / 12%);
  }
  .stats thead tr {
    display: grid;
    grid-template-columns: 1fr 1fr;
  }
  .stats thead td {
    display: none;
  }
  .stats tbody th {
    grid-column: 1 / -1;
    grid-row: 1;
    font-weight: 400;
    color: var(--muted);
    text-align: center;
    font-size: 0.75rem;
  }
  .stats .home {
    color: #ff6b6b;
    text-align: left;
    font-weight: 700;
  }
  .stats .away {
    color: #4dabf7;
    text-align: right;
    font-weight: 700;
  }
  .stats .tactics-title {
    margin-top: 1.25rem;
  }
  .tactics {
    display: grid;
    gap: 0.5rem;
    font-size: 0.8125rem;
  }
  .tactics .sides {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.25rem;
  }
  .tactics .sides button {
    background: transparent;
    border: 1px solid currentcolor;
    border-radius: 4px;
    padding: 0.125rem 0.5rem;
    cursor: pointer;
    text-align: center;
    opacity: 0.55;
  }
  .tactics .sides button.active {
    opacity: 1;
    background: rgb(255 255 255 / 12%);
  }
  .tactics label {
    display: grid;
    grid-template-columns: 1fr 1.4fr;
    align-items: center;
    gap: 0.5rem;
    color: var(--muted);
  }
  .tactics select {
    font: inherit;
    color: inherit;
    background: var(--bg);
    border: 1px solid var(--muted);
    border-radius: 4px;
    padding: 0.125rem 0.25rem;
  }
  .tactics select:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  /* Narrow screens: the panel goes under the pitch. */
  @media (max-width: 56rem) {
    .match {
      width: min(100% - 2rem, calc((100vh - 11rem) * 113 / 76));
    }
    .stage {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .match footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
  }
  .hud {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 1rem;
  }
  .team {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .team.away {
    justify-content: flex-end;
  }
  .name {
    font-weight: 700;
  }
  .home .name {
    color: #ff6b6b;
  }
  .away .name {
    color: #4dabf7;
  }
  .card {
    min-width: 1.25rem;
    padding: 0 0.25rem;
    border-radius: 3px;
    text-align: center;
    font-size: 0.8125rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    color: #1b1b1b;
  }
  .card.yellow {
    background: #ffd43b;
  }
  .card.red {
    background: #fa5252;
    color: #fff;
  }
  .centre {
    display: flex;
    flex-direction: column;
    align-items: center;
    line-height: 1.2;
  }
  .score {
    font-size: 1.75rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .time {
    color: var(--muted);
    font-size: 0.875rem;
    font-variant-numeric: tabular-nums;
  }
  .clock {
    color: var(--fg);
    font-weight: 700;
  }
  .possession {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-top: 0.5rem;
    font-size: 0.75rem;
    font-variant-numeric: tabular-nums;
  }
  .bar {
    flex: 1;
    height: 0.375rem;
    border-radius: 3px;
    background: #4dabf7;
    overflow: hidden;
  }
  .bar .home {
    display: block;
    height: 100%;
    background: #ff6b6b;
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
