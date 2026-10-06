# 01 — O que é o projeto

## Objetivo

Um jogo de gerenciamento de futebol (no espírito de Football Manager) que:

- roda no **navegador** (Chrome, Firefox, Safari, Edge), sem instalar nada;
- funciona **offline**, com os saves no próprio navegador;
- mostra a partida em **2D a 60 quadros por segundo**, com jogadores em
  movimento contínuo;
- simula o resto do mundo em segundo plano, rápido, **com o mesmo motor**.

O dono do projeto é o Rodrigo (GitHub `Tapia03`). Ele define escopo e ordem,
aprova desenhos e faz os merges. A conversa com ele é em português.

## Stack

| Camada | Tecnologia |
|---|---|
| Motor e regras | Rust (toolchain fixa **1.99.0**, MSRV 1.80), compilado para WASM com `wasm-bindgen` / `wasm-pack` |
| Matemática | `libm` para toda função transcendental (a std é proibida pelo `clippy.toml`) |
| RNG | `rand_xoshiro` (xoshiro256++), indexado por evento |
| Render | WebGL2 via `glow`; malha gerada na CPU |
| Frontend | Svelte 5 (runes) + TypeScript estrito + Vite |
| Persistência | SQLite em WASM (`@sqlite.org/sqlite-wasm`) sobre OPFS, com fallback para IndexedDB |
| Testes | `cargo test`, `wasm-pack test`, Playwright (Chromium, Firefox, WebKit) |
| CI / deploy | GitHub Actions; Cloudflare Pages (produção na `main`, preview por branch) |

Ainda não existem: autenticação e sync (Supabase, previstos para a Fase 10)
e PWA.

## Mapa do repositório

```
crates/
  fm-core/         geometria, campo, RNG, matemática via libm
  fm-entities/     jogadores: atributos, ficha estática, estado dinâmico, gerador
  fm-match/        O MOTOR DE PARTIDA (o coração do projeto)
  fm-render/       formas → malha de triângulos; único lugar com `unsafe` (ffi/)
  fm-wasm/         a fronteira wasm-bindgen: EngineHost, WorldHost, malha, câmera, SAB
  fm-world/        o mundo: liga, clubes, calendário, simulação do dia, WorldSave
  fm-persistence/  codecs dos blobs do save (ficha 60 bytes, dinâmico 14, onze 44)
  fm-economy/      vazio, Fase 9
  fm-test-utils/   alocador contador para testes
frontend/src/
  app/             App.svelte (a partida), Saves.svelte (tela de saves)
  engine-bridge/   ponte TS ↔ WASM; leitura do SharedArrayBuffer
  render/          interpolação de quadros
  save/            protocolo, cliente, schema e migrações do banco; world-db.ts
  world/           cliente e protocolos do Worker de mundo e do pool de partida
  workers/         engine.worker.ts (partida ao vivo), db.worker.ts (banco),
                   world.worker.ts (mundo), match.worker.ts (partida do pool)
tests/e2e/         Playwright
tests/golden/chromium/   goldens de pixel (só Chromium)
docs/              SPEC.md, STATE.md, patches/, handoff/
```

## O motor de partida (`fm-match`)

É a parte mais trabalhada e a mais delicada.

- **Tick lógico único de 10 Hz.** Decisões, ações e RNG só acontecem no
  tick. Em cada tick: máquina de fases do time → decisão de cada jogador →
  resolução da ação → âncoras de formação.
- **Movimento analítico.** A posição de um jogador e a da bola são funções
  fechadas do tempo desde o último evento (reação, aceleração, cruzeiro para
  o jogador; projétil com arrasto para a bola). Não há integração passo a
  passo.
- **LOD só muda a amostragem**, nunca a lógica:
  - `Full`: 60 snapshots por segundo, para o render;
  - `Reduced`: 10 por segundo;
  - `Abstract`: nenhum snapshot — é como o mundo é simulado.
  A mesma partida dá o mesmo resultado nos três, bit a bit.
- **RNG indexado por evento:** a semente de cada sorteio vem de
  `(seed da partida, id do jogador, contador de ações)`. Não há estado de
  RNG para guardar; uma partida se refaz só com a seed.
- **Espaço contínuo:** campo de 105 × 68 m, sem grade.
- **Sem alocação dentro do tick** (há teste com alocador contador).

Peças principais: `MatchEngine` (API: `new`, `tick_logic`, `sample`, `run`,
`set_tactics`, `state`, `events`), `MatchState`, `TickFrame`,
`DecisionSystem`, `ActionResolver`, `PhaseStateMachine`, `FormationAnchor`,
`PlayerKinematics`, `BallFlight`, `TuningParams`.

Táticas que o motor entende hoje: formação (4-4-2, 4-3-3, 4-2-3-1, 3-5-2,
5-3-2), mentalidade (5 níveis), largura, altura da linha e pressing
(4 níveis, versão mínima). **Não existem** no motor: "tempo", fadiga dentro
da partida, substituições, impedimento apitado, vantagem de mando.

## Como a partida chega à tela

```
Worker de engine                          Main thread
┌────────────────────┐   SharedArrayBuffer   ┌─────────────────────────┐
│ EngineHost (WASM)  │ ───────────────────▶  │ lê o anel, interpola    │
│ tick_logic + 6     │   anel de 16 slots    │ em TS, pede a malha ao  │
│ snapshots por tick │                       │ WASM (função pura) e    │
└────────────────────┘ ◀─── postMessage ───  │ desenha com WebGL2      │
        ▲                  (velocidade,      └─────────────────────────┘
        │                   táticas, pausa)
```

- Existe **um só** `MatchEngine`, dentro do Worker. A main thread não tem
  motor: só lê bytes.
- O anel é lock-free, um escritor e um leitor; o número de sequência é
  publicado por `Atomics.store` depois de o slot estar escrito.
- O layout do slot está na **versão 4** (72 palavras, 288 bytes): tick,
  tempo, placar, fases, expulsos, bola, 22 posições, cartões, posse,
  táticas dos dois times e estatísticas. Documentado no topo de
  `crates/fm-wasm/src/sab.rs`, espelhado em
  `frontend/src/engine-bridge/sab.ts`.
- HUD, painel de estatísticas, painel tático e rótulos são **DOM**. Campo,
  jogadores, bola e overlays são **malha no canvas**.
- Comandos táticos vão por `postMessage`; a partida é função de
  `(seed, lista de comandos com o tick de cada um)`.
- A câmera é só a "vista" da malha com outros números (função pura no WASM).

## Persistência (Fase 7A)

- Um **Worker de banco** (`db.worker.ts`), separado do de engine, é o único
  que abre arquivos e fala SQL.
- Armazenamento: VFS `opfs-sahpool`; onde não há OPFS, banco em memória com
  o arquivo inteiro gravado como blob no IndexedDB. As duas opções ficam
  atrás da interface de `frontend/src/save/files.ts`.
- **Um arquivo por save** mais um `catalog.sqlite` com a lista de saves e o
  ponteiro "arquivo ativo" de cada um.
- Schema como **cadeia de migrações** (`schema.ts`, `migrate.ts`):
  `PRAGMA user_version` + tabela `migrations`, cada migração na sua
  transação, `.bak` antes da cadeia, arquivo de versão mais nova recusado.
- Substituir um save (import) é **trocar o ponteiro no catálogo** numa
  transação; arquivos que nenhuma linha referencia são removidos no boot.
- Os outros Workers falam com o banco por `MessagePort`, com pedidos de
  domínio `{id, op, args}` (`save/protocol.ts`), nunca SQL cru.
- Telas: `?view=saves` (tela mínima), `?view=blank` (página vazia para
  testes), sem `view` a partida demo. A tela `?view=world` ainda não existe.

## O mundo (Fase 7B)

- **`fm-world`** gera o mundo inteiro de uma seed (20 clubes, 500 jogadores
  sintéticos, 380 partidas, todos os clubes em 4-4-2) e simula um dia:
  `matches_today()` → `play(id)` (puro, LOD Abstract) →
  `finish_day(resultados)`.
- **Schema v2** do save: `players` com a ficha em blob, `clubs`, `matches`
  (a seed de cada partida em BLOB de 8 bytes), `tactics`, `meta`. Operações
  `world.create`, `world.load`, `world.commitDay` (um dia = uma transação),
  `world.standings`, `world.round`.
- **Worker de mundo** (`world.worker.ts`): o único dono do `WorldHost`.
  Vive o dia e pede o `commitDay` ao Worker de banco por `MessagePort`. O
  banco é sempre a verdade: se o commit falha, o mundo é recarregado dele.
- **Pool de Workers de partida** (`match.worker.ts`): `min(núcleos, 10)`
  Workers, cada um com um `WorldHost` sincronizado antes da rodada; fila
  dinâmica; com pool o Worker de mundo coordena e não joga. O save é o
  mesmo, bit a bit (`save.digest`), com qualquer tamanho de pool.
- A main cria todos os Workers e distribui as portas; nenhum Worker cria
  outro.

## Invariantes (o que nunca pode quebrar)

- Tick lógico único de 10 Hz; o LOD só muda a amostragem.
- RNG indexado por evento.
- Paridade bit a bit nativo × WASM
  (`crates/fm-match/golden/engine_parity.txt`).
- `test_cross_lod_consistency`: Full, Reduced e Abstract dão o mesmo estado
  final.
- `libm` para toda função transcendental.
- `unsafe` só em `fm-render/src/ffi/` e em `fm-test-utils::alloc_counter`.
- Nenhuma alocação em `tick_logic`.
- **Orçamento de custo do tick:** contagem de instruções (callgrind) de uma
  partida inteira em Abstract; cada commit pode subir no máximo **+1,5%**
  sobre a referência em `crates/fm-match/golden/instructions.txt`
  (hoje **582.952.421**). Estourou: parar e trazer o perfil.
- Mudança de arquitetura entra no SPEC **antes** do código.
