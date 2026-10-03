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
18. **A estimativa usa a mesma física da execução `[ALTERADO v2.1]`**
    (invariante de design, 2026-10-02): toda estimativa da decisão (sucesso
    de passe, disputa pela bola, drible, tabela, corrida com a bola) usa a
    cinemática, a física da bola e as regras que o motor de fato executa.
    Se um comportamento precisar de algo que a execução não tem (inércia,
    reação, giro), isso entra **primeiro** na execução e só depois na
    estimativa. Origem: o passe em profundidade com acerto de 0,3% e o
    passe longo estimado como rasteiro (Fase 5, item 5).

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
     **`[ALTERADO v2.1]`** a partir da Fase 5 (c1), cada trajetória tem
     até três fases, todas em forma fechada (ver Fase 5, "Física de
     movimento"): reação, aceleração constante e cruzeiro (a fórmula
     acima, a partir do fim da aceleração).
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

*Achados na implementação (adições ao modelo aprovado):*
- **Impasse.** Sem botes contínuos, um portador encurralado na linha de
  fundo ficava com a bola pelo resto da partida:
  - o chute estava bloqueado e abaixo do limiar;
  - a condução mirava o limite do campo;
  - "segurar" virava a escolha permanente;
  - o goleiro não podia disputar a bola.

  O primeiro teste do modelo novo mostrou 77 botes por partida, mas era
  **artefato do impasse**: as partidas travavam cedo. Correções:
  - **saída forçada:** quando a condução perde todo o valor
    (`carry_decay_start + carry_decay_span` ticks), o portador chuta se
    estiver no raio de chute; senão, passa para a melhor opção, mesmo com
    nota baixa;
  - **goleiro sai no pé** dentro da própria área (`keeper_smother`), com
    `one_on_ones`/`handling` e sem a penalidade de área;
  - `w_linger` (bônus por portador parado) fica no `TuningParams`, mas com
    valor **0**: medido, não mudou o resultado.
- **Calibração do modelo (60 partidas):**
  - marcador mais próximo do portador: <1 m 9%, 1–2 m 26%, 2–3 m 24%,
    3–4 m 18%, 4–6 m 12%, >6 m 11%. Antes, 97% ficava abaixo de 1 m;
  - alguém ao alcance de bote em 29% dos ticks de posse (antes, 97%);
  - limiar de bote 1,15 (a mediana das notas ao alcance é 0,53): **75
    botes por partida** (real ~70; antes 876);
  - falta por bote: base 0,26 + 0,24 × agressividade, mais o termo "por
    trás": **19,4 faltas** (real ~22) e 2,15 amarelos;
  - vermelhos (0,53) e pênaltis (0,02) ficam para a Fase 6.
- **Efeito colateral para o item (c):** sem o marcador colado, a posse
  flui demais: 3.580 passes (real ~900) e 66 chutes (real ~25), com 3,5
  gols. A calibração conjunta de chutes e passes é o próximo item.

**(c) Chutes e passes — como a decisão é feita `[ALTERADO v2.1]`:**
- **Quem decide:** só o portador, a cada tick, em
  `DecisionSystem::choose_action`, sem RNG. A execução (precisão, defesa
  do goleiro, domínio) é do `ActionResolver`, com RNG.
- **Ordem da decisão:**
  1. Se `holder_ticks < min_hold_ticks` (4; goleiro 15) e ninguém está a
     menos de `pressure_radius` (2,5 m) → segura (`Hold`). Se está
     pressionado, essa espera é pulada.
  2. Três notas, todas determinísticas:
     - **chute** = (base + habilidade) × (1 − distância/25 m) ×
       centralidade × bloqueio × ganho. O bloqueio vale 0,3 se qualquer
       defensor de linha estiver a menos de 1 m da linha portador→gol;
     - **passe** (o melhor dos companheiros a 4–45 m) = base + progresso
       + linha livre − comprimento, com penalidade para linha estreita e
       para passe ao goleiro;
     - **condução** = (base + espaço 6 m à frente), ou 0,15 se apertado;
       decai depois de 2,5 s com a bola e vale metade perto do gol.
  3. Chuta se a nota do chute passa de 0,6 e é a maior das três.
  4. **Saída forçada** (da correção de (b)): com 5,5 s com a bola, chuta
     se estiver a menos de 25 m do gol, senão passa.
  5. Senão, passa se o passe ≥ condução; senão, conduz.
- **Penalizações duplas encontradas:** as três vêm do mesmo marcador de
  contenção.
  1. **Chute bloqueado pelo marcador de contenção.** O ponto de contenção
     fica, por construção, *exatamente* na linha portador→gol. Todo
     portador contido tem a nota de chute × 0,3 e, se chutar, a bola ainda
     pode ser interceptada fisicamente em voo. Medido (60 partidas): **98%
     dos chutes vinham da saída forçada**, não da decisão normal. Por isso
     os chutes eram próximos (46% a menos de 11 m) e os gols altos (3,85).
  2. **Contenção conta como pressão.** As faixas de contenção (área 1–2 m,
     meio 2–3 m) ficam dentro de `pressure_radius` (2,5 m). O mesmo
     defensor (i) pula o tempo mínimo com a bola, (ii) deixa a condução
     "apertada" (0,15) e (iii) piora a precisão do passe no resolver.
     Medido: **66% dos passes saíam com menos de 0,5 s de bola**.
  3. **Distância** entra na nota de chute e na execução (no alvo − d/60,
     defesa + d/80). Isso é antecipação legítima (a decisão estima o
     resultado), não um bug, mas as duas curvas se somam e foram
     calibradas juntas.
- **Outros achados:**
  - **tempo de bola parada irreal:** reinícios esperavam 1,5–4 s (real:
    lateral ~9 s, tiro de meta ~16 s, falta ~20 s, escanteio ~25 s,
    saída depois de gol ~45 s). Bola parada era 4% do jogo (real ~35%), o
    que infla todas as contagens por partida em ~1,5×;
  - **domínio ruim demais:** um receptor típico erra o primeiro toque em
    15–20% das vezes (`control.base` 0,6); real ~3–5%. 26% dos passes
    terminavam em bola solta por domínio errado.
- **Calibração só com constantes — resultado:** busca por coordenadas
  (30 partidas por ponto, sementes fixas) sobre 16 constantes de decisão,
  chute, passe, domínio, reinício, bote e falta, contra as metas reais.

  | Configuração | Gols | Chutes (alvo) | Passes (acerto) | Botes | Faltas | Bola parada |
  |---|---|---|---|---|---|---|
  | Atual | 3,6 | 63 (18) | 3.610 (63%) | 74 | 19 | 4% |
  | Melhor ponto, domínio atual | 2,1 | 30 (10) | 1.303 (64%) | 68 | 25 | 39% |
  | Melhor ponto + domínio real | 1,0 | 17 (6) | 1.560 (87%) | 32 | 12 | 20% |
  | Real (aprox.) | 2,7 | 25 (9) | 900 (80%) | 70 | 22 | ~35% |

  O melhor ponto encosta no limite do realista: o tempo mínimo com a bola
  fica em 2,5 s e os reinícios ficam acima dos reais. Mesmo assim, os
  passes ficam em 1.300.
- **Conclusão — é problema de modelo:** os números "bons" do melhor ponto
  vêm do mecanismo errado. Chances, botes, faltas e reinícios nascem da
  bola solta por domínio ruim. Com domínio realista o jogo fica estéril:
  o time circula a bola (87% de acerto) mas não cria. Faltam
  comportamentos que geram chance:
  - passe para o espaço (bola na frente do companheiro que corre), não só
    no pé;
  - corrida de ruptura sem bola;
  - drible 1×1 contra o marcador de contenção (hoje a condução contra um
    marcador é só "apertada", sem duelo).

  Esses itens são do escopo de (d) (comportamentos por papel). Calibrar
  (c) antes de (d) fixaria constantes que (d) vai invalidar.
- **Decisão (2026-10-02): (c) vira (c1) + (c2).**
  - **(c1) — modelo de criação de jogadas, escopo B (médio):**
    - xG estimado no momento da decisão (distância e ângulo da boca do
      gol). O bloqueio passa a ser a fração do gol coberta pelos corpos na
      frente. A saída forçada não chuta mais: passa ou afasta;
    - domínio realista desde o início (`first_touch`, velocidade e altura
      da bola, pressão);
    - linha de impedimento como dado do tick;
    - estado `Run` para atacantes e pontas, respeitando a linha;
    - passe em profundidade mirando à frente do corredor;
    - tabela xT + moeda comum de valor para chute, passe e condução
      (substitui as notas ad hoc);
    - apoio sem bola: 3–4 companheiros próximos escolhem posição com linha
      de passe aberta;
    - drible 1×1 como duelo no `ActionResolver`;
    - tabela (one-two).
  - **Fora de (c1), vai para (d):** máquinas de estado completas por
    papel, overlap, cruzamento/cabeceio, gatilhos de pressão, contra-ataque.
  - **(c2) — calibração** contra o real, só com (c1) em pé. A meta de (c1)
    é o modelo fazer sentido, não os números fecharem. O teto de 1.303
    passes / 64% do modelo antigo não é referência.
  - **Orçamento:** se qualquer passo passar de 45 ms no CI, PARAR e trazer
    o perfil. "Quase dentro" não vale.
  - **Paridade:** a referência bit a bit será **regenerada de propósito**
    em (c1): a partir do domínio realista, o comportamento muda por
    decisão. Cada regeneração entra no commit que muda o comportamento,
    com o motivo na mensagem.
- **Ordem aprovada de (c1):** 1. base comum → 2. tabela xT + moeda comum
  → 3. linha de impedimento → 4. estado `Run` → 5. passe em profundidade →
  6. apoio sem bola → 7. drible 1×1 → 8. tabela. Os itens 1 e 2 são commits
  separados, cada um com paridade verificada, para poder reverter só o 2.
- **(c1) item 1 — base comum `[ALTERADO v2.1]`:**
  - **xG geométrico** (`xg.rs`): logística no ângulo da boca do gol (via
    `atan2` do libm) e na distância. Coeficientes ajustados por âncoras
    centrais na faixa dos modelos públicos (6 m ≈ 0,40; 11 m ≈ 0,17;
    20 m ≈ 0,05; 25 m ≈ 0,03); revisão em (c2). Fator do finalizador
    `0,6 + 0,8 × habilidade` (neutro em 0,5).
  - **Bloqueio por cobertura:** fração do ângulo da boca do gol coberta por
    corpos adversários de linha entre o chutador e o gol (meia-largura
    0,5 m; união de intervalos). O goleiro já está no xG geométrico. A
    nota binária "defensor a menos de 1 m da linha × 0,3" foi removida.
  - **Decisão de chute:** chuta quando o xG estimado (geometria ×
    finalizador × parte descoberta) ≥ `shoot_xg_min` (0,06). Valor próprio,
    sem competir com as notas ad hoc de passe/condução; o item 2 troca isso
    pela moeda comum.
  - **Saída forçada não chuta mais:** passa para a melhor opção ou, sem
    passe, faz `Clear` (bola longa ~45 m para a frente, `Loose`).
  - **Desfecho do chute ligado ao xG:** P(gol) = xG sem bloqueio ×
    goleiro (`1,2 − 0,4 × gk`) × pressão (−15%). O sorteio no alvo
    continua igual; P(defesa) é a que faz no alvo × não defendido = P(gol).
    Bloqueio não entra aqui: é físico (o chute é interceptado em voo). A
    decisão antecipa o bloqueio, a física o executa — sem dupla contagem.
    Ordem dos sorteios inalterada.
  - **Domínio realista:** `control.base` 0,82 + 0,16 × toque (receptor
    típico ~90%, bom ~95%), pressão a menos de `pressure_radius` −6%,
    bola alta −15%, rápida e interceptação como antes.
  - **Paridade:** o golden é regenerado neste commit, de propósito.
  - **Medido (30 partidas, só para registro; não é meta de (c1)):** 25
    chutes (7,9 no alvo), 3,7 gols, 4.100 passes (71%), 70 botes. Os chutes
    agora vêm da decisão (5% depois de 5,5 s com a bola, todos por valor
    próprio, nenhum forçado); 72% de menos de 11 m, porque o portador ainda
    conduz até perto do gol antes de chutar. Os passes subiram porque o
    domínio deixou de devolver a bola.
- **(c1) item 2 — tabela xT + moeda comum `[ALTERADO v2.1]`:**
  - **Moeda:** toda opção do portador vale "probabilidade de esta posse
    terminar em gol". Chute = xG estimado (item 1). Passe, condução e
    segurar = P(manter) × xT(depois) − P(perder) × xT do adversário onde a
    bola se perde. O portador age se a melhor opção supera segurar por
    `act_margin` (0,002).
  - **xT** (`value.rs`): grade 12×8 de Karun Singh, valores publicados
    copiados sem recálculo (origem e conversão no item "Tabela xT" abaixo),
    interpolação bilinear entre centros de células.
  - **Sucesso do passe (estimado, sem RNG):** sobrevivência a cada
    adversário × precisão (distância e habilidade) × domínio do receptor
    (mesma fórmula do item 1). Um adversário corta se o alcance dele quando
    a bola passa (`0,8 m + velocidade × (t − 0,25 s)`, bola a ~14 m/s)
    cobre a distância até a linha. Quem está colado no passador não reage a
    tempo; quem está longe na linha tem tempo. Substitui "abertura < 4 m".
  - **Condução:** melhor de três direções (frente e diagonais 45°). No
    espaço mantém 97%; contra marcador, `0,45 + 0,4 × drible` (o item 7
    troca isso pelo duelo 1×1).
  - **Segurar:** vale `1 − 0,03 × ticks com a bola` do xT atual (posse
    parada perde valor enquanto a defesa se organiza; zero em ~3,3 s). Uma
    erosão fixa por decisão foi testada e falhou (o portador segurava até a
    saída forçada).
  - **Removidas** as notas ad hoc de passe e condução (base, progresso,
    abertura, comprimento, linha estreita, passe ao goleiro, decaimento da
    condução, fator perto do gol) e o limiar fixo de xG do item 1.
  - **Saída forçada** continua como trava de impasse (55 ticks): melhor
    passe pelo valor ou `Clear`.
  - **Otimização exata:** o valor de um passe nunca passa do xT do destino,
    então destinos cujo xT não supera o melhor valor atual são descartados
    antes da estimativa cara; a cobertura do gol só é calculada se o chute
    descoberto pode vencer. Um `debug_assert` refaz a busca completa e
    compara bit a bit em todas as partidas dos testes em debug.
  - **Paridade:** golden regenerado neste commit, de propósito.
  - **Medido (30 partidas; não é meta):** 4,5 gols, 21 chutes (7,5 no
    alvo), 1.630 passes (84%), 22 botes. Passes saem sobretudo entre 1 e
    2,5 s com a bola; 16–19% ainda pela saída forçada.
  - **Custo:** +32% de instruções sobre o item 1 (callgrind, uma partida:
    575M → 760M). O perfil aponta `best_pass` (38%: estimativa de sucesso
    para até 10 destinos em todo tick com a bola, que agora é 71% do jogo)
    e a interpolação xT (~9–16%). Acima do orçamento; resolvido pela
    cadência de decisão abaixo.
  - **Cadência de decisão `[ALTERADO v2.1]`:** o portador reavalia as
    opções a cada `decision_cadence_ticks` (3 ticks ≈ 0,3 s) e, entre uma
    avaliação e outra, mantém o plano (`carrier_plan`: segurar, ou conduzir
    até o mesmo alvo). **Exceções, reavaliação imediata:** no tick em que
    recebeu a bola, sempre que há adversário a menos de `pressure_radius`,
    e ao atingir a saída forçada. Teste: `test_redecide_on_pressure_and_receive`.
    - Decisões completas por partida (média de 30): **40.299 → 17.334**
      (−57%). O contador só existe com a feature `diagnostics` (fora do
      build do jogo).
    - **Escolha do valor (instruções, seed 2026, e comportamento):**
      cadência 2 = 583,1M (+2,9% sobre o item 1: estoura a regra de
      1,5%); 3 = 540,5M; 4 = 512,9M, mas quantiza o momento do passe (a
      faixa de 1,5–2 s com a bola cai de 43% para 16% dos passes) e corta
      ~27% dos botes. Fixado em **3**.
    - **Por que 3 e não 4 (comportamento, 180 partidas cada):** a erosão
      do valor de segurar faz o passe vencer por volta de 1,5–2 s com a
      bola. Com cadência 3, as avaliações caem nos ticks 16 e 19, dentro
      da janela; com 4, em 17 e 21, e a de 21 já está fora. Medido: passes
      soltos entre 1,5 e 2 s = 44% (sem cadência), 44% (3), **18% (4)**;
      depois de 2 s = 30%, 29%, **56%**. Com 4 a posse contínua sobe 13%
      sobre o jogo sem cadência (3: +6,5%) e os botes caem 21%. A
      economia de 4 (−5% de instruções) viria de um artefato de
      quantização, não de jogo.
    - **A cadência é parâmetro de comportamento, não só de performance.**
      Ela interage com a erosão do valor de segurar (quando o passe vence)
      e com qualquer opção nova da moeda comum. Em (c2), com o passe em
      profundidade em pé, a equação muda e a cadência pode precisar ser
      reajustada; o critério é o comportamento (distribuição do momento do
      passe), não só o custo.
    - **Jogo antes/depois da cadência 3** (180 partidas cada, média ± erro
      padrão sobre 6 blocos de 30):

      | Métrica | Sem cadência | Cadência 3 | Leitura | Real (aprox.) |
      |---|---|---|---|---|
      | Gols | 4,60 ± 0,22 | 4,23 ± 0,12 | −8%, dentro do ruído | 2,7 |
      | Chutes (no alvo) | 23,3 (8,2) | 21,7 (7,7) | −7%, ~1,4 EP | 25 (9) |
      | Passes | 1.647 ± 12 | 1.541 ± 7 | **−6%, real** | ~900 |
      | Acerto de passe | 84,0% | 83,5% | igual | ~80% |
      | Posse (mandante) | 52,5% | 50,8% | igual | — |
      | Posse contínua (tempo com a bola) | 19,9 s | 21,2 s | **+6,5%, real** | não verificado |
      | xG por chute | 0,37 | 0,37 | igual | ~0,10 |
      | xG por partida | 8,3 | 7,7 | −6,5%, ~1,1 EP | ~2,7 |
      | Botes / faltas | 22,3 / 7,3 | 19,7 / 6,5 | −12% / −11%, ~2 EP | ~70 / 22 |

      A cadência deixa a posse ~1,3 s mais longa por sequência: menos
      passes e menos disputas. Nada se afasta do real; a distância até o
      real (passes 1,7×, gols 1,6×, xG 3×, xG por chute 3,7×) é do
      modelo e é tema de (c2) e dos itens 5–7, não da cadência.
    - **Hipótese testada e descartada:** a queda de gols viria do portador
      perder a "janela de chute" entre avaliações. Uma exceção
      experimental (reavaliar todo tick dentro do raio de chute) recuperou
      só ~0,2 gol (4,04 → 4,22 em 60 partidas), dentro do ruído. Não
      adotada.
    - **Queda de gols com a cadência: efeito observado, causa não
      confirmada, candidato para (c2).** −17% em 60 partidas, −8% em 180
      (dentro do ruído). A hipótese da janela de chute continua possível
      com outra variante (reavaliar **ao entrar** no raio de chute, não em
      todo tick), ainda não testada.
    - A cadência é parte do tick lógico, igual em Full, Reduced e Abstract:
      a paridade entre LODs se mantém. Ela muda o comportamento, então o
      golden é **regenerado** (esperado em (c1)).
  - **Cache de xT das 22 posições: avaliado e não implementado.** Contagem
    real: 20 consultas de xT por decisão (6 da condução em posições novas,
    que nenhum cache cobre; 2 do segurar; ~12 dos passes, já reduzidas pela
    poda). Pré-calcular as 22 posições nos dois sentidos custaria 44
    consultas, e só as 11 do próprio time, 22: ambas **acima** das ~14 que
    substituiriam. Reabrir se o número de consultas por decisão crescer
    (itens 5 e 6).
  - **Custo final do item 2 (instruções, seed 2026):** 540,5M, **−4,6%**
    sobre o item 1 (566,6M) e −0,5% sobre o estado anterior a (c1)
    (543,4M). O CI mediu o mesmo número que a máquina local (539.461.992
    no commit `5b362ed`): a contagem não depende do runner.
- **(c1) item 3 — linha de impedimento como dado do tick `[ALTERADO v2.1]`:**
  - `TickFrame::compute_offside` + `offside_line(lado_atacante)`: `x` do
    penúltimo jogador ativo do time que defende, contado da própria linha
    de fundo (goleiro conta; expulsos não; com menos de dois, a linha de
    fundo). Só para o lado com a posse (o único com corredores); `None`
    para o outro e com bola solta.
  - **Desvio do "uma vez por tick":** passa a ser **no máximo uma vez por
    tick, e só nos ticks que usam a linha** (o motor chama
    `compute_offside` depois de `observe_ball` apenas quando há corredor
    em `Run`, item 4). Motivo, medido: calcular em todo tick custava
    +1,82% de instruções (~185 por tick) sem nenhum consumidor; a regra
    de 1,5% estourava num item sem comportamento. O custo real passa a
    aparecer no item 4, onde está o uso.
  - Sem mudança de comportamento: golden inalterado; instruções 540,2M
    (−0,06%).
  - Testes: penúltimo defensor com goleiro na linha, goleiro adiantado
    virando a linha, expulso não conta, linha só para quem tem a posse.
- **(c1) item 4 — estado `Run`: estimativa de custo antes do código.**
  Orçamento: +1,5% sobre o item 3 (540,2M → ≤ 548,3M, ~8,1M instruções).
  - **Dívida do item 3 (`compute_offside`):** medido +1,82% (~9,8M) se
    calculado em todo tick com posse (~73% dos ticks). **Isso sozinho não
    cabe.** Desenho para caber: os corredores escolhem o alvo da corrida
    na mesma cadência do portador (a cada 3 ticks, todos no mesmo tick),
    e a linha só é calculada nesses ticks, se houver atacante elegível.
    Teto: 1/3 → **≤ +0,61%**; esperado ~+0,4%.
  - **Lógica do `Run`:** seguir o alvo em todo tick (≤ 3 corredores, ~20
    instruções cada) ≈ +0,4%; escolher o alvo nos ticks de cadência
    (procura do vão entre defensores) ≈ +0,3–0,7%.
  - **Efeito indireto:** corridas mudam o jogo (mais passes adiante, posse
    diferente), o que muda o custo de `best_pass` e da interceptação em
    qualquer direção. Não estimável antes de medir.
  - **Total estimado: +1,1% a +1,7%, no limite.** Se a medição passar de
    1,5%, PARAR e trazer o perfil.
- **(c1) item 4 — estado `Run` (implementado) `[ALTERADO v2.1]`:**
  - `runs.rs`: atacantes (`Striker`) e pontas (`Winger`) do time com a
    posse, nas fases `InPossession`/`TransitionAttack`, com um companheiro
    na bola. Nos ticks de cadência (a cada 3), quem está elegível (sem
    corrida e fora do intervalo) e tem ≥ 4 m até a linha corre até 0,5 m
    **aquém** da linha de impedimento, no vão lateral mais aberto (5
    posições em ±8 m, defensores a até 10 m da linha). Dura 2,5 s;
    intervalo `60 − 40 × off_the_ball` ticks. Sem RNG.
  - A linha é calculada só nesses ticks e só se houver alguém elegível.
  - A corrida vale enquanto a fase do time for de posse (inclusive com a
    bola em voo, quando a posse fica "solta" mas a fase se mantém) e
    sobrepõe a âncora no `plan_shape`. O portador e o receptor de passe
    continuam sendo comandados por `on_ball`.
  - **Custo medido: 535,6M, −0,85% sobre o item 3.** O custo direto
    (linha + corridas) foi mais que compensado pelo efeito no jogo.
  - **Jogo (180 partidas, contra o item 3):** gols 4,57 (antes 4,23, ~1,2
    EP: ruído); passes 1.616 (+5%); acerto 85,7% (+2 pontos); posse
    contínua 23,8 s. Corredor ativo em 62% dos ticks; **2.139 corridas por
    partida** (real: dezenas). **A frequência é problema de modelo, não
    constante para (c2)** (corrigido em 2026-10-03; ver "Passo 2 do
    Caminho A, commit 1"): ela sai de "tempo de posse ÷ intervalo", sem
    decisão de correr. Corredor além da linha em 3,4% dos ticks de corrida
    (o viés conhecido, sem apito).
  - **Efeito grande:** a posse do mandante foi de 50,8% para **63,0%**. O
    mandante da demo joga 4-4-2 (2 corredores); o visitante, 4-3-3 (3
    corredores, com as duas pontas). Mais corredores = jogo mais direto
    e menos posse. Plausível no futebol real, mas a intensidade é
    estrutural: tratada no passo 2 do Caminho A (gatilho + teto de
    corredores), não em (c2).
  - Testes: corre até a linha e nunca além; mira o vão aberto; não corre
    sem posse nem sem espaço; só atacantes e pontas.
  - **Retorno à forma:** quando a corrida acaba, o `plan_shape` volta a
    mandar o jogador para a âncora (a corrida só sobrepõe a âncora enquanto
    `run_until > tick`). Medido numa partida completa (teste
    `runners_return_to_shape_after_a_run`): 2.154 corridas terminadas; de
    volta a menos de 5 m da âncora em **1,7 s na mediana e 2,7 s no p90**;
    distância média dos corredores à âncora por janela de 15 min = 8,3 /
    7,7 / 7,7 / 8,0 / 7,9 / 8,1 m, sem deriva. O teste exige mediana ≤ 3 s,
    p90 ≤ 6 s e a última janela ≤ 1,5 × a primeira + 2 m.
  - Golden de paridade regenerado (comportamento novo).
- **(c1) item 5 — passe em profundidade: implementado, DESLIGADO, item
  pausado (2026-10-02).**
  - Código: `Action::ThroughPass { to, target }`; alvo = `run_target` do
    corredor + 8 m além da linha, no corredor dele; sucesso = sobrevivência
    na linha de passe × precisão × corrida até a bola (corredor contra o
    adversário mais rápido, inclusive o goleiro) × domínio em movimento;
    valor na moeda comum. Execução: `resolve_pass_to` (o passe normal virou
    um caso dele, bit a bit igual), bola chega a 0,5 m/s ao ponto.
  - Chaves no `ValueTuning`, **ambas `false`**: `through_balls` e
    `lofted_lane`. Desligado, o jogo e o golden são os do item 4
    (535,9M, +0,05%). Os testes ligam as chaves.
  - **Por que pausado (medido, 30–60 partidas):**
    1. Ligado como desenhado: **0,2 passe em profundidade por partida**
       (real: ~5–15) e **+3,12% de instruções**, só para procurar.
       Diagnóstico: a sobrevivência estimada na linha de passe tem mediana
       **0,04**, porque esses passes são longos (mediana 42 m) e o modelo
       de alcance trata todo passe como rasteiro a ~14 m/s.
    2. **Inconsistência encontrada (afeta também o item 2):** acima de
       28 m o resolver joga **pelo alto**, mas a estimativa de sucesso não
       sabe disso; passes longos normais também são subestimados. Correção
       (`lofted_lane`): no passe pelo alto só contam adversários a até 2 m
       da saída ou 5 m da queda.
    3. Com a correção: **134 passes em profundidade por partida**, gols
       6,2–7,8, acerto de passe 65%, **+5,1% de instruções**. O modelo é
       sensível demais às estimativas de sucesso (corrida até a bola com
       posições paradas, sem a inércia de quem já corre nem o giro do
       defensor).
    4. Testados sem efeito suficiente: só avaliar corredores a ≤ 3 m do
       ponto legal (E1: 0,1 por partida) e +0,5 s de giro para defensores
       de linha (E1+E2: 0,4–0,5 por partida).
  - **Retomado (2026-10-02), em três passos com commit e medida cada:**
    1. ligar só `lofted_lane`; 2. redesenhar a disputa pela bola; 3.
    passe em profundidade só para corredores perto da linha. Meta: 5–15
    passes em profundidade por partida, ≤ +1,5% de instruções sobre o
    item 4 por passo.
  - **Passo 1 — `lofted_lane` ligado:** 532,6M instruções (−0,56%). Jogo
    (180 partidas, contra o item 4): passes 1.616 → **1.401** (−13%),
    acerto 85,7% → **79,2%** (mais passes longos, menos precisos; real
    ~80%), gols 4,57 → 5,21 (+14%, ~2 EP), posse contínua 23,8 → 15,9 s
    (jogo mais direto), xG por partida 7,9 → 8,6. **Posse do mandante
    63% → 75%:** a assimetria 4-4-2 × 4-3-3 cresce de novo; para (c2)
    (separar formação de qualidade de elenco). Golden regenerado.
  - **Passo 2 (reenquadrado): tempo de chegada + restrição a corredor em
    posição legal — um passo só.** O "passo 2" original (só a disputa pela
    bola, sem restringir quem pode receber) estava mal definido: 36 passes
    em profundidade por partida não é meio caminho, é defeito. A mecânica
    é uma só:
    - **tempo de chegada:** bola (rasteira: solução exata do rolamento;
      alta: tempo de voo), corredor embalado sem reação, defensor de linha
      com reação + 0,5 s de giro se a bola vai às costas, goleiro sem giro
      e com alcance das mãos; quem chega antes espera a bola;
    - **só corredores a ≤ 3 m do ponto legal** podem receber.
    - Medido ligado: 542,1M (+1,22% sobre o item 4) e **13,3 ± 0,6 passes
      em profundidade por partida** (meta 5–15).
  - **Item 5 NÃO fechado: acerto dos passes em profundidade ≈ 0,3%**
    (outros passes: 79%), medido com contador exato (`diagnostics`). Quase
    todos falham. Causa provável: a estimativa supõe inércia (corredor
    embalado) e giro do defensor, mas a **cinemática do motor não tem
    nenhum dos dois** (velocidade máxima instantânea, sem reação). É a
    mesma classe de erro do passo 1: **a estimativa tem que usar a mesma
    física da execução.** O código fica commitado com `through_balls =
    false` (comportamento e golden do passo 1).
  - **Regra derivada (vale para os itens seguintes):** toda estimativa da
    decisão usa a cinemática e as regras que o motor de fato executa; se o
    modelo precisar de inércia ou reação, elas entram primeiro na
    cinemática (mudança de modelo, decisão do usuário), depois na
    estimativa.
  - **Status: item 5 PENDENTE** até a física nova entrar. Código
    commitado desligado (`5ce4218`), reavaliado no passo 4 da nova ordem.
- **Assimetria de posse — causa isolada (2026-10-02, 60 partidas cada):**

  | Experimento | Posse do mandante |
  |---|---|
  | 4-4-2 (casa) × 4-3-3 | 73–74% |
  | 4-3-3 (casa) × 4-4-2 | 26–28% (o 4-4-2 segue com ~73%) |
  | 4-4-2 × 4-4-2 | 49–52% |
  | 4-3-3 × 4-3-3 | 49–50% |
  | 4-4-2 × 4-3-3 sem corridas e sem passe em profundidade | 50–53% |

  Não é lado nem elenco: é a **formação somada às corridas**. O 4-3-3 tem 3
  corredores (atacante + 2 pontas) que ficam correndo para a linha em ~62%
  dos ticks; o portador fica sem apoio curto e os passes longos para quem
  está na linha falham. O 4-4-2 corre só com 2 atacantes, e os meias de
  lado ficam. **É estrutural** (frequência e equilíbrio corrida × apoio),
  não calibração: vira item de (c1), ligado ao item 6 (apoio sem bola).
- **Gols contra o real, não contra o passo anterior (regra para (c2)):**
  real ~2,7. Item 4: 4,6; passo 1: 5,2; sem corridas: 3,6. Os gols
  sobem porque cada passo de realismo acrescentou **ataque** (chute por
  xG, corridas até a linha sem apito de impedimento, passe pelo alto por
  cima da defesa) sem a contraparte **defensiva**: ninguém acompanha o
  corredor (a defesa só contém o portador e cobre atrás dele), não há
  linha de impedimento ativa nem goleiro saindo do gol. Em (c2), toda
  métrica é comparada com o real; o passo anterior é linha de base móvel.
- **Nova ordem de (c1) — Caminho A (aprovado 2026-10-02):**
  1. reação e aceleração na movimentação (física nova);
  2. defesa acompanha corredores + equilíbrio corrida × apoio (corrige a
     assimetria de posse) + **goleiro saindo do gol**;
  3. item 9: construção desde a defesa (passe para trás como opção que
     preserva a posse);
  4. refazer o item 5 em cima da física nova;
  5. itens 6–8.
  Em todos: comparar métricas com o **real**, nunca com o passo anterior.
- **Física de movimento (passo 1) `[ALTERADO v2.1]`:**
  - **Parâmetros no `TuningParams` (`KinematicsTuning`):** `max_accel`
    (m/s²), `reaction_ms` (ms), `reaction_threshold` (m), `turn_rate`
    (rad/s).
  - **Trajetória em três fases, em forma fechada** (planejada a cada tick
    em `move_players`):
    1. **reação:** se o alvo mudou mais que `reaction_threshold` desde o
       plano anterior, o jogador segue com a velocidade que tinha por
       `reaction_ms` (uma reação em andamento não recomeça a cada tick);
    2. **aceleração:** aceleração constante `max_accel` da velocidade
       atual até a desejada (rumo ao alvo, na velocidade pedida);
    3. **cruzeiro:** linha reta até o alvo na velocidade pedida, parando
       nele (sem fase de frenagem nesta versão).
  - **Giro:** a direção desejada só pode girar `turn_rate × Δt` a partir
    da direção em que o jogador já se move (Δt = tempo desde o plano
    anterior); parado (< 0,5 m/s), vira para qualquer lado.
  - Ângulos via `fm_core::math` (libm). `pos_at` continua função pura de
    `(trajetória, t)`: a paridade entre LODs se mantém.
  - **Dois commits:** (1) estrutura com `max_accel = ∞`, `reaction_ms =
    0`, `turn_rate = ∞` — as fases 1 e 2 têm duração zero e a fase 3 é a
    fórmula antiga com as mesmas operações: **golden idêntico**; (2)
    valores realistas: golden regenerado.
  - **Medição do commit 1 (2026-10-02) — PARADO, não commitado:**
    - Estrutura com física desligada (`Lead` opcional na `Trajectory`,
      atalho decidido uma vez por tick): golden **idêntico**, testes ok,
      mas **550,3M = +2,74% sobre o item 4** (+3,3% sobre o passo 1 do
      item 5). Perfil: ~2,4M no teste da fase em `pos_at` (1,2 milhão de
      chamadas por partida); o resto (~13M) difuso no corpo inlinado do
      `tick_logic` (trajetória maior, inlining diferente). Duas versões
      anteriores mediram +5,7% e +3,8%.
    - **Protótipo do commit 2** (aceleração 4,5 m/s², reação 200 ms acima
      de 2 m, giro 6 rad/s): **o jogo quebra** — 3 passes por partida,
      bola parada 81–91% do tempo. Sem desaceleração, e com giro limitado,
      o jogador não para sobre um ponto (passa ou orbita o alvo); quem bate
      o reinício nunca chega a < 1 m da bola. Falta um quarto elemento:
      **chegada** (frear até parar no alvo; girar parado).
    - Código guardado fora da branch (stash + patch), aguardando decisão.
  - **Contexto para (c2) e para futuras reescritas da cinemática: o motor
    só funciona porque é arcade.** Até aqui todo jogador muda de velocidade
    e de direção instantaneamente (sem inércia, reação, giro ou frenagem).
    Partes inteiras do motor dependem disso sem dizer:
    - reinícios exigem que o batedor chegue a < 1 m da bola: com inércia e
      giro limitado ele passa do ponto ou orbita, e o reinício nunca sai;
    - a disputa pela bola, a contenção e a cobertura supõem que o defensor
      se reposiciona na hora;
    - as estimativas da decisão (invariante 18) foram calibradas contra
      essa execução instantânea.
    Qualquer física realista expõe esses pontos de uma vez (medido: 3
    passes por partida, 81–91% de bola parada). Toda mudança na
    cinemática deve ser medida contra esse risco, não só contra custo.
- **Commit 1 da física (estrutura desligada):** a `Trajectory` volta ao
  formato original; a física (`Lead`: reação, aceleração, frenagem de
  chegada) fica num campo à parte do jogador, `None` no modelo
  instantâneo, e o caminho de posição sem `Lead` é o código antigo. Golden
  **idêntico**; **537,6M = +0,39% sobre o item 4** (três tentativas
  anteriores com a física dentro da trajetória: +5,7%, +3,8%, +2,74%).
- **Commit 2 da física (ligada, 15 m/s² / 80 ms / 15 rad/s) — PARADO,
  não commitado (2026-10-02):**
  - **O jogo roda:** bola parada 9%, 870 passes por partida, posse do
    mandante 56% (a assimetria quase some), gols 5,5–5,9, acerto 61%.
  - **Custo: 831M = +55% sobre o item 4** (teto 562M). Perfil: o
    replanejamento físico (`steer`) dos 22 jogadores em todo tick (1,2
    milhão de vezes por partida, ~200–320 instruções cada) soma +245M no
    `tick_logic`; a posição com frenagem, +32M no `capture`; mais bolas em
    voo, +17M em interceptação/recepção.
  - **Experimento — manter o plano enquanto a intenção não muda** (alvo a
    < 0,5 m e mesma velocidade, e o plano termina no alvo): mantém o plano
    em 59,5% das chamadas, o jogo segue rodando (bola parada 9–10%, 865
    passes), mas o custo fica em **734,6M (+37%)**. Uma primeira versão,
    que mantinha planos que paravam aquém do alvo, travava os reinícios
    (93% de bola parada).
  - Conclusão: com replanejamento quase todo tick, a física completa não
    cabe em +5%. Decisão pendente com o usuário.
- **Física só para quem está no lance (aprovado 2026-10-02):**
  - **No lance (física em todo tick):** o portador; os defensores que o
    `on_ball` comanda contra o portador (contenção, cobertura, quem dá o
    bote); quem persegue uma bola em voo e o receptor de um passe; o
    batedor de bola parada; os corredores com corrida ativa (e, quando o
    passo 2 existir, quem acompanha um corredor).
  - **Fora do lance (arcade):** o resto — quem só segue a âncora da
    formação (defesa recuada, meias longe da bola, goleiro fora do lance).
    Modelo instantâneo, replanejado na **cadência de 3 ticks**.
  - **Regra de transição:** todo replanejamento parte da posição e da
    velocidade atuais do plano em curso, qualquer que seja o modelo. Ao
    entrar no lance, a física começa da velocidade que o jogador já tinha
    (a do seu plano arcade); ao sair, o arcade parte da posição atual.
    Posição sempre contínua; velocidade contínua na entrada no lance.
    Teste: `test_player_entering_play_does_not_jump`.
  - **Manter o plano** enquanto a intenção não muda (alvo a < 0,5 m, mesma
    velocidade, e o plano termina no alvo).
  - Faixas de custo: ≤ 562M aceito; 562–593M aceito como exceção pontual
    se (c) fechar ≤ 593M; > 593M parar com o perfil.
  - **Medido (2026-10-02), não commitado:** **588,9M = +9,96% sobre o
    item 4** (faixa de exceção). Critério de funcionamento: bola parada
    5–9%, 950 passes por partida. Contra o real (180 partidas): gols 4,86
    (2,7), chutes 27,5 / 9,0 no alvo (25 / 9 ✓), passes 950 (~900 ✓),
    **acerto 56% (~80% ✗)**, **botes 8,8 e faltas 3,3 (~70 / 22 ✗)**, posse
    do mandante 56,9%, xG por chute 0,23 (~0,10). Corredores voltam à
    forma em 1,1 s (mediana).
  - **Teste da Fase 4 falha:** `match_statistics_are_plausible` exige ≥ 3
    faltas por partida; mediu 2,83 (seeds 0–5). Parado pela regra "teste
    falhou → log + hipóteses, sem correção tentativa".
- **Invariante 18 do lado defensivo (achado 2026-10-02):** o bote exigia
  um defensor já a ≤ 1,8 m (`tackle_range`), mas a contenção o posiciona a
  1–4 m do portador. No arcade a distância "acontecia" (29% dos ticks com
  bola dominada havia alguém ao alcance); com física, o defensor para no
  ponto de contenção e fica fora (9%). A decisão supunha uma movimentação
  que a execução não tem — o mesmo erro do passe em profundidade, agora
  na defesa.
- **Commit 2 da física = física no lance + passo 2 (defesa ajustada),
  num commit só (decisão do usuário, opção (a)):**
  - **Engajamento:** dentre os defensores recuperados a até
    `engage_range` (4,5 m) do portador, o de maior nota de bote acima do
    limiar parte para cima dele a toda velocidade (alvo = portador, no
    lance). O bote se resolve só quando ele de fato chega a
    `tackle_range`. A física decide quando o bote acontece.
  - Meta: passar `match_statistics_are_plausible` (≥ 3 faltas). Não se
    persegue o real; os números saem do mecanismo.
  - **Custo: 596,3M = +10,91% sobre o commit 1 (537,6M).** Orçamento
    pontual deste commit: até ~607M. **Dívida explícita:** passa do teto
    de saída de (c) (593M ≈ 45 ms) em 3,3M; precisa ser devolvido antes de
    fechar (c). O próximo item volta à regra de +1,5%.
  - Medido (180 partidas, 4-3-3 e 4-4-2): gols 5,30, chutes 25,8 / 8,4 no
    alvo, passes 929 (acerto 56%), botes 10, faltas 3,6, bola parada 7%,
    xG por chute 0,23.
- **Passo 2 do Caminho A, resto — dois commits medidos em separado
  (aprovado 2026-10-03):** (1) equilíbrio corrida × apoio; (2) defesa
  acompanha o corredor. Um commit por push; o commit 2 só começa com o CI
  do commit 1 verde e com duas medidas de 180 partidas na mão: corredores
  ativos por tick (média) e posse do 4-4-2 × 4-3-3 nas duas orientações.
  A regra de +1,5% vale para cada commit.
- **Commit 1 — equilíbrio corrida × apoio: a corrida passa a ser decidida
  `[ALTERADO v2.1]`:**
  - **Classificação: modelo, não constante.** Até aqui todo atacante ou
    ponta elegível corria, e o único freio era o intervalo individual: a
    frequência saía de "tempo de posse ÷ intervalo" (2.139 corridas por
    partida, corredor ativo em 62% dos ticks). É o mesmo defeito do
    `TEAM_TACKLE_GAP` nos 876 desarmes. Reduzir o intervalo esconderia o
    defeito e manteria a assimetria entre formações (3 corredores no
    4-3-3, 2 no 4-4-2).
  - **Teto de corredores simultâneos por time:** `max_runners` (**1**), o
    mesmo para qualquer formação. Enquanto houver uma corrida viva no
    time, ninguém mais começa outra. Se a posse do 4-4-2 × 4-3-3 não
    chegar a ~50% com 1, o teto sobe para 2.
  - **Gatilho — o portador pode servir a corrida:** uma corrida só começa
    se (a) o portador não está pressionado (nenhum adversário a menos de
    `pressure_radius`) e (b) o ponto da corrida fica ao alcance de passe
    do portador (entre `pass_min_dist` e `pass_max_dist`).
  - **Quem corre:** dentre os elegíveis (papel de corredor, sem corrida,
    fora do intervalo, com espaço até a linha, ao alcance do portador),
    quem tem o **vão lateral mais aberto** (a mesma medida que já escolhe
    a faixa da corrida); no empate, o menor índice. Sem RNG.
  - **Quem não corre fica na âncora:** é o apoio curto que existe hoje. O
    posicionamento ativo de apoio continua sendo o item 6.
  - **O intervalo individual** (`60 − 40 × off_the_ball` ticks) continua,
    só como recuperação física; não é mais ele que dita a frequência.
  - Linha de impedimento: continua calculada só nos ticks de cadência em
    que alguém pode usá-la (e agora só se o teto e o gatilho permitem).
  - Golden de paridade regenerado (comportamento novo).
  - **Custo (CI, run #55): 598.838.544 instruções, +0,43% sobre
    596,3M.** Dentro do +1,5%; a dívida ao teto de (c) (593M) vai de 3,3M
    para 5,8M.
  - **Medido (2026-10-03, 180 partidas por linha, `calibrate` com
    `FM_MATCHES=180`):**

    | Configuração | Corridas/partida | Corredores por tick | Ticks com corredor | Posse do 4-4-2 (casa / fora) |
    |---|---|---|---|---|
    | Antes (`5d34253`) | 1.984 | 0,859 | 48,8% | 56,8% / 57,4% |
    | Teto 1 (este commit) | 1.138 | 0,493 | 49,3% | 57,7% / 58,1% |
    | Teto 2 | 1.338 | 0,584 | 42,1% | 57,0% / 57,5% |
    | Teto 0 (sem corridas) | 0 | 0 | 0% | 54,3% / 54,3% |

    Controles com teto 1: 4-4-2 × 4-4-2 = 49,9%; 4-3-3 × 4-3-3 = 49,7%.
  - **O teto faz o que devia no custo, não na posse.** Os corredores por
    tick caem 43% (0,86 → 0,49), mas a posse do 4-4-2 × 4-3-3 **não se
    move** com o teto (56,8 → 57,7%), e subir para 2 também não (57,0%).
  - **A causa da assimetria mudou com a física.** No arcade, tirar as
    corridas levava a posse a 50–53% (tabela "Assimetria de posse"). Com a
    física no lance, **sem corrida nenhuma o 4-4-2 ainda fica com 54,3%**
    nas duas orientações: ~4 dos ~8 pontos vêm da formação em si, não das
    corridas. As corridas explicam os outros ~3,5 pontos, e esses não
    dependem de quantos correm ao mesmo tempo (teto 1, 2 ou sem teto dão
    57–58%). Decisão pendente com o usuário.
  - **O gatilho quase não segura:** os ticks com algum corredor ficam em
    49% (antes 48,8%). "Portador sem pressão + alvo ao alcance de passe" é
    verdade quase sempre; quem limita é o teto.
  - Outros números com teto 1 (não são meta): gols 4,87, chutes 28,2 /
    9,1 no alvo, passes 928 (acerto 58%), botes 8–9, faltas 3,2, bola
    parada 7–8%. O 4-3-3 × 4-3-3 joga muito diferente do 4-4-2 × 4-4-2
    (7,2 gols e 41 chutes contra 4,6 e 25; acerto 52% contra 65%).
- **Commit 2 — defesa acompanha o corredor (desenho; só depois da medida
  do commit 1):** no tick em que a corrida nasce, o defensor de linha mais
  próximo do corredor vira o marcador dele; enquanto a corrida vive, o
  alvo do marcador é um ponto do lado do gol em relação à **posição
  atual** do corredor (não o alvo da corrida: o defensor não lê a intenção
  do adversário) e ele entra no lance (física). Contenção, cobertura e
  bote têm prioridade. Sem estimativa nova: o marcador perto do receptor
  já entra na sobrevivência da linha de passe e na pressão sobre o
  domínio (invariante 18). Riscos a medir: custo (cada marcador na física
  custa 200–320 instruções por tick) e a linha defensiva recuando atrás
  do corredor.
- **Critério de saída de (c): ≤ 48 ms (decisão consciente, 2026-10-03)
  `[ALTERADO v2.1]`:** a meta de 45 ms (~593M instruções pela régua)
  continua como alvo desejável, mas (c) fecha com **≤ 48 ms (~633M)**.
  Não é relaxamento: a física só para quem está no lance é estrutural e
  custa mais do que o orçamento inicial previa (+10,9% num commit só). Pela
  régua, o gate de 50 ms (~660M) nunca foi violado. (O alarme de relógio,
  não bloqueante, lê 38–42 ms ou 64 ms para o mesmo código conforme o
  runner: runs de 2026-10-03.) A regra por commit não muda: +1,5%
  sobre a linha de base; acima disso, parar e trazer o perfil antes de
  otimizar. A meta de `tick_logic` da Seção 0 (40 ms) e o gate (50 ms)
  não mudam.
- **Passo 2 do Caminho A, commit 2 (defesa acompanha o corredor):
  tentado e revertido (2026-10-03).** +2,67% de instruções (estimativa:
  +1,1%), acompanhamento parcial (mediana 4,4 m no fim da corrida; alvo
  1,5 m), posse do 4-4-2 57,7% → 59,7%, linha defensiva 26,8 → 26,0 m.
  Revisitar depois do item 9. O desenho acima fica como registro.
- **Passo 2.5 do Caminho A — goleiro saindo do gol:** defesa (sai para
  interceptar, corta cruzamento, joga como líbero). Depois do item 9 e
  antes de refazer o item 5. Não confundir com "goleiro como receptor"
  (item 9, commit 3).
- **Item 9 (construção desde a defesa) — diagnóstico antes do código
  (2026-10-03, `examples/buildup_stats.rs`, 30 partidas por orientação):**
  - **Quem recebe passe:** zagueiros 0,0–0,1%, laterais 1–3,5%, goleiro
    0%, meias centrais 44–46%. Passes para trás já são 40–45% (vão para
    os meias, não para a defesa). **44–52% dos passes saem na saída
    forçada** (5,5 s); o portador escolhe conduzir em 75% das decisões.
  - **Desenho aprovado (não implementado):** termo de perda do passe por
    proximidade do adversário ao receptor, na moeda única (sem segunda
    moeda); três commits (moeda; apoio dos defensores em `plan_shape`;
    goleiro como receptor, sozinho).
  - **Achado que contradiz a premissa do desenho (invariante 18 do lado
    do passe), por distância do adversário mais próximo ao receptor no
    chute (4-4-2 em casa; a outra orientação dá o mesmo):**

    | Adversário mais próximo | Passes/partida | Receptor domina | Estimativa da decisão | Dos que falham: adversário / companheiro / bola parada |
    |---|---|---|---|---|
    | < 3 m | 25 | 20,8% | 39,8% | 89% / 10% / 1% |
    | 3–6 m | 117 | 38,7% | 65,9% | 90% / 8% / 2% |
    | 6–12 m | 426 | 55,1% | 77,5% | 80% / 8% / 12% |
    | ≥ 12 m | 301 | 77,4% | 82,1% | 53% / 29% / 18% |

    Na execução, um passe para um receptor **sem ninguém a menos de 6 m
    só chega em 55%** das vezes, e a falha vira bola do adversário em
    80–90% dos casos (53% mesmo com o receptor a mais de 12 m de
    qualquer adversário). A estimativa da decisão é otimista em 20–27
    pontos abaixo de 12 m. Um termo de perda que suponha "falha perto de
    receptor livre é recuperada" seria ainda mais otimista que a
    execução. **Parado para decisão do usuário**: o problema está na
    execução do passe (o "acerto 56%" das dívidas), não na moeda.
- **Diagnóstico da execução do passe (time-box: 2 commits de medição,
  aprovado 2026-10-03; o item 9 espera).**
  - **Medição A — por que o passe falha (`examples/pass_failures.rs`, 30
    partidas, 4-4-2 em casa; a outra orientação dá o mesmo):** cada passe é
    seguido do chute até o primeiro toque (ou a bola sair). 872 passes por
    partida, **43,0% falham**.

    | Classe | Por partida | De todos | Das falhas | < 10 m | 10–20 m | 20–28 m | ≥ 28 m (alto) |
    |---|---|---|---|---|---|---|---|
    | Receptor domina | 470 | 53,9% | — | 66,2% | 58,4% | 62,6% | 9,3% |
    | Outro companheiro toca antes | 26 | 3,0% | — | 1,9% | 0,8% | 3,7% | 9,6% |
    | Interceptação em voo | 174 | 19,9% | **46,3%** | 10,3% | 18,4% | 21,6% | 30,9% |
    | Receptor fora do ponto | 113 | 13,0% | **30,2%** | 14,7% | 14,0% | 4,1% | 26,8% |
    | Domínio errado | 49 | 5,7% | 13,2% | 6,5% | 6,3% | 6,5% | 1,2% |
    | Erro de direção | 39 | 4,5% | 10,4% | 0,4% | 2,0% | 1,6% | 22,2% |
    | Passes por partida | 872 | | | 124 | 370 | 257 | 121 |

    Definições: o ponto de mira é onde o receptor estava no chute (é o
    que o resolver mira). *Interceptação*: um adversário toca a bola
    enquanto ela ainda se aproxima do ponto. *Erro de direção*: a bola
    passa do ponto sem nunca chegar a `receiver_radius` (1,5 m) dele.
    *Receptor fora do ponto*: a bola chega a menos de 1,5 m do ponto e
    passa sem o receptor tocar. *Domínio errado*: o receptor toca e não
    controla.
  - **Nenhuma causa passa de 50%.** A maior é a interceptação em voo
    (46%), e ela acontece longe do alvo (a bola ainda estava a 16 m do
    ponto, na média). A segunda é o receptor fora do ponto (30%): a bola
    passa, na média, a 0,7 m de onde ele estava.
  - **O passe pelo alto quase nunca chega:** 9,3% de acerto em 121 passes
    por partida (14% dos passes).
  - Depois da falha a posse vai para o adversário em 88% (interceptação),
    79–80% (fora do ponto, erro de direção) e 66% (domínio errado).
  - **Medição B — como acontece a interceptação em voo (a maior causa;
    174 por partida; mesma ferramenta, 30 partidas, 4-4-2 em casa):**

    | Medida | Distribuição |
    |---|---|
    | Ticks do chute ao toque [1 / 2 / 3–5 / 6–10 / 11+] | 32% / 28% / 13% / 14% / 12% |
    | Fração do passe já percorrida [< 25 / < 50 / < 75 / ≥ 75%] | 63% / 12% / 12% / 13% |
    | Interceptador no chute: distância ao passador [< 2,5 / < 5 / < 10 / ≥ 10 m] | 41% / 31% / 16% / 12% |
    | Interceptador no chute: distância à linha de passe [< 1 / < 2 / < 4 / ≥ 4 m] | 66% / 15% / 14% / 6% |
    | Quanto o interceptador andou até o toque [< 1 / < 3 / < 6 / ≥ 6 m] | 56% / 23% / 12% / 10% |
    | Adversários a < 2 m da linha no chute [0 / 1 / 2 / 3+] — cortados | 11% / 69% / 17% / 3% |
    | Idem — passes que chegam | 59% / 38% / 2% / 0% |

    - **É bloqueio, não antecipação.** 60% dos cortes acontecem em até 2
      ticks (0,2 s) do chute e 63% no primeiro quarto do passe; em 66% o
      interceptador já estava a menos de 1 m da linha, em 72% a menos de
      5 m do passador, e em 56% andou menos de 1 m. O portador passa para
      cima de um defensor parado na linha, ao lado dele.
    - **57% dos passes cortados saem na saída forçada** (dos que chegam,
      47%): o portador é obrigado a soltar a bola aos 5,5 s e a melhor
      opção restante está bloqueada.
    - **A decisão sabia do risco, mas menos do que a execução cobra:**
      sucesso estimado 59,9% para os passes cortados (83,5% para os que
      chegam). Na estimativa, um defensor em cima da linha corta no
      máximo 70% (`pass_intercept_max`); na execução, qualquer adversário
      a menos de `intercept_radius` (0,9 m) da bola toca nela, e o passe
      acaba (domina em 59%, desvia em 41%).
  - **Hipóteses para decisão do usuário (não investigadas além disto):**
    (1) estrutura: a saída forçada produz passes para linhas bloqueadas;
    (2) estimativa × execução (invariante 18): o bloqueio por um corpo na
    linha vale ≤ 70% na decisão e ~100% na execução; (3) o defensor de
    contenção fica, por construção, na linha portador→gol, isto é, em
    cima dos passes para a frente. O "receptor fora do ponto" (30% das
    falhas) e o passe pelo alto (9% de acerto) não foram detalhados.
  - **Time-box estendido (2026-10-03): mais dois commits** — (3)
    calibração do bloqueio na estimativa; (4) medição do receptor fora do
    ponto. O passe pelo alto (9% de acerto) fica como dívida, sem
    investigação agora.
  - **Commit 3 — bloqueio na estimativa do passe `[ALTERADO v2.1]`
    (invariante 18 do lado do passe):**
    - **Execução medida** (passes rasteiros, pelo adversário mais próximo
      da linha no chute): cortado em **83%** com alguém a < 0,45 m da
      linha, **43%** a 0,45–0,9 m, **15%** a 0,9–1,5 m, 7% a 1,5–2,5 m,
      5% além. Antes a estimativa dava ≤ 70% × (1 − distância/0,8 m) para
      quem está perto do passador: ~53%, ~13% e 0% nas três primeiras
      faixas.
    - **Modelo:** a chance de um adversário cortar o passe passa a ser o
      maior de dois termos: o **bloqueio** (corpo perto da linha: 1 em
      cima dela, caindo linearmente a 0 em `block_reach` = 1,35 m, que é
      1,5 × `intercept_radius`) e a **corrida** até a linha (o termo que
      já existia, até `pass_intercept_max`, sem mudança). Vale para o
      passe e para a linha do passe em profundidade.
    - Teste: `a_body_on_the_lane_blocks_the_pass`. Golden de paridade
      regenerado (a decisão muda).
    - **Medido (180 partidas por orientação no `calibrate`; 30 nas
      ferramentas de passe), contra o commit 1 do passo 2:**

      | Métrica | Antes | Commit 3 | Real (aprox.) |
      |---|---|---|---|
      | Passes por partida | 928 | 892 | 900 |
      | Acerto de passe | 58% | 60% | 80% |
      | Passes que falham (`pass_failures`) | 43,0% | 40,0% | ~20% |
      | Interceptações em voo por partida | 174 | 142 | — |
      | Passes com adversário a < 0,45 m da linha | 82 | 40 | — |
      | Saída forçada (passes aos 5,5 s) | 47% | 51% | — |
      | Gols | 4,87 | 4,27 | 2,7 |
      | Chutes (no alvo) | 28,2 (9,1) | 26,7 (8,3) | 25 (9) |
      | Faltas | 3,2 | 2,8 | 22 |
      | Posse do 4-4-2 (casa / fora) | 57,7% / 58,1% | 59,0% / 59,2% | — |

    - **A saída forçada sobe 4 pontos** (47% → 51% no conjunto de 180
      partidas; por time, em 30 partidas: +2,4 a +6,8). O limite combinado
      para parar era +5. O portador deixa de passar para linhas bloqueadas
      e, sem outra opção, chega mais vezes à trava dos 5,5 s: a trava
      continua sendo metade dos passes.
    - Os passes bloqueados que sobram (40 por partida) continuam sendo
      cortados em 77%: são, na maioria, escolhas da saída forçada, que
      aceita qualquer valor.
    - **Custo do commit 3 (CI): 597.652.818 instruções, −0,20%** sobre
      598.838.544.
  - **Commit 4 — medição do "receptor fora do ponto" (112 por partida
    depois do commit 3; `pass_failures`, 30 partidas, 4-4-2 em casa; a
    outra orientação dá o mesmo):**
    - **Hipótese (a), raio de controle pequeno: descartada.** Em 0% dos
      casos o receptor ficou no ponto e a bola passou fora do alcance; em
      2% ele só esteve ao alcance entre dois ticks.
    - **Hipótese (b), o receptor se move: confirmada.** Quando a bola
      chega ao ponto de mira, o receptor está a mais de 3 m dele em 98%
      dos casos e a mais de 6 m em 65%. Mecanismos (exclusivos): **70%
      saiu do ponto e nunca chegou a 1,5 m da bola**; **28% esteve sob a
      bola só enquanto ela estava alta demais** (> 1,8 m: é o passe pelo
      alto — o receptor corre para um ponto da trajetória onde a bola
      ainda está no ar); 2% entre ticks; 0% impedido por ter acabado de
      chutar.
    - **A causa é o receptor em movimento no chute.** O resolver mira onde
      o receptor está no instante do chute; a estimativa da decisão
      também. Acerto por velocidade do receptor no chute:

      | Velocidade do receptor no chute | Passes por partida | Receptor domina |
      |---|---|---|
      | < 1 m/s | 379 | 71% |
      | 1–3 m/s | 94 | 57% |
      | 3–5 m/s | 285 | 53% |
      | ≥ 5 m/s | 130 | 19% |

      Dos "fora do ponto", 78% tinham o receptor a ≥ 3 m/s no chute (dos
      passes que chegam, 35%) e 35% estavam numa corrida viva (dos que
      chegam, 3%). Dos que saíram do ponto, metade foi na direção do
      passador e metade para longe.
    - **Classificação: estrutura (sincronização passe × movimento do
      receptor), não calibração.** Não existe antecipação: nem a mira nem
      a estimativa levam em conta para onde o receptor está indo. É a
      mesma falta que derrubou o passe em profundidade. O mesmo sinal
      aparece nas interceptações (63% com o receptor a ≥ 3 m/s) e no erro
      de direção (55–60%).
    - Não medido: por que metade dos receptores se afasta do passador, e
      quanto da perda vem do giro e da frenagem da física.
- **Item 9, antes do commit 1 — por que metade dos passes sai na saída
  forçada (commit 5, `examples/forced_release.rs`, 30 partidas, 4-4-2 em
  casa; a outra orientação dá o mesmo):** a cada decisão do portador, o
  valor de cada opção como `choose_action` a vê (`option_values`, só com
  a feature `diagnostics`).
  - 886 passes por partida; **52,5% na saída forçada** (465).
  - **É decisão de conduzir, não falta de opção:**

    | Causa do período que termina na trava | Parcela |
    |---|---|
    | Nenhum passe valia mais que zero em nenhuma decisão | 0,3% |
    | Havia passe de valor positivo, mas nunca acima de segurar | 6,2% |
    | **Um passe valia mais que segurar, mas conduzir valia mais** | **90,3%** |
    | O passe foi a melhor opção em alguma decisão e não saiu | 3,2% |

  - **Nos períodos forçados, conduzir vale mais que o melhor passe em 97%
    das decisões.** Valor médio (× 1000): segurar 3,4, melhor passe 5,9,
    **conduzir 19,3**. O portador carrega a bola 17,9 m em média até a
    trava (3,2 m nos períodos em que o passe sai por valor). O melhor
    passe vale mais que zero em 84% das decisões, com sucesso estimado de
    71%.
  - **Por que conduzir ganha (leitura do modelo, não medição):** o valor
    de conduzir é `manter × xT(5 m à frente)`, com manter = 97% no espaço,
    e **não erode** com o tempo de bola; o de segurar erode 3% por tick; o
    do passe paga a chance de falha (29%) no ponto do receptor. Conduzir é
    a única opção sem custo.
  - **Onde a trava dispara:** 13% no terço defensivo, 56% no médio, 30% no
    final; 62% por meias, 29% por atacantes, 7% por defensores, 2% pelo
    goleiro. A construção desde a defesa quase não entra nela.
  - **Consequência para o item 9:** o termo de perda do passe muda o valor
    do passe, que não é o que decide nesses períodos. Tratar a condução é
    o escopo do item 7 (drible 1×1). Decisão pendente com o usuário.
- **Sinais registrados (não calibrar agora):**
  - **Posse do mandante:** 73% (arcade) → 56,9% (física no lance) →
    42,5% (física + engajamento). A assimetria mudou com a física, não com
    a tática; mas ela também troca de lado com o mecanismo de defesa, o
    que indica que ainda não é um equilíbrio — é sensível a cada mudança
    de movimento. Não investigar agora (decisão anterior).
  - **Acerto de passe 56% (real ~80%):** invariante 18 do lado do passe
    (a estimativa de linha supõe a cinemática antiga). Vai para o passo 4
    (refazer o item 5) ou (c2).
  - **Botes/faltas 10 / 3,6 (real ~70 / 22):** calibração para (c2).
- **Orçamento pontual do passo de física (aprovado 2026-10-02):** até +5%
  sobre o item 4, com a chegada (frenagem + giro parado) no mesmo commit
  da física ligada; teto de 560M instruções (parar acima de 562M). Sem
  commit de estrutura desligada acima de +1,5%. Valores iniciais
  conservadores que rodem: `max_accel` 15 m/s², reação 80 ms, giro 15
  rad/s; os realistas (4,5 / 200 / 6) ficam para quando o motor estiver
  estável. Critério para ligar: em 180 partidas, bola parada < 40% e
  passes > 500. **Depois do passo de física, a regra de +1,5% por item
  volta.**
  - **Goleiro saindo do gol: passo 2, não passo 1.** É comportamento
    (decisão de sair e quando), não física: o goleiro usa a mesma
    cinemática de todos.
- **Observação do usuário na v0 (2026-10-02), medida (30 partidas, código
  do passo 1):** no mandante (4-4-2), os dois meias centrais (#7, #8) têm
  **72% das posses** do time; zagueiros e laterais, 0,1–0,9% cada. No
  visitante (4-3-3): meias 20–22%, atacantes e pontas 13–18%, defesa
  0,5–1,4%. No real, zagueiros e laterais estão entre os que mais tocam.
  Hipótese: a moeda comum quase nunca escolhe passe para trás (xT menor),
  sem construção desde a defesa. Para (d) (comportamentos por papel) e
  (c2).
- **Impedimento — só a linha, sem apito `[ALTERADO v2.1]`:**
  - a linha é dado do `TickFrame`: o penúltimo defensor (o goleiro conta),
    calculado uma vez por tick a partir das posições;
  - corredores em `Run` não escolhem alvo além da própria linha;
  - não se apita nada: sem tiro livre, sem evento, sem mudança em
    reinício. O apito vira fase própria depois;
  - **viés conhecido:** se um corredor ficar além da linha (a linha anda
    depois que ele escolheu o alvo), o motor não flagra. Isso infla gols em
    profundidade em relação ao real. **Não é bug a corrigir em (c2)**; a
    correção é a fase do apito.
- **Tabela xT:** ponto de partida é a tabela pública de Karun Singh, como
  dado fixo no `TuningParams`. **Não é derivada do motor**, porque isso
  seria circular. Pode não transferir perfeitamente para o nosso motor: em
  (c2) é revisada contra o motor já estruturado.
  - **Origem:** Karun Singh, "Introducing Expected Threat (xT)", post de
    blog (karun.in/blog/expected-threat.html), ~2018–2019. **Não é paper
    revisado por pares.** Grade publicada em
    `karun.in/blog/data/open_xt_12x8_v1.json`.
  - **Não verificado neste ambiente** (o proxy bloqueia karun.in): o ano
    exato e o conjunto de dados (liga e temporadas) que geraram a grade.
    Uma versão anterior deste SPEC dizia "Premier League" sem verificação;
    foi removido. Conferir em (c2).
  - **Cópia usada:** `xT_Grid.csv` do repositório público de tutoriais de
    McKay Johns (`mckayjohns/youtube-videos`, `data/xT_Grid.csv`), que
    republica a grade de Karun Singh. Os 96 valores entram como estão, sem
    recálculo. Sinais de que é a grade certa: simétrica na largura e com
    0,2575 na célula central em frente ao gol. **Pendente de verificação
    contra `karun.in/blog/data/open_xt_12x8_v1.json`.** O essencial — ser
    externa ao motor — está garantido.
  - **Conversão de coordenadas:** a grade divide o campo inteiro em 12
    colunas iguais no comprimento e 8 linhas iguais na largura, em
    coordenadas normalizadas. No nosso campo (105 × 68 m): coluna =
    `x_ataque / 105 × 12`, linha = `y / 68 × 8`; `x_ataque = x` para quem
    ataca a direita e `105 − x` para quem ataca a esquerda (coluna 0 = gol
    próprio). Como a grade é simétrica na largura, a orientação de `y` não
    importa.
  - **Desvio do original:** Karun Singh usa o valor constante por célula. Nós
    interpolamos bilinearmente entre os centros das células, para a decisão
    não "pular" na fronteira de célula; no centro de cada célula o valor é
    exatamente o publicado (testado).
- **Ferramentas:** `examples/decision_stats.rs` (tempo com a bola,
  comprimento de passe, distância de chute, passes por posse) e
  `examples/calibrate.rs` (estatísticas com `chave=valor` sobrescrevendo o
  `TuningParams`, mais como terminam os passes).

**Restrição arquitetural `[ALTERADO v2.1]`:** nenhuma decisão do
`DecisionSystem` (nem do `ActionResolver`) pode consultar estado de
amostragem: interpolação do LOD Full, snapshots ou `sample()`. Só o estado
do `tick_logic`, via `MatchState` e `TickFrame`. O teste de paridade
detectaria uma violação, mas isso é regra de design, não um bug a ser pego
por teste.

**Orçamento:** a meta continua em 40 ms e o gate em 50 ms. Se a Fase 5
estourar, o perfil aponta o custo e o assunto volta para decisão humana.
A meta não é ajustada.

**Medição do orçamento `[ALTERADO v2.1]`:** tempo de relógio no CI mede a
máquina, não o código. O mesmo motor mediu **41,6 ms e 52,9 ms** em dois
runners (runs 30 e 32, mediana de 7), e o item 2 de (c1), com +32% de
trabalho, passou com 43,7 ms num runner rápido.
- **Checkpoint fino (bloqueante):** contagem de instruções do
  `tick_logic` com callgrind (`benches/instructions.rs`). Uma partida
  completa em Abstract, seed 2026 (a mesma do criterion), medindo
  "montagem + partida − montagem". Determinística: duas execuções dão o
  mesmo número. Falha se subir mais de **1,5%** sobre a linha de base
  commitada (`golden/instructions.txt`, o último item aceito);
  `UPDATE_INSTRUCTIONS=1` regrava a base ao aceitar um item.
- **Régua de conversão:** **543,4M instruções ≈ 41,2 ms**. Origem: commit
  `416e2a5` medido com este bench (543.405.929) e o criterion
  `tick_logic/full_match_abstract` do mesmo commit no CI (run 30:
  41,2 ms, mesma seed). Portanto ~13,2M instruções por ms. É aproximação:
  instruções não são tempo, e funções caras por instrução (libm) pesam
  diferente.
- **Gate em ms (50 ms) e meta (40 ms):** continuam rodando como **alarme
  grosseiro, não bloqueante**. Não são o critério de checkpoint.
- **Regra de checkpoint de (c1):** depois de cada item, reportar
  instruções (e o equivalente em ms pela régua). Se o item subir mais de
  1,5% sobre o anterior, PARAR e trazer o perfil.
- **Histórico (instruções, seed 2026):** antes de (c1) 543,4M; item 1
  566,6M (+4,3%, aceito antes desta regra existir); item 2 com cadência
  540,5M (−4,6%).

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
  - **Exceção — voo em curso `[ALTERADO v2.1]`:** se, na captura, a bola
    já está num voo lançado em tick anterior (`kick_ms != now_ms`), a
    posição desse voo em `now_ms` é avaliada uma vez e reutilizada enquanto
    a bola for esse mesmo voo. O único lugar que cria voo
    (`ActionResolver::kick`) usa o instante do tick atual, então passe,
    chute ou rebote dentro do tick tem `kick_ms == now_ms` e é sempre lido
    de novo. Um `debug_assert` confere o valor em cada acesso. Não é
    sampling: é cache de valor já calculado do tick lógico.
  - **Amostragem compartilhada da trajetória:** os pontos de interceptação
    dos perseguidores e do receptor usam a mesma amostragem da trajetória
    (preguiçosa, até 30 × 100 ms), calculada uma vez por tick.
  - **Resultado:** referência de paridade inalterada; `tick_logic` em
    Abstract caiu de 57,9 ms para **41,2 ms no CI** depois do modelo de
    defesa.
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
