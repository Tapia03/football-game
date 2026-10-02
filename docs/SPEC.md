# SPEC — Simulador de Futebol no Navegador

**Revisão 2.1** (fonte da verdade versionada).

- **v2** corrigiu duas contradições da v1: tick lógico único (1A) e RNG
  indexado por evento (1B). Essas correções estão marcadas com `[CORRIGIDO v2]`.
- **v2.1** troca wgpu por WebGL2 via `glow` atrás da trait `Renderer2D`.
  Também isola todo `unsafe` em `fm-render/src/ffi/`, adota branches `fase-N`
  e registra os desvios já aprovados. Essas mudanças estão marcadas com
  `[ALTERADO v2.1]`.

**Regra permanente:** toda mudança de arquitetura atualiza este arquivo no
mesmo PR que o código, em commit anterior ao código.

---

# SEÇÃO 0 — DECISÕES TRAVADAS

1. **UI:** Svelte 5. Não tem VDOM, o runtime é pequeno e a main thread fica
   livre para o render.
2. **Backend:** Supabase (Auth + Postgres + Storage).
3. **Deploy:** Cloudflare Pages. Os headers COOP/COEP ficam em
   `frontend/public/_headers`.
4. **Secrets:** o humano cria as contas fora da sessão. Os secrets
   `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID`, `VITE_SUPABASE_URL` e
   `VITE_SUPABASE_ANON_KEY` ficam no repositório. Os workflows são
   preparados, mas viram no-op enquanto os secrets não existirem.
5. **Branches `[ALTERADO v2.1]`:**
   - Ao iniciar a Fase N, criar `fase-N` a partir da `main` atualizada e
     commitar só nela.
   - Nunca commitar em branch já mergeada.
   - Se o stop hook forçar push, empurrar `fase-N` e abrir PR `fase-N → main`.
   - Antes de iniciar a próxima fase, verificar que o PR anterior foi
     mergeado. Se não foi, PARAR e aguardar.
6. **Correção 1A** (tick lógico único + trajetórias analíticas + libm):
   arquitetura oficial.
7. **Correção 1B** (RNG com `match_seed` e mistura assimétrica): ver 3.B.
8. **Golden de screenshot:** a estrutura prevê um arquivo por navegador,
   em `tests/golden/{chromium,firefox,webkit}/`. Para render, a comparação
   de pixel é só no Chromium; ver Fase 6 `[ALTERADO v2.1]`.
9. **FPS no CI** é só informativo. O gate de FPS é manual, em hardware local.
10. **10.000 partidas Full vs Abstract** só no job noturno. Em cada push,
    roda uma amostra de 500.
11. **Critério 20:** upload contra um mock local (Supabase local ou mock
    HTTP). Não mede banda real.
12. **Render `[ALTERADO v2.1]`:** WebGL2 via `glow` no MVP, atrás da trait
    `Renderer2D`. wgpu pode entrar depois como backend alternativo, em
    `fm-render/src/wgpu_backend.rs`, sem `unsafe` e sem reescrever a camada
    de render.
13. **`unsafe` `[ALTERADO v2.1]`:** só pode existir em dois lugares.
    - **Produção:** `fm-render/src/ffi/`, com os submódulos `sab.rs`,
      `glow_backend.rs` e `wasm_shims.rs`. `#[allow(unsafe_code)]` só em
      `ffi/mod.rs`, valendo para ele e os filhos.
    - **Teste (emenda v2.1):** o crate `fm-test-utils`, módulo
      `alloc_counter`, atrás da feature `alloc-counter`. Esse crate é
      sempre `dev-dependency`, nunca dependência normal.
    - **Motivo da emenda:** o alocador contador do critério 17 exige
      `unsafe impl GlobalAlloc`. Colocá-lo em `fm-render` obrigaria
      `fm-entities` a depender do renderer, o que inverte a hierarquia.
    - Todos os outros crates usam `#![forbid(unsafe_code)]`.
    - O lint de workspace é `unsafe_code = "deny"`, o que cobre também
      testes e benches.
14. **Fluxo de fases `[ALTERADO v2.1]`:** quando a fase estiver completa e o
    CI passar nos 3 navegadores, abrir o PR e aguardar o merge. Depois do
    merge, a próxima fase começa sem pedir nova autorização; basta verificar
    o merge.
15. **Teste falhou `[ALTERADO v2.1]`:** PARAR e trazer o log completo com
    hipóteses, não com uma correção tentativa.

16. **Toolchain fixado `[ALTERADO v2.1]`:** `rust-toolchain.toml` e o CI
    usam Rust 1.99.0 (rustfmt e clippy do mesmo toolchain). O job
    `stable-canary` roda o stable mais recente só como aviso antecipado;
    não bloqueia.
17. **Orçamento de desempenho `[ALTERADO v2.1]`** (substitui o "< 30 ms"):
    - `tick_logic` em Abstract (partida inteira): meta ≤ 40 ms no runner
      do CI, medido por `criterion` no job `bench`. Um gate de teste em
      release falha acima de 50 ms (25% de margem), para que toda regressão
      apareça no PR que a introduz.
    - Fase 12: 10 partidas em Abstract (uma rodada) em ≤ 400 ms, com
      `hardwareConcurrency` workers no runner.
    - O `WorldSimulator` usa `min(navigator.hardwareConcurrency, 10)`
      workers, com barra de progresso e estimativa. Em hardware fraco,
      degrada com elegância em vez de falhar.
    - Tick a 5 Hz está **rejeitado** (violaria 3.A).

## 0.1 — Desvios registrados `[ALTERADO v2.1]`

| Desvio | Motivo | Revisar em |
|---|---|---|
| `wasm-opt` desligado | Download do binaryen bloqueado no sandbox; e uma ferramenta a menos no argumento de paridade bit a bit | Fase 6 (tamanho do binário) |
| `@playwright/test` fixado em 1.56.1 | Casa com o Chromium 141 pré-instalado no ambiente de desenvolvimento | Quando o ambiente mudar |
| Screenshots das Fases 0–5 são evidência, não golden | Fontes do sandbox diferem das do runner do CI | Golden começa na Fase 6 |
| `clippy.toml` proíbe `f32`/`f64::{sin,cos,powf,sqrt,exp,ln,…}` da std | Transforma a regra da libm em erro de CI | — |
| Estado dinâmico do jogador em ponto fixo inteiro (0..=10000) | Determinismo trivial entre alvos; `PlayerDynamic` com 14 bytes | — |
| Benches ficam por crate (`crates/*/benches/`) | Exigência do Cargo; `benches/` na raiz fica reservado | — |
| Teste de fumaça WebGL2 **pulado no Firefox do CI** | Firefox headless no runner sem GPU não cria contexto GL nenhum (`tryNativeGL … FEATURE_FAILURE_WEBGL_EXHAUSTED_DRIVERS`), nem com `webgl.force-enabled=true` e Mesa llvmpipe/EGL instalado (o EGL surfaceless do Mesa funciona; o Firefox não o usa). Firefox real com GPU não é afetado; a lacuna é coberta só por validação manual | Se um runner com GPU ou um Firefox que use EGL surfaceless ficar disponível |

**Pendência resolvida (Fase 4):** atributos de goleiro viram um 5º bloco
(ver 3.I). Atributos são estáticos: **não existe** sistema de evolução ou
recuperação de atributos (treino, idade, retorno de lesão). Se for
desejado, é uma fase própria.

---

# SEÇÃO 1 — CONTEXTO E PAPEL

O assistente atua como Arquiteto de Software Sênior especializado em
motores de simulação esportiva e aplicações web de alta performance. O
objetivo é um jogo de gerenciamento de futebol com:

1. Execução no **navegador** (Chrome, Firefox, Safari, Edge).
2. Autenticação e sincronização na nuvem via Supabase.
3. **Offline-first:** o IndexedDB é a fonte de verdade; a nuvem é sync.
4. Visualização 2D a 60 fps, com jogadores em movimento contínuo, HUD
   tático e overlays que refletem as instruções em tempo real.
5. **Um único motor de partida**, parametrizado por LOD. A lógica que gera
   gols, cartões e decisões é idêntica em todos os LODs.

A sessão roda na nuvem, sem navegador gráfico. Consequências:

- Toda funcionalidade tem teste automatizado.
- Testes visuais usam Playwright headless com screenshots golden.
- Determinismo é testado explicitamente.
- Cada fase entrega um relatório de evidências: screenshots, saídas de
  teste e benchmarks.
- O humano faz a validação visual localmente. Ela deve ser uma
  confirmação, não uma surpresa.

---

# SEÇÃO 2 — STACK

## Engine
- **Rust** stable (edition 2021+).
- **wasm-bindgen** + **wasm-pack**.
- **libm** para TODA função transcendental, via `fm_core::math`. Isso
  garante paridade bit a bit entre nativo e WASM. O `clippy.toml` proíbe
  as versões da std.
- **rand_xoshiro** (xoshiro256++) para o RNG determinístico.

## Render `[ALTERADO v2.1]`
- **WebGL2 via `glow`** (crate `fm-render`), compilado para wasm32. Roda
  na main thread com o `<canvas>`.
- **Trait `Renderer2D`** (segura) é a única API que o resto do jogo
  enxerga. Implementações:
  - `GlowRenderer` (MVP): as chamadas GL vivem em `ffi/glow_backend.rs`.
  - `WgpuRenderer` (futuro, opcional): `wgpu_backend.rs`, sem `unsafe`.
- Pipeline 2D com blending, sem depth buffer. Formas procedurais
  (círculos, retângulos, linhas) desenhadas com instancing.
- Shaders em GLSL ES 3.00.
- **Por que não wgpu no MVP:**
  - WebGPU não está disponível em navegador headless no Linux do CI.
  - O wgpu acrescenta vários MB ao WASM.
  - O fallback WebGL2 do wgpu é justamente o caminho menos testado dele.
- **Validado na Fase 3:** WebGL2 com shader funciona headless em Chromium,
  Firefox e WebKit no CI (ver teste de fumaça).

## Frontend
- **Svelte 5** + **TypeScript** (strict, `noUncheckedIndexedAccess`,
  `exactOptionalPropertyTypes`).
- **Vite** para build e dev.
- **Web Workers** + **SharedArrayBuffer** para o engine rodar fora da
  main thread.

## Persistência e Backend
- **IndexedDB** (via `idb`).
- **Supabase:** Auth (email + Google OAuth via **redirect**, nunca popup,
  porque popup quebra com COOP `same-origin`), Postgres e Storage.
- Abstrações `SaveBackend` e `AuthProvider`.

## PWA
- Service Worker: cache-first para assets, network-first para a API.
- `manifest.json` completo.
- COOP/COEP via `frontend/public/_headers`.

## Testes
- `cargo test` (nativo), `wasm-bindgen-test` (navegador).
- **Playwright** (Chromium, Firefox, WebKit) para E2E.
- **criterion** para benchmarks; gates de tempo como testes `#[ignore]`
  rodados em release.

## CI/CD
- **Jobs:** `test-rust`, `test-wasm`, `test-e2e` (matriz por navegador),
  `bench`, `deploy-preview`, `nightly-deep-tests` (cron).
- Deploy automático para o Cloudflare Pages em push para `main`.

---

# SEÇÃO 3 — ARQUITETURA

## 3.A — Motor único com tick lógico fixo `[CORRIGIDO v2]`

Dar `dt` diferente a cada LOD e ainda exigir placar idêntico é
contraditório: o estado diverge. A solução:

1. **Tick lógico único, igual em todos os LODs:** 10 Hz
   (`LOGICAL_DT = 100 ms`).
   - Em cada tick roda, em ordem: `PhaseStateMachine::update`,
     `DecisionSystem::choose_action`, `ActionResolver::resolve`,
     `FormationAnchor::compute`.
   - Ações e RNG são consumidos APENAS aqui.
2. **Movimento e bola são analíticos (forma fechada):**
   - `PlayerKinematics`:
     `pos(t) = lerp(P0, T, clamp((t - t0) * S / dist(P0, T), 0, 1))`.
   - `BallKinematics`: projétil + Magnus + drag linear, em forma fechada.
   - `pos(t)` é função pura de `(id, t, último evento)`.
3. **O LOD controla só a amostragem:**
   - `Full`: 60 snapshots/s, interpolados, com renderer.
   - `Reduced`: 10 snapshots/s.
   - `Abstract`: nenhum snapshot.
4. **libm em toda função transcendental**, para nativo e WASM ficarem
   bit-idênticos.
5. **`test_cross_lod_consistency`** passa por construção. O chi-quadrado
   continua como verificação redundante.

## 3.B — RNG indexado por evento `[CORRIGIDO v2]`

```rust
pub fn rng_for_event(match_seed: u64, player_id: u32, action_count: u32) -> Rng {
    let packed = ((player_id as u64) << 32) | (action_count as u64);
    let seed = splitmix64(match_seed ^ splitmix64(packed));
    Rng::seed_from_u64(seed)
}
```

**Propriedades:**
- Assimétrica: `(3, 7) != (7, 3)`.
- Depende da partida.
- Reproduzível.

O `match_seed` é guardado no save. Fluxos que não são de partida usam o
mesmo esquema com um separador de domínio XOR na seed. Exemplos:
`rng_for_week` (atualização semanal) e a geração de jogadores.

## 3.C — Espaço contínuo
- Campo de 105 × 68 m, com a origem no canto inferior esquerdo. O gol
  `Left` fica em `x = 0`.
- Posições: `Vec2`/`Vec3` em metros. Sem grid; varredura linear
  (≤ 23 entidades).

## 3.D — Snapshot e interpolação
- `MatchSnapshot` é quase POD, sem alocações.
- O engine escreve num ring buffer duplo em `SharedArrayBuffer`
  (`fm-render/src/ffi/sab.rs`), sem locks.
- O renderer desenha `lerp(prev, curr, alpha)`.

## 3.E — IA de jogador (por tick lógico, em todos os LODs)
1. `FormationAnchor::compute(role, ball_pos, phase, tactics) -> Vec2`
2. `RoleBehavior::update(player, ctx)`
3. `DecisionSystem::choose_action(player, ctx) -> Action`
4. `ActionResolver::resolve(action, player, ctx) -> ActionResult`
   (consome `rng_for_event`)

O movimento é `PlayerKinematics::plan_trajectory`, resolvido na amostragem.

## 3.F — Fases de jogo
- `InPossession`
- `OutOfPossession`
- `TransitionAttack` (roubou a bola nos últimos 3 s)
- `TransitionDefense` (perdeu a bola nos últimos 3 s)
- `SetPiece`

## 3.G — Física da bola (analítica) `[ALTERADO v2.1]`
- **Rolando:** desaceleração por damping linear (`v(t) = v0·e^(-μt)`),
  em forma fechada.
- **No ar:** drag linear só no plano horizontal; o eixo vertical é uma
  parábola pura (gravidade sem drag).
  - Motivo: com drag no eixo vertical, achar o instante do quique exige
    raiz de equação transcendental. Com parábola pura, o quique sai de uma
    equação de 2º grau, em forma fechada e sem iteração.
  - O erro físico é pequeno nas distâncias de um campo.
- **Magnus:** adiado para depois do MVP.
- **Bounce:** `v_z' = -restitution · v_z`, mais atrito horizontal no
  impacto. A solução reinicia a partir daí.
- **Cadeia de segmentos:** no instante do chute, a trajetória inteira
  (voo → quiques → rolagem → parada) é calculada uma vez e guardada num
  array fixo de segmentos. `pos_at(t)` escolhe o segmento: é função pura do
  chute e não aloca.
- **Domínio:** se `dist(bola, pé) < raio_controle` no tick lógico, rola
  `first_touch`.

## 3.H — Renderização `[ALTERADO v2.1]`

```
fm-render/
  src/lib.rs            trait Renderer2D, Camera, DrawList (seguro)
  src/glow_renderer.rs  GlowRenderer: impl Renderer2D usando ffi::glow_backend
  src/ffi/mod.rs        #![allow(unsafe_code)] — único lugar com unsafe
  src/ffi/glow_backend.rs  chamadas glow/WebGL2 (contexto, buffers, shaders)
  src/ffi/sab.rs        ring buffer em SharedArrayBuffer
  src/ffi/wasm_shims.rs cola JS que não sai segura via wasm-bindgen
  src/wgpu_backend.rs   (futuro) WgpuRenderer, sem unsafe
```

**Contrato da `Renderer2D`** (esboço; finalizado na Fase 6):

```rust
pub trait Renderer2D {
    fn resize(&mut self, width_px: u32, height_px: u32, dpr: f32);
    fn begin_frame(&mut self, camera: &Camera, clear: Rgba);
    fn draw(&mut self, list: &DrawList); // círculos, retângulos, linhas, texto
    fn end_frame(&mut self) -> Result<(), RenderError>;
}
```

**Regras:**
- A `DrawList` é montada por código seguro e independente de backend
  (campo → sombras → jogadores → bola → labels → overlays → HUD).
- O backend só traduz a `DrawList` em chamadas de GPU.
- Câmera com 3 modos: `FullPitch`, `HalfPitch` (segue a bola) e
  `Tactical`.
- Formas procedurais no MVP; sprites ficam para depois do MVP.
- O renderer não lê nem escreve estado de jogo. Recebe snapshots e
  produz pixels. Overlays não afetam o determinismo.

## 3.I — Entidades
- `PlayerDatabase` em SoA: `Vec<PlayerStatic>` (frio) +
  `Vec<PlayerDynamic>` (quente). `PlayerId(u32)` é a única chave; nunca
  `Rc`/`Arc<Player>`.
- Atributos em 5 blocos POD (10/9/8/8/6 × `u8`, escala 1..=100).
  - O 5º bloco é `GoalkeepingAttributes`: `reflexes`, `handling`,
    `positioning_gk`, `aerial_reach`, `one_on_ones`, `distribution`.
    `size_of == 6`.
  - O bloco está sempre presente (tamanho fixo), mas só é consultado
    quando o papel é goleiro. `[ALTERADO v2.1]`
- No `weekly_update`, o goleiro acumula 40% da fadiga de um jogador de
  linha por minuto e tem metade do risco de lesão por minuto, porque
  percorre muito menos distância. `[ALTERADO v2.1]`
- `size_of::<PlayerDynamic>() <= 24`, garantido em tempo de compilação
  (atual: 14).

## 3.J — Auth (Supabase)
- Telas: entrar, criar conta, continuar como convidado.
- Convidado: save só no IndexedDB. Autenticado: save local + sync.
- Google OAuth via **redirect**.
- Sessão em `localStorage`, com rotação de refresh.
- Logout não apaga o save local.
- O save de convidado migra para a conta no primeiro signup.

## 3.K — Sync offline-first
- O IndexedDB é a fonte de verdade.
- O sync worker empurra mudanças quando há rede, com fila persistente.
- Save em chunks versionados.
- Conflito: last-write-wins por chunk, com o timestamp do servidor.
- O sync pausa durante partida em LOD `Full`.
- A UI mostra o status.

## 3.L — PWA
- Service Worker: cache-first para assets, network-first para a API, sem
  cachear auth.
- `manifest.json` com `display: standalone`, orientação `landscape`.
- Headers:

```
/*
  Cross-Origin-Opener-Policy: same-origin
  Cross-Origin-Embedder-Policy: require-corp
```

---

# SEÇÃO 4 — ESTRUTURA DE DIRETÓRIOS `[ALTERADO v2.1]`

```
football-game/
├── Cargo.toml                (workspace)
├── clippy.toml               (proíbe transcendentais da std)
├── rust-toolchain.toml
├── package.json  vite.config.ts  playwright.config.ts  tsconfig.json
├── docs/SPEC.md              (este arquivo)
├── .github/workflows/
│   ├── ci.yml                (test-rust, test-wasm, test-e2e, bench)
│   ├── nightly.yml           (10k partidas Full vs Abstract)
│   └── deploy.yml            (Cloudflare Pages; no-op sem secrets)
├── crates/
│   ├── fm-core/              (math/libm, Vec2/Vec3, pitch, Rng)
│   ├── fm-entities/          (PlayerDatabase, atributos, weekly_update)
│   ├── fm-match/             (tick lógico, cinemática, decisão, snapshot)
│   ├── fm-economy/           (MarketValue, TransferAI)
│   ├── fm-world/             (WorldSimulator, League, Fixture)
│   ├── fm-persistence/       (serialização, versionamento)
│   ├── fm-render/            (Renderer2D; ffi/ = único unsafe de produção)
│   ├── fm-test-utils/        (só dev-dependency; alloc-counter)
│   └── fm-wasm/              (bindings wasm-bindgen)
├── frontend/
│   ├── index.html
│   ├── public/ (_headers, manifest.json, icons/, sw.js)
│   └── src/ (app, auth, save, engine-bridge, render, ui, workers)
├── tests/
│   ├── e2e/                  (Playwright)
│   └── golden/{chromium,firefox,webkit}/
└── README.md
```

---

# SEÇÃO 5 — FASES

Uma fase por vez, na branch `fase-N`. Ao final de cada uma:

1. `cargo test --workspace`
2. `cargo clippy --workspace --all-targets -- -D warnings` (nativo e
   wasm32)
3. `cargo fmt --check`
4. `wasm-pack test --headless --chrome crates/fm-wasm`
5. `npx playwright test` (Chromium/Firefox/WebKit, no CI)
6. Abrir o PR `fase-N → main` com o CI verde e aguardar o merge.
7. Relatório: screenshots, benchmarks e saída dos testes.

Se algum teste falhar: PARAR e reportar o log completo com hipóteses.

## FASE 0 — Bootstrap ✅
Workspace, Vite + Svelte 5 + TS strict, `wasm-pack`, Playwright nos 3
navegadores, página "Hello from Rust (libm: π)", tabela golden de libm,
`ci.yml` e `deploy.yml`.

## FASE 1 — Core + Geometria + RNG + libm ✅
`math` (libm), `Vec2`/`Vec3`, `pitch`, `Rng`, `rng_for_event`, testes de
assimetria e colisão, e paridade nativo/WASM.

## FASE 2 — Entidades ✅
Atributos, `PlayerStatic`/`PlayerDynamic`/`PlayerBio`, `PlayerDatabase`
SoA e `weekly_update` (500k em ~14 ms). Zero alocação. Digest nativo ==
WASM.

## FASE 3 — Espaço, Formação, Fases ✅
- **Primeiro commit `[ALTERADO v2.1]`:** este `docs/SPEC.md` + teste de
  fumaça de WebGL2 (glow, shader GLSL ES 3.00, readback de pixel) nos 3
  navegadores.
- `Formation`: 4-4-2, 4-3-3, 4-2-3-1, 3-5-2, 5-3-2.
- `FormationAnchor::compute(slot, ball_pos, phase, tactics, frame) -> Vec2`.
- `Phase` + `PhaseStateMachine` com timers.
- `PlayerKinematics::plan_trajectory` / `pos_at`.
- `RoleBehavior`: stubs por posição.
- **Testes:** âncoras; o centro de massa desloca com a bola; `pos_at` é
  determinístico e independe do número de avaliações.

**Decisões de interface `[ALTERADO v2.1]`:**
- **Espaço relativo ao time (`TeamFrame`):** formações e táticas são
  escritas uma vez, com `depth` 0 = próprio gol e 1 = gol adversário, e
  `lateral` -1 = direita do time e +1 = esquerda. O `TeamFrame` espelha
  essas tabelas para quem ataca para a esquerda, e há teste garantindo a
  simetria exata entre os dois lados.
- **Tempo de partida em `u32` milissegundos**, nunca em f32 acumulado.
  `LOGICAL_DT_MS = 100` e `TRANSITION_TICKS = 30` (3 s), contados em ticks
  lógicos.
- **Assinatura de `plan_trajectory`:** ficou
  `plan_trajectory(from, target, speed_m_s, t_ms)`. É uma função pura, sem
  depender do struct do jogador. `replan` parte de `pos_at` no instante da
  nova decisão, então não há teletransporte.
- **Velocidade máxima:** `top_speed(pace) = 5,5 + 0,035·pace` m/s (de
  5,5 a 9,0 m/s).
- **Recalibração:** a partir da Fase 4, os valores das âncoras ficam em
  `AnchorTuning` (ver Fase 4).
- **Táticas da Fase 3:** `Mentality` (5 níveis, ±8,4 m no bloco),
  `Width` e `LineHeight` (±5 m na defesa, metade no meio-campo).
- **Bola solta (`Possession::Loose`):** os times mantêm a fase e o
  relógio de transição continua correndo.

## FASE 4 — Tick Lógico + Action Resolver (FASE-CHAVE)
- `LOGICAL_DT = 100 ms`.
- `MatchEngine::tick_logic()` não conhece o LOD.
- `ActionResolver`: `resolve_pass`, `resolve_shot`, `resolve_tackle`,
  `try_receive`, todos via `rng_for_event`.
- `MatchEngine::sample(lod, t)`.
- **Critérios de saída obrigatórios `[ALTERADO v2.1]`:**
  - `test_cross_lod_consistency` (BLOQUEANTE)
  - `test_determinism_across_runs`
  - `test_libm_parity_in_engine`

**Decisões da Fase 4 `[ALTERADO v2.1]`:**
- **`AnchorTuning`:** todas as constantes de `FormationAnchor` (forma por
  fase, centro do bloco, largura, altura da linha, peso da linha,
  mentalidade, goleiro, limites) ficam num único struct. `Default` usa os
  valores calibrados na Fase 3, sem nenhuma mudança de valor. A Fase 5
  ajusta por ali.
- **Isolamento do motor:** `MatchEngine` copia, na criação, os atributos
  dos 22 titulares (`MatchPlayer`). Durante a partida não toca no
  `PlayerDatabase`. Assim o motor é autocontido e roda igual em nativo, em
  WASM e em worker.
- **`DecisionSystem::choose_action` mínimo:** a versão completa é da
  Fase 5. Na Fase 4:
  - o portador escolhe entre chutar, passar, conduzir e segurar, por
    pontuação determinística;
  - o defensor mais próximo pressiona e tenta o desarme.
  
  A escolha não consome RNG. Só o `ActionResolver` consome.
- **Desfecho do chute decidido no chute:** gol, defesa ou fora é sorteado
  em `resolve_shot`, usando os atributos de goleiro. O evento só é aplicado
  no tick em que a bola chega à linha.
- **Eventos:** gol, chute, defesa, falta, cartão (amarelo e vermelho; o 2º
  amarelo vira vermelho) e saída de bola. Ficam num log com capacidade
  pré-alocada, e `tick_logic` não aloca (critério 17).
- **Paridade nativo × WASM:**
  - arquivo de referência gerado no nativo com 180 snapshots completos (1
    a cada 300 ticks) da partida inteira, comparados campo a campo, bit a
    bit;
  - mais o placar e a lista de eventos;
  - mais um digest de todos os 54.000 ticks.
  
  Gerar todos os snapshots completos daria vários MB no repositório.
- **Auditoria de assinaturas (critério 18):** um teste lê o fonte e
  garante que `choose_action` e `resolve` não recebem `dt` nem `LodLevel`.

**Fase 5 (a) — `TickFrame` concluído:** a paridade bit a bit continua
inalterada. O `tick_logic` em Abstract (partida inteira) caiu de 70 ms para
52 ms no ambiente de desenvolvimento e mede **37,0 ms no runner do CI**
(criterion; o gate de 50 ms passa com mediana de 37,05 ms). Está dentro do
orçamento de 40 ms.

**Fase 5 (b) — investigação dos ~876 desarmes `[ALTERADO v2.1]`:**

*Definição exata no código (Fase 4):* uma "tentativa de desarme" é cada
chamada de `ActionResolver::resolve_tackle` (contador `TeamState::tackles`).
O `tick_logic` dispara essa chamada quando todas estas condições valem ao
mesmo tempo:
1. a bola tem portador (`BallState::Held`) há pelo menos 3 ticks;
2. o time defensor está fora do intervalo `TEAM_TACKLE_GAP` (70 ticks,
   cerca de 7 s, contados desde a última tentativa do time);
3. existe um jogador de linha do time defensor a menos de `TACKLE_RANGE`
   (1,8 m) do portador e fora da própria cooldown (25 ticks, ou 40 se foi
   driblado).

Não há decisão de "dar o bote ou conter": toda oportunidade é aproveitada.

*Medição (20 partidas, `tick_logic` puro):*
- a bola tem portador em 87% dos ticks;
- em **97%** desses ticks há um defensor a **menos de 1 m** do portador;
- são cerca de 875 tentativas e 16,6 faltas por partida, ou seja,
  **1,9% de faltas por tentativa**.

*Classificação: **problema de modelo**, não de constante.* São três causas:
1. **Marcação sobre o portador.** O primeiro marcador recebe como alvo a
   posição exata do portador e converge para distância zero. Dois corpos
   no mesmo ponto, o tempo todo.
2. **Sem decisão defensiva.** A frequência de tentativas é ditada só por
   cooldowns (`TEAM_TACKLE_GAP`). Com o marcador colado, o número sai
   aproximadamente de "tempo de posse ÷ intervalo", e não de jogo.
3. **Compensação cruzada.** A probabilidade de falta foi calibrada para
   baixo (cerca de 2% por tentativa) para compensar o excesso de
   tentativas, enquanto no futebol real a fração de botes que viram falta
   é muito maior. Ajustar só as constantes manteria as duas distorções se
   cancelando.

*Correção aprovada (modelo de defesa do portador) `[ALTERADO v2.1]`:*
1. **Contenção por faixa de perigo.** A distância que o marcador mantém
   do portador depende de onde está a bola, medida pela distância ao gol
   que o time defende. Cada faixa é uma constante separada em
   `TuningParams`:
   - meio-campo defensivo / longe do gol: 3–4 m;
   - meio-campo ofensivo / entrada da área: 2–3 m;
   - área / zona de finalização: 1–2 m.

   O ponto de contenção fica do lado do gol (entre o portador e o gol
   defendido), na distância da faixa.
2. **Decisão de bote no `DecisionSystem`**, determinística e sem RNG:
   - *Elegibilidade:* defensor de linha, recuperado (cooldown individual) e
     ao alcance (`TACKLE_RANGE`).
   - *Pontuação do bote:* soma
     - **ângulo** do defensor em relação ao gol: estar goal-side (entre o
       portador e o gol) pontua alto, perseguir por trás pontua baixo;
     - **vulnerabilidade do portador:** acabou de dominar (poucos ticks com
       a bola);
     - **perfil do defensor:** `tackling` e `decisions` pesam a favor;
       `aggression` antecipa o bote;
     - **contexto:** pontua mais na transição defensiva e menos dentro da
       própria área (risco de pênalti).
   - *Regra:* dá o bote se a pontuação passar de um limiar
     (`TuningParams`); senão, mantém a contenção.
   - O ângulo também entra no `ActionResolver`: bote por trás tem mais
     chance de falta e menos de ganhar a bola.
3. **Remoção do `TEAM_TACKLE_GAP`.** Só restam as cooldowns individuais,
   que modelam a recuperação física.
4. **Falta por bote recalibrada** contra metas reais, depois do passo
   anterior.

**Restrição arquitetural `[ALTERADO v2.1]`:** nenhuma decisão do
`DecisionSystem` (nem do `ActionResolver`) pode consultar estado de
amostragem: interpolação do LOD Full, snapshots ou `sample()`. Só o estado
do `tick_logic`, via `MatchState` e `TickFrame`. O teste de paridade
detectaria uma violação, mas isso é regra de design, não um bug a ser pego
por teste.

**Orçamento:** a meta continua em 40 ms e o gate em 50 ms. Se a Fase 5
estourar, o perfil aponta o custo e o assunto volta para decisão humana.
A meta não é ajustada.

**`TuningParams` `[ALTERADO v2.1]`:** todas as constantes de calibração do
motor (âncoras, decisão, duelo, falta e cartões, chute, passe, domínio,
contenção e reinícios) ficam num único struct, passado em `MatchSetup`.
Primeiro a centralização é feita com os valores atuais e a referência de
paridade **não pode mudar**; só depois o modelo de defesa é alterado.

**Calibração base da Fase 4 (média de 60 partidas demo, `examples/match_stats`):**

| Métrica por partida | Fase 4 | Real (aprox.) | Situação |
|---|---|---|---|
| Gols | 2,4 | 2,7 | ok |
| Chutes (no alvo) | 11,5 (5,4) | 25 (9) | baixo |
| Faltas | 17 | 22 | ok |
| Amarelos / vermelhos | 2,2 / 0,32 | 4 / 0,15 | amarelo baixo, vermelho alto |
| Pênaltis | 1,1 | 0,3 | alto |
| Passes (acerto) | 358 (57%) | 900 (80%) | baixo |
| Tentativas de desarme | 876 | ~70 | muito alto (só 2% viram falta) |
| Custo nativo, release | 72 ms | — | Fase 12: 380 partidas → ~27 s em 1 thread |

Causa comum: ainda não há organização defensiva (marcação, pressão
coordenada) nem circulação de bola. Isso é exatamente a Fase 5, que deve
recalibrar contra a coluna "Real". As constantes estão em `AnchorTuning`,
`decision.rs` e `resolver.rs`, cada uma com uma linha de racional.

## FASE 5 — Role Behaviors
- **Ordem obrigatória `[ALTERADO v2.1]`:**
  - (a) Refatorar para `TickFrame` e medir de novo.
    **Gate:** se `tick_logic` passar de 40 ms, PARAR.
  - (b) Investigar os ~876 desarmes como problema de modelo.
  - (c) Calibrar chutes e passes em conjunto.
  - (d) Restante: comportamentos por papel e `TuningParams`.
- **`TickFrame` `[ALTERADO v2.1]`:** estrutura na pilha, montada uma vez
  por tick lógico.
  - **Conteúdo:**
    - posições das 22 entidades no instante do tick (constantes durante o
      tick, porque as trajetórias só são replanejadas no fim);
    - âncoras de formação dos 22;
    - fase de cada time;
    - posse e bola parada, usadas pela `PhaseStateMachine`.
  - **A bola não é cacheada:** o estado dela muda dentro do tick (recepção,
    chute, desarme). `TickFrame::ball` deriva a posição do estado atual da
    bola usando as posições cacheadas dos jogadores.
  - **Regra:** `DecisionSystem`, `ActionResolver` e `PhaseStateMachine`
    recebem `&TickFrame` e nunca chamam `pos_at` nem
    `FormationAnchor::compute` diretamente.
  - **Prova de equivalência:** o arquivo de referência de paridade não
    muda.
- State machines completas por posição e `DecisionSystem::choose_action`.
- **Testes:**
  - Overlap Left: o lateral-esquerdo muda em ≤ 1 s.
  - Pressing: os defensores fecham em ≤ 3 s.
  - Counter Attack: o tempo até o chute cai depois de uma roubada.
  - Tight Marking: a distância média cai 2 m.

## FASE 6 — Snapshot + Renderer2D (glow/WebGL2) + Canvas `[ALTERADO v2.1]`
- `MatchSnapshot` POD em `SharedArrayBuffer`, com ring buffer duplo
  (`ffi/sab.rs`).
- `fm-wasm` expõe `init_engine(seed)`, `tick_logic()`,
  `sample_snapshot()` e `get_snapshot_ptr()`.
- `engine.worker.ts` escreve snapshots no SAB.
- `fm-render`:
  - trait `Renderer2D` + `DrawList` + `Camera` (seguros);
  - `GlowRenderer` sobre `ffi/glow_backend.rs`;
  - shapes procedurais com instancing.
- A interpolação `prev`/`curr` fica na camada segura.
- Reavaliar o `wasm-opt` (tamanho do binário).
- **Estratégia de golden `[ALTERADO v2.1]`:**
  - Comparação de pixel **só no Chromium**: o SwiftShader do CI é
    renderizador por software determinístico.
  - **Firefox e WebKit** rodam os mesmos testes de render **sem comparação
    de pixel**, só para garantir que não quebram. O WebKit do CI informa
    "Apple GPU", mas é máscara de privacidade: no Linux é software, e não se
    sabe se é estável entre runs.
  - **Firefox no CI não tem WebGL** (ver 0.1): os testes de render são
    pulados nele, e a cobertura do Firefox fica na validação manual.
  - Isso substitui o item 8 da Seção 0 para screenshots de render.
  - Investigar a estabilidade do WebKit é opcional e não bloqueia nada.
- **Testes Playwright:**
  - `match-render.spec.ts`: screenshot comparado com o golden do
    Chromium, com tolerância de 1%. No WebKit, só checa que não há erro de
    render. No Firefox, é pulado no CI (ver 0.1).
  - Determinismo visual (Chromium): 3 execuções com a mesma seed geram
    screenshots idênticos.
  - Teste de contrato da `Renderer2D` com um backend de gravação (sem
    GPU): a mesma `DrawList` gera a mesma sequência de comandos.
  - FPS medido e informado (não é gate).

## FASE 7 — UI + Overlays Táticos
- Telas: menu, elenco, táticas e calendário.
- Overlays via `Renderer2D`: linha de impedimento, zonas de pressão,
  linhas de formação, linhas de passe e setas de instrução.
- HUD.
- **Testes Playwright:**
  - Overlap Left via UI.
  - Pressing.
  - Overlays não afetam o determinismo.

## FASE 8 — Auth (Supabase)
- Login, signup, magic link, convidado e logout.
- Google OAuth via redirect.
- `AuthProvider` / `SupabaseAuthProvider`.
- Migração do save de convidado.
- **Testes:** `auth.spec.ts`, convidado e migração.

## FASE 9 — Sync Offline-First
- `LocalSaveStore`, `CloudSaveStore`, `SyncWorker` com fila persistente,
  last-write-wins por chunk.
- **Testes:**
  - 2 contexts sincronizam em < 5 s.
  - Offline por 30 s, depois sync.
  - Conflito resolvido de forma determinística.
- **Benchmark:** 50 MB contra mock local em < 5 s.

## FASE 10 — PWA
- `manifest.json`, ícones, service worker, `_headers`.
- OAuth redirect funcionando com COOP.
- **Testes:** `offline.spec.ts`; service worker ativo.

## FASE 11 — Motor Econômico
- `MarketValue::estimate`, `TransferAI::evaluate_sale`,
  `player_accepts_move` e `scout_targets`.
- Testes de cenário.

## FASE 12 — World Simulator
- `JobSystem` sobre Web Workers.
- `WorldSimulator::simulate_round` e `weekly_tick`.
- **Testes:**
  - 20 clubes × 38 rodadas em < 10 s.
  - Chi-quadrado Full vs Abstract com `p > 0.05`: 500 partidas por push,
    10.000 no nightly.

## FASE 13 — Demo + Deploy
- Bootstrap com 500k jogadores, 200 clubes e 10 ligas.
- Deploy no Cloudflare Pages.
- README e relatório final.

---

# SEÇÃO 6 — CONVENÇÕES

## Rust
- `clippy::all` + `clippy::pedantic` com `-D warnings`.
- **`unsafe` `[ALTERADO v2.1]`:**
  - só em `fm-render/src/ffi/` (produção) e em
    `fm-test-utils::alloc_counter` (testes); ver 0.13;
  - os demais crates usam `#![forbid(unsafe_code)]`;
  - todo bloco `unsafe` leva comentário `// SAFETY:`.
- `snake_case` para funções e variáveis, `PascalCase` para tipos.
- Nunca `Rc<Player>`/`Arc<Player>`.
- Comentários explicam o PORQUÊ.
- Fórmulas de probabilidade têm racional em 1 linha.

## TypeScript
- Strict + `noUncheckedIndexedAccess`.
- Sem `any` (use `unknown` + type guards).
- Sem `enum` (use union types).
- Preferir `readonly`.

## Testes
- Todo módulo Rust tem `#[cfg(test)] mod tests`.
- Toda feature de UI tem teste Playwright.
- Determinismo testado explicitamente.
- Goldens por navegador.

---

# SEÇÃO 7 — FORMATO DE ENTREGA POR FASE

1. Resumo.
2. Arquivos.
3. Saída real dos testes.
4. Screenshots.
5. Benchmarks.
6. PR `fase-N → main`.
7. Aguardar o merge.

---

# SEÇÃO 8 — RESTRIÇÕES DE COMPORTAMENTO

- **Ritmo:**
  - Uma fase por vez.
  - CI vermelho é bloqueante.
  - A próxima fase só começa depois de verificar o merge do PR anterior.
- **Testes:** se um teste falhar, PARAR e trazer o log + hipóteses.
- **Arquitetura:**
  - Decisão sub-ótima detectada: PARAR e propor uma alternativa com dados.
  - Prompt ambíguo: perguntar.
  - Dependência pesada: consultar antes.
  - NUNCA modificar o tick lógico único nem o RNG indexado por evento.
  - Mudança de arquitetura atualiza `docs/SPEC.md` no mesmo PR, antes do
    código.
- **Segurança:** NUNCA commitar credenciais.
- **Numérico:** libm para TODA função transcendental (o clippy impõe).

---

# SEÇÃO 9 — CRITÉRIOS DE ACEITAÇÃO FINAL

1. `cargo test --workspace` passa 100%.
2. `npx playwright test` passa em Chromium, Firefox e WebKit.
3. `npm run dev` em `localhost:5173`.
4. `npm run build && npm run preview`.
5. Deploy em produção (depois dos secrets).
6. PWA instalável, funcionando em modo avião.
7. Login/signup via Supabase (email + Google redirect).
8. Save sincroniza entre dois navegadores em < 5 s.
9. Offline por 1 min, depois sync automático.
10. `test_cross_lod_consistency` idêntico entre Full, Reduced e Abstract.
11. `test_determinism_across_runs` (tick 500).
12. `test_libm_parity_in_engine`: nativo == WASM, bit a bit.
13. Golden screenshots: comparação de pixel no Chromium; Firefox e WebKit
    sem erro de render.
14. FPS: informativo no CI; gate manual ≥ 55 fps.
15. `weekly_update` de 500k jogadores em < 150 ms.
16. 20 clubes × 38 rodadas em < 10 s, com `min(cores, 10)` workers; uma
    rodada de 10 partidas em ≤ 400 ms no runner (ver Seção 0, item 17).
17. Nenhuma alocação em `MatchEngine::tick_logic` nem em
    `PlayerDatabase::weekly_update` após o bootstrap (alocador contador).
18. `DecisionSystem::choose_action` e `ActionResolver::resolve` sem `dt`
    nem `LodLevel`.
19. Defensive → Attacking: o centro de massa sobe ≥ 5 m em ≤ 5 s.
20. Sync de um save de 500k jogadores contra mock local em < 5 s.
