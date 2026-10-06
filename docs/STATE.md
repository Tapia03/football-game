# STATE — estado oficial do projeto

Resumo de uma tela que sobrevive a compactações de sessão. A fonte de verdade
de arquitetura e regras é o [`docs/SPEC.md`](SPEC.md); este arquivo só diz
*onde estamos*. Atualizado em **2026-10-05** (Fases 6 e 7A mergeadas; Fase 7B — mundo mínimo e calendário — **em implementação**, branch `fase-7b`: commits 1 a 4 feitos (SPEC, `fm-world`, persistência do mundo, Worker de mundo com progresso e cancelamento) e o **pool de Workers de partida (7B.4b) feito**, obrigatório porque a rodada mediu 1.453 ms no Chromium do CI. **Medição do pool no CI pendente.** Próximo: o commit 6 (tela `?view=world`), com desenho a aprovar antes do código).

**Para quem chega agora:** comece por [`docs/handoff/`](handoff/00-LEIA-PRIMEIRO.md).
As seções deste arquivo abaixo de "Fase anterior — Fase 5 (c1)" são o
registro daquela fase: os números de lá são do motor de `f17b1d9`.

## Ordem geral (redefinida pelo usuário em 2026-10-05)
Fases 0–6 fechadas. A ordem nova, que **substitui** a anterior ("Bola
longa" antes da UI):

1. **Fase 7 — MVP de gerenciamento** (próxima; começa depois do merge do
   PR da Fase 6).
2. **Fase 8 — Bola longa + contraparte defensiva** (pausada até a Fase 7
   fechar).
3. **Fase 9 — Resto do gerenciamento:** mercado, contratos, finanças,
   ligas múltiplas, copas — e o que o SPEC chamava de "Fase 7" (UI da
   partida, botão simular, painel tático só do clube do usuário, zoom e
   pan). **PWA e Demo + Deploy foram movidas explicitamente para cá**
   (depois do MVP).
4. **Fase 10 — Polimento e comunidade:** packs, auth, sync.

**Motivo:** depois das Fases 0–6 o projeto é um motor de partida com um
spike visual, não um jogo de gerenciamento jogável. O MVP vem antes de
refinar o motor para (1) provar que é um jogo de gerenciamento, (2)
descobrir problemas estruturais — persistência, custo de simulação em
background, UX de avançar dia, comunicação entre Workers — enquanto o
código é pequeno, e (3) ter feedback visual do motor no contexto real,
para orientar a próxima rodada de refinamento.

**Fase 7 em quatro sub-fases, um PR cada, CI verde antes da próxima:**
- **7A — Persistência local (desenho aprovado em 2026-10-05, no SPEC):**
  SQLite em WASM (build oficial) num Worker dedicado, VFS `opfs-sahpool`,
  fallback para blob no IndexedDB; um arquivo por save + catálogo;
  colunas para as telas e blobs bit a bit para o motor; migração de
  schema em cadeia no open (`user_version` + tabela `migrations`);
  escrita atômica por troca de ponteiro no catálogo + limpeza de órfãos
  no boot; export/import; `navigator.storage.persist()`; protocolo de
  domínio por `MessagePort`. **A fonte de verdade local passou a ser o
  SQLite em OPFS** (era IndexedDB); o sync por chunk será redesenhado na
  Fase 10. Branch `fase-7a`, da `main` depois do merge do PR #7.
  **Feita e mergeada (2026-10-05, PR #9).** O que o CI mostrou: Chromium e Firefox
  têm OPFS; o WebKit do Playwright não tem `navigator.storage` e roda só
  no fallback. Tela mínima em `?view=saves` (`&storage=idb` força o
  fallback). Detalhes e achados no SPEC.
  - **O IndexedDB não é um caminho de segunda classe.** No WebKit do
    Playwright ele é o **único** caminho. Decisão do usuário
    (2026-10-05): tratar o IndexedDB como o backend que vai carregar toda
    uma fatia de usuários, e a **Fase 10 (auth/sync) tem de assumir
    IndexedDB como backend primário no Safari, não OPFS**.
    - Ressalva registrada: isso foi medido no WebKit do Playwright, que
      não tem `navigator.storage`. O Safari real pode ter OPFS (a
      documentação pública diz que tem desde a versão 15.2, e o
      `opfs-sahpool` pede 16.4 ou mais novo; aba privada não). **Não
      verificado.** Até alguém medir num Safari de verdade, vale a
      premissa conservadora acima.
  - **A verificar no Safari real** (não há como no CI): se o OPFS
    funciona, e se o Worker carrega logo depois de recarregar a página (no
    WebKit do Playwright falha por ~1 s; o cliente tenta de novo).
  - **Para a 7B:** o Worker de mundo fala com o banco por `MessagePort`
    (`connect()`); entram `world.load` e `world.commitDay` (uma transação
    por dia) e o teste de determinismo por `save.digest`.
- **7B em andamento (desenho aprovado em 2026-10-05, no SPEC):** Worker
  de mundo **único** primeiro; o pool de Workers de partida se decide com
  o número do CI (rodada > 1 s: obrigatório, trazer o número antes;
  < 500 ms: fica para depois). Migração v2 (a primeira de verdade), save
  sem mundo recusado com mensagem clara, navegação saves ↔ mundo.
  "Avançar até a próxima partida" e o link da partida para os saves são
  da 7C.
  - **7B.2 feito:** `fm-world` nativo (bootstrap, calendário, simulação do
    dia, classificação; 13 testes). **Todos os clubes em 4-4-2**, por
    decisão do usuário depois do achado abaixo. Temporada nativa em
    17,4–18,9 s; rodada com mediana de 455–494 ms.
  - **7B.3 feito:** codecs em `fm-persistence` (ficha 60 bytes, dinâmico
    14, onze 44), `WorldSave` em `fm-world`, migração v2 (recria
    `players`, `clubs` e `matches`, com guarda; `seed` vira BLOB),
    operações `world.create` / `load` / `commitDay` / `standings` /
    `round`. Save com mundo: **160 kB**. Save da 7A migra e responde
    `no-world`.
  - **7B.4 feito:** `WorldHost`, Worker de mundo (`world.new`, `world.open`,
    `world.advance` dia a dia), progresso por partida, cancelamento entre
    partidas, recarga, prazo do banco. **Medição no CI (4 núcleos, um
    Worker de mundo):** rodada de 1.453 ms no Chromium, 1.536 ms no
    WebKit, 8.784 ms no Firefox; 141 / 152 / 871 ms por partida; o
    `commitDay` custa 10–57 ms. **Pool obrigatório pela regra (> 1 s).**
  - **Dívida — Firefox ~6× mais lento neste WASM** (871 contra 141 ms
    por partida no CI). Causa não investigada; o dono vai conferir num
    Firefox real. Se confirmado, item da Fase 8, junto com o WASM 3,4×
    mais lento que o nativo. Um pool não leva o Firefox abaixo do tempo
    de uma partida (~0,9 s por rodada, no melhor caso).
  - **7B.4b feito (pool de partida):** 4 sub-commits de código (7B.4b.2 a
    7B.4b.5, mais o SPEC em 7B.4b.1). `WorldHost` com `play` puro,
    `finish_day(resultados)` e `sync_day`; `min(núcleos, 10)` Workers de
    partida; mesmo `save.digest` com 0, 1, 2 e 4 workers; com pool o
    coordenador não joga; fila dinâmica; Worker de partida morto é
    retirado e a partida dele é jogada pelo Worker de mundo; pool que não
    responde, o mundo segue sozinho. **Medição no CI pendente (sub-commit
    5).** Decisão com o número do Chromium do CI: ≤ 700 ms aprovado;
    700 ms a 1 s aprovado com ressalva; > 1 s parar e trazer o perfil.
  - **Dívida — meta 3 do pool não atingida (Fase 8):** na máquina do dono
    (16 núcleos), Chromium, a rodada leva 567 ms com 10 workers, contra
    1.676 ms sem pool (meta: ≤ 300 ms com 8 núcleos ou mais). Dez Workers
    em 16 núcleos não escalam linearmente. Entra na Fase 8 junto com o
    Firefox ~7× e o WASM 3,4× mais lento que o nativo.
  - **Firefox com pool:** ~2,7 s por rodada no runner, estimado; nunca
    abaixo de uma partida (871 ms). É a dívida do WASM no Firefox, não
    falha do pool.
  - **Incidente de infraestrutura do GitHub (2026-10-05):** runners
    hospedados em fila; jobs esperam 10–15 min e são cancelados sem rodar
    ("The job was not acquired by Runner of type hosted"). Nos pushes do
    7B.4b.2 e do 7B.4b.3 os jobs que conseguiram runner passaram; os
    cancelados ficam vermelhos por infra e **não são reexecutados**.
    - **Exceção pontual à regra "um push por commit":** os sub-commits 4
      e 5 do pool (7B.4b.4 e 7B.4b.5) foram empurrados juntos. Não é
      mudança de regra nem precedente.
    - **Regra provisória, enquanto durar a fila:** job cancelado por
      infra não conta como vermelho; só conta job que rodou e falhou.
      Com o GitHub normalizado, volta a regra normal.
    - **O push 4+5 (`b9326d5`, run 37370161249) também ficou sem
      testes:** `bench` passou (582.952.421 instruções, +0,00%) e
      `stable-canary` passou; `test-rust`, `test-wasm` e os três
      `test-e2e` foram cancelados por infra. **Os e2e do 7B.4b.4 e do
      7B.4b.5 nunca rodaram no CI e a medição do pool no CI não existe.**
      Localmente, `cargo test --workspace --release` e o clippy passam.
      O dono autorizou reexecutar só os jobs cancelados desse run.
  - **7B.4b medido no CI (2026-10-06, rerun, 4 núcleos):** Chromium
    **435 ms com pool de 4**, 485 ms com 3, 939 ms sem pool — **meta 2
    atingida (≤ 700 ms), pool aprovado por essa medida**. Firefox 3.764 ms
    com pool (8.969 ms sem). WebKit 1.520 ms sem pool. WASM: 1,5 MB do
    mundo + 1,3 MB por Worker de partida. Tabela no SPEC. A linha de base
    sem pool caiu de 1.453 para 939 ms sem mudança no motor; não
    investigado.
  - **WebKit do CI: um Worker de partida ficou mudo na medição** (pool de
    3, `"play 0: o Worker de partida não respondeu em 30.0 s"`); o teste
    exige zero retirados e reprovou. A suíte normal passou nos três
    navegadores. **Causa indeterminada.**
  - **7B.4c em andamento (pool observável, desenho aprovado em
    2026-10-06, no SPEC):** confirmação de recebimento (`started`),
    `error` e `messageerror` escutados a vida toda, prazo de confirmação
    de 5 s, prazo da partida adaptativo (10× a mediana das últimas 20,
    piso 3 s, teto 30 s), medição repetida 20 vezes no WebKit do CI, e a
    correção do `FM_WORLD_SEASON` (a temporada rodava em todo push).
    **A tela (commit 6) espera isto fechar.**
  - **Fica para o commit 6** (o "commit 5" do desenho original; o pool
    entrou antes; desenho apresentado em 2026-10-05, ainda não
    aprovado): a tela `?view=world` e a navegação saves ↔ mundo;
    comparar a classificação do Rust com a do banco (precisa do
    `WorldHost`); os testes de determinismo e de crash com o mundo de
    verdade. **Desenho a aprovar antes do código.**
- **Achado da 7B.2 — desequilíbrio entre formações (item da Fase 8):**
  4-3-3 contra 4-3-3 dá 12,5 gols por partida; com as formações sorteadas
  a formação decide a tabela (correlação força × posição final 0,19–0,65;
  os clubes em 4-3-3 no topo, os em 3-5-2 no fundo); com todos em 4-4-2 a
  correlação é 0,71–0,94 e os gols ficam em ~3,0 por partida. A Fase 8
  corrige. Tabela completa no SPEC (7B).
- **Nota para a 7C:** a tela de tática só oferece 4-4-2 habilitada; as
  outras formações aparecem com o aviso "formação desbalanceada até a
  Fase 8".
- **Observações da temporada, não bloqueantes:** o motor não tem vantagem
  de mando (162 vitórias em casa, 158 fora, ~50%); a distância entre
  campeão e lanterna com todos em 4-4-2 foi larga (93 a 15 pontos) —
  ajuste de `DRAFT_NOISE`, para (c2).
- **Dívida de desempenho (Fase 8 ou 9, não mexer agora):** o WASM roda
  uma partida ~3,4× mais devagar que o nativo (169–182 ms contra ~51 ms).
  Alavanca conhecida: `wasm-opt`, desligado de propósito.
- **7B — Mundo mínimo e calendário (escopo):** bootstrap determinístico (1 liga,
  20 clubes, ~500 jogadores sintéticos — sem a base FM real), round-robin
  de 38 rodadas / 380 partidas, `WorldSimulator` num Worker com LOD
  Abstract para todas as partidas de background (o `MatchEngine` que já
  existe), save a cada dia, avançar dia com barra de progresso.
- **7C — Telas básicas:** meus saves, elenco, tática (formação e papel
  por slot), calendário, classificação, botão "Avançar dia" sempre
  visível. Svelte.
- **7D — Integração com o 2D:** "Assistir partida" abre o render da Fase
  6 em LOD Full; ao fim volta ao calendário. Nenhum motor novo.

**Regras da Fase 7:** uma pergunta por vez; **não mexer no motor** (se o
MVP precisar, PARAR e trazer a proposta); determinismo bit a bit e
`test_cross_lod_consistency` continuam bloqueantes; bench no orçamento.

**Fora do MVP (Fase 9 ou depois):** base FM real; mercado, contratos,
finanças, diretoria, imprensa, base, seleções; ligas múltiplas, copas,
continentais; overlays táticos avançados no 2D (inclui o 6C-2); auth e
sync; modding / packs.

**Nome:** o jogo ainda não tem nome; nos documentos, "o jogo" ou "o
projeto".

Sem lugar na ordem ainda: (c2) e (d) da Fase 5 (das quais depende o
6C-bis), fadiga, IA tática.

## Fase 6 (render) — completa e mergeada (PR #7, `9a27cab`)
**Fase 6 — Snapshot + Renderer2D + Canvas**, branch `fase-6` (criada de
`main` em `6ae001a`, depois do merge do PR #6). Partes: **6A** infra de
render (worker + SAB + interpolação) → 6B HUD e overlays → 6C painel
tático (mentalidade, tempo, pressing) → 6D câmera. 6C-bis (comportamentos
por papel) fica para depois de (d). Desenho do 6A no SPEC, Fase 6.

**6A feito** (commits `faf5cdf`, `d05bcc3`, `ce1fde4`, `534e549` e o de
testes): motor no worker, anel de snapshots no SAB (208 bytes × 16), 60
snapshots por segundo publicados adiantados, a main lê e interpola em
TS, malha como função WASM pura. Latência tick → desenho a 1×: média
18,9 ms, máximo 22,7 ms.

**6B em três commits visuais:**
- **6B-1 feito:** SAB versão 2 (período, cartões, posse), HUD em DOM,
  `run_to`, primeiro golden.
- **6B-2 feito:** SAB versão 3 (288 bytes por posição: estatísticas de
  time, com o xG relido de fora do tick e testado bit a bit contra o
  motor), painel lateral, toggles F1 (rótulos: número + posição; o projeto
  ainda não tem nomes) e F2 (vetores de velocidade), dois goldens.
- **6B-3 feito:** overlays geométricos como funções WASM puras do
  snapshot (SAB continua na versão 3): F3 linha de impedimento pela regra
  (penúltimo defensor, bola e meio-campo; o motor usa só o penúltimo
  defensor, testado bit a bit) e F4 linhas de formação por setor; terceiro
  golden (`match-overlays.png`). Custo de F3 + F4: +186 vértices, sem
  diferença mensurável por quadro no Chromium.
- **6C feito (2026-10-05):** painel tático em DOM (Casa/Visitante,
  mentalidade, pressão; tempo visível e desabilitado), comando por
  `postMessage`, SAB versão 4 (táticas na palavra 55; o painel mostra o
  que o snapshot diz). `MatchEngine::set_tactics` e pressing mínimo no
  motor (escala a contenção fora da área; só Medium calibrado, bit a bit
  com antes). Critério 3: Defensive → Attacking sobe o bloco ≥ 10,0 m em
  5 s (pela página: +14,1 m). Critério 5: distância ao portador nos 5 s
  depois da troca 4,07 / 3,46 / 2,20 m (Low / Medium / UltraHigh; pela
  página: 2,37 → 1,39 m). Partida = função de (seed, comandos com tick),
  testado bit a bit pela página. Três goldens refeitos.
- **6D feito (2026-10-05):** câmera como função WASM pura (vista
  parametrizada, alvo com zona morta, blend exponencial de 100 ms que
  assenta exato em ≤ 0,8 s): FullPitch, HalfPitch (2×, segue a bola),
  Tactical (zoom-out + rótulos e linhas de formação automáticos, sem
  mexer nos toggles); tecla `C` cicla, `1`/`2`/`3` direto, botões no
  rodapé. Custo por quadro: +0,08 ms (HalfPitch), +0,44 ms (Tactical).
  Velocidades 1×/2×/5×/10×/30×/60×. Cinco goldens. Aprovado visualmente
  pelo usuário ("os 3 modos estão ótimos"). Zoom/pan manuais ficam para
  a Fase 7.
- **6C-2: pendente, depois da Fase 8 ("Bola longa")** (overlays de zonas de
  pressing e opções de passe; trabalho de motor + render, desenho e PR
  próprios). Não entra no PR da Fase 6.
- **Fase 9 (a antiga "Fase 7" do SPEC), já registrado:** botão **"Simular partida"** (LOD Abstract,
  tela de resultado com placar, estatísticas e eventos, botão voltar). A
  infraestrutura existe; o custo é UI.

**Bench:** referência atualizada para 582.952.421 instruções no fim do 6C
(era 590.430.506; caiu 1,27% no 6C.2 com a paridade inalterada).

**Dívidas do painel tático (registradas no 6C):**
- **Pressing sem fadiga.** No futebol real, pressing alto cobra o time
  aos 60–70 min. O motor não tem fadiga. Candidato a (c2) ou a uma fase
  de fadiga.
  - **Achado (estimativa, 12 partidas, não fato):** mesmo sem fadiga,
    UltraHigh **não** é estritamente melhor. Só o mandante mudando de
    nível, gols sofridos: Low 4,00, Medium 2,67, High 2,75, UltraHigh
    6,58 — quem contém a 1–2 m é batido e sobra espaço atrás. Emergente,
    não calibrado: o motor já tem trade-off real de posicionamento. A
    fadiga acrescentaria o custo ao longo do tempo, que continua
    faltando. Tabela completa no SPEC (6C).
- **Sem IA tática.** Quando o usuário muda um time, o outro continua com
  as táticas padrão: não responde a Attacking com Cautious, não muda com
  o placar nem com o relógio. Fase futura "IA tática" ou (d).
- **Seletor Casa/Visitante é provisório.** Na Fase 7 só o lado do clube
  do usuário é controlável; o seletor some ou trava.
- **Tempo** não existe no motor (pergunta de modelo; encosta na soltura
  forçada, dívida da (c1)).

**Piso do teste de latência:** mais de 15 quadros em 3 s (amostra mínima,
não taxa de quadros); o WebKit do CI desenha 27–46 quando divide o runner
com o teste de F1/F2. Se a intermitência voltar: dividir o job do WebKit
(latência isolada + resto) — registrado no SPEC, não implementado.

**Cuidado ao rodar os e2e localmente:** se uma aba (inclusive o painel de
navegador do Claude) estiver aberta numa página da partida, ela disputa a
CPU com o WebGL por software dos testes e os testes de tempo (latência,
`runTo`) falham de forma intermitente. Fechar ou navegar essa aba para
outra página antes de rodar.

## Fase anterior — Fase 5 (c1), mergeada
**Fase 5 — Role Behaviors**, branch `fase-5`. O usuário decidiu (2026-10-04)
fechar (c1) no motor do 5D-2 (`f17b1d9`); PR #6 mergeado em `6ae001a`.

### O que a fase entrega (motor de `f17b1d9`, 180 partidas por orientação)
| Métrica por partida | Fase 5 (c1) | Real (aprox.) |
|---|---|---|
| Passes (acerto) | 970 (68,5%) | 900 (80%) |
| Botes / faltas | 70,8 / 19,4 | 70 / 22 |
| Amarelos / vermelhos | 2,86 / 0,22 | 4 / 0,15 |
| Pênaltis | 0,005 | 0,3 |
| Chutes (no alvo) | 26,4 (8,1) | 25 (9) |
| Gols | 4,43 | 2,7 |
| Saída forçada (passes na trava dos 5,5 s) | 52–53% | — |
| Bola parada | 3% | ~35% |
| Posse do 4-4-2 contra 4-3-3 (casa / fora) | 59,7% / 60,7% | — |
| Custo de `tick_logic` | 590,4M instruções ≈ 44,8 ms | meta 45 ms |

- Física coerente: reação, aceleração, giro e chegada só para quem está
  no lance; reação em `steer` corrigida; reinícios dentro das linhas.
- Defesa que tira a bola na frequência do futebol (limiar do bote 1,01).
- Estimativa do passe alinhada à execução no bloqueio da linha.
- Ferramentas de medição em `crates/fm-match/examples/` (`calibrate`,
  `pass_failures`, `buildup_stats`, `forced_release`, `option_breakdown`,
  `carry_cost`, `tackle_stats`).

### Fora do que foi entregue (decisão do usuário)
- **(c2) calibração** e **(d) comportamentos por papel** (com os testes de
  Overlap, Pressing, Counter Attack e Tight Marking do SPEC): não
  iniciados. Itens 6–9 de (c1) e o item 5 (passe em profundidade,
  desligado): não feitos.
- **Para (c2):** vermelhos 0,22; assimetria de posse 4-4-2 × 4-3-3 (~60%,
  estrutural, do `plan_shape`); gols 4,4; bola parada 3%.

### Patch pronto, não aplicado: altura da bola em `intercept_point`
- **Arquivo:** `docs/patches/intercept-point-height.patch` (aplica sobre
  `f17b1d9`: `git apply docs/patches/intercept-point-height.patch`).
- **O que é:** uma condição em `fn intercept_point`
  (`crates/fm-match/src/engine.rs`, linha 663 em `f17b1d9`): pular os
  pontos da trajetória em que a bola está acima de
  `ControlTuning::max_height` (1,8 m). Para isso o `FlightPath` passa a
  guardar `Vec3` e a função recebe `reach_h`.
- **Por que existe:** no passe pelo alto o receptor corria para debaixo da
  bola ainda no ar e ela caía onde ele estava (acerto 21%).
- **Efeito medido:** passe pelo alto 21% → 57%; acerto geral 68,5% → 74%;
  saída forçada 52% → 44%; passes 1.082; **gols 8,5 por partida** e
  chutes 47,9; `match_statistics_are_plausible` falha (9,17 > 7,0).
- **Por que não foi aplicado:** falta a contraparte defensiva. A bola longa
  por cima da defesa passa a chegar e nada a contém.

### Fase 8: "Bola longa + contraparte defensiva" (pausada até a Fase 7 fechar)
- **Branch:** nasce da `main` depois do merge da Fase 7.
- **Quatro componentes, time-box por componente (não escopo fechado):**
  patch do passe pelo alto (pronto; provavelmente o primeiro, para medir
  o efeito puro), goleiro saindo do gol, impedimento apitado, marcação
  de corredores redesenhada.
- **Para fechar a fase:** gols ≤ 3,5 por partida **e** os quatro
  componentes no lugar. Se a marcação de corredores falhar de novo, o
  número não fecha sozinho. Se passar de 3,5: PARAR e reescopar.
- **3,5 é "aceitável", não "bom"** (real ~2,7): se fechar em ≤ 3,5, a
  diferença fica como dívida no backlog de refinamento.
- **Também entra, vindo do MVP (7B.2):** o **desequilíbrio entre
  formações** (4-3-3 × 4-3-3 com 12,5 gols por partida; a formação
  decidindo a tabela) e a ausência de vantagem de mando. Enquanto isso
  não for corrigido, o mundo usa só 4-4-2.
- **Pré-requisito:** aplicar o patch acima.
- **Escopo:** (1) goleiro saindo do gol (interceptar, cortar cruzamento,
  líbero) — o antigo passo 2.5; (2) impedimento apitado, com tiro livre;
  (3) marcação de corredores **redesenhada** — o mecanismo tentado não
  chega perto (mediana 3,9 m do corredor com a reação corrigida; alvo
  1,5 m).
- **Métrica de sucesso:** gols ≤ 3,5 por partida com o patch aplicado.

### Histórico do Caminho A (como chegamos aqui)
1. Física de movimento — feito (`8e5e7e8` + `5d34253`).
2. Defesa ajustada à física — engajamento e corrida decidida feitos
   (`19e7194`); "defesa acompanha o corredor" tentado e revertido.
3. Item 9 (construção desde a defesa) — só diagnóstico. A investigação
   desviou para a execução: bloqueio na estimativa do passe (`ac88f94`),
   reinícios (`8b59e10`), bote (`616b1f6`), reação em `steer` (`f17b1d9`).
4. Refazer o item 5 e 5. itens 6–8: não feitos.

**Dois nomes de "passo" aparecem no SPEC:** os passos do Caminho A (acima)
e os passos do item 5 (passe em profundidade: passo 1 `lofted_lane`
ligado; passo 2 implementado e desligado).

### Tentativa revertida: defesa acompanha o corredor (2026-10-03)
Commits `53f017a` (SPEC) e `2915e96` (código), revertidos. Desenho: cada
corrida ganha um marcador (o adversário de linha mais próximo), que fica
1,5 m do lado do gol do corredor, na física. Lições:
1. **O estimador de custo errou 2×.** Previsto: 200–320 instruções por
   tick com marcador na física (+1,1%). Medido no CI: **614.856.548,
   +2,67%** sobre 598,8M (~620 instruções por tick com marcador). Causa
   não investigada (sem perfil: o callgrind só roda no CI e a saída não é
   guardada). Estimativas futuras levam margem maior.
2. **Acompanhamento parcial.** Distância marcador–corredor no fim da
   corrida: mediana **4,4 m** (alvo 1,5 m; 5,3 m no início). Só **29%** das
   corridas terminam com o marcador a ≤ 3 m.
3. **A posse do 4-4-2 piorou:** 57,7% → 59,7% (4-4-2 em casa).
4. **A linha defensiva não recuou:** 26,8 m → 26,0 m (limite combinado:
   5 m). O risco de a linha descer atrás do corredor não se confirmou.

### Padrão de descobertas (vale para toda investigação futura)
Quatro defeitos de física/execução apareceram em sequência, cada um
mais fundo que o anterior, e todos primeiro como "problema de decisão ou
de calibração":
1. **Passe em profundidade** (0,3% de acerto): a estimativa supunha uma
   inércia que a execução não tinha.
2. **Marcação de corredores** (revertida): custo 2× o estimado e
   acompanhamento parcial.
3. **Reinícios**: 48% dos reinícios cobrados eram perdidos na hora (ponto
   em cima da linha).
4. **Reação em `steer`**: numa virada forte a reação recomeçava a cada
   tick e o jogador quase não freava.

Regra: quando um sintoma aparecer numa camada (decisão, calibração),
conferir antes se a camada de baixo (execução, física) faz o que se
supõe — medindo a execução de fora, tick a tick, antes de mexer em
constante ou em modelo.

### Itens de (c1)
| Item | Estado |
|---|---|
| 1 xG, cobertura, domínio | pronto |
| 2 xT + moeda comum + cadência 3 | pronto |
| 3 linha de impedimento | pronto |
| 4 estado `Run` | pronto |
| 5 passe em profundidade | parcial: passo 1 ligado; passo 2 desligado, refazer (Caminho A, passo 4) |
| 6 apoio sem bola | não iniciado |
| 7 drible 1×1 | não iniciado |
| 8 tabela | não iniciado |
| 9 construção desde a defesa | previsto (Caminho A, passo 3) |

Também prontos na Fase 5: (a) `TickFrame`, (b) modelo de defesa,
otimização (cache de voo + trajetória compartilhada). (c2) e (d): não
iniciados.

### Commits na `fase-5` (sobre `main`, mais recente primeiro)
| Hash | Resumo |
|---|---|
| (reverts) | Revert de `2915e96` e `53f017a` (defesa acompanha o corredor) |
| `2915e96` | Defesa acompanha o corredor — **revertido** (614,9M, +2,67%) |
| `53f017a` | SPEC do commit 2 — **revertido** |
| `87b1692` | Baseline 590.430.506; SPEC: 5D-3 medido e não aplicado |
| `f17b1d9` | Reação em `steer` corrigida; bote em 1,01 (590,4M, −1,98%) |
| `72385d0` | Baseline 602.368.082; SPEC: 5C, re-medição do passe, bug da reação |
| `616b1f6` | Bote recalibrado: limiar 0,97 e cartões (602,4M, −1,28%) |
| `ab66f15` | SPEC/STATE: varredura do bote e valores aplicados |
| `b06c28e` | Medição: por que o passe vale 3× menos que conduzir |
| `7be015d` | Medição: por que saem ~8 botes; perfil do custo dos reinícios |
| `90f19b1` | CI guarda o perfil do callgrind; baseline 610.183.817 |
| `8b59e10` | Reinícios não são mais perdidos na hora (610,2M, +2,10%, aceito) |
| `1921910` | Medição: custo da condução na execução (e o bug dos reinícios) |
| `d3150dc` | Medição: por que 52% dos passes saem na saída forçada |
| `f4ebba2` | Baseline 597.652.818; piso de faltas do teste em 2 |
| `efc9cd1` | Medição: receptor fora do ponto (commit 4 do diagnóstico do passe) |
| `ac88f94` | Bloqueio na estimativa do passe (597,7M, −0,20%) |
| `3f55274` | Medição B: como acontece a interceptação em voo |
| `5b39ba1` | Medição A: classificação das falhas de passe |
| `be1d847` | SPEC/STATE: critério de (c) ≤ 48 ms, passo 2.5, diagnóstico do item 9 |
| `692f5af` | Baseline de instruções 598.838.544; dívida 5,8M |
| `19e7194` | Corrida decidida: gatilho + teto de corredores (598,8M) |
| `cb05418` | SPEC: passo 2 do Caminho A, commit 1; medição de 180 partidas |
| `6e65012` | docs: consolida estado no fim da Fase 5 (item 5 parcial) |
| `1b329ab` | STATE: consolidação antes da pausa |
| `5d34253` | Física no lance + engajamento defensivo (596,3M) |
| `9953a2d` | SPEC: física só no lance; medição 588,9M; teste de faltas falhou |
| `36a13b5` | SPEC: física completa custa +55% (+37% mantendo planos) |
| `8e5e7e8` | Física commit 1: estrutura desligada (537,6M, golden idêntico) |
| `cbee512` | SPEC: invariante 18, nova ordem de (c1), desenho da física |
| `5ce4218` | Item 5 passo 2: chegada + corredor legal, DESLIGADO |
| `3ef931c` | SPEC/STATE: item 5 passos 2–3 medidos; concentração nos meias |
| `055bb01` | Item 5 passo 1: `lofted_lane` ligado |
| `cec2090` | Item 4: teste de retorno à forma; cadência como comportamento |
| `3c50c56` | Item 5: profundidade implementada e desligada |
| `82dff91` | Item 4: estado `Run` |
| `e89a672` | SPEC: justificativa da cadência 3, estimativa do item 4 |
| `a020b6d` | Item 3: linha de impedimento |
| `0cf5f21` | Item 2: cadência 3; contador só com `diagnostics` |
| `5b362ed` | Item 2: cadência de decisão + checkpoint por instruções |
| `0df6051` | SPEC: proveniência da tabela xT |
| `f2dc274` | Item 2: tabela xT e moeda comum |
| `057e7dc` | STATE: marco do pipeline WASM → WebGL2 → Pages |
| `7a4a454` | Item 1: xG, cobertura do gol, domínio |
| `38f1931` | SPEC/STATE: decisões de (c1) |
| `48259d2` | (c): diagnóstico + ferramentas de calibração + STATE |
| `416e2a5` | Cache de voo + trajetória compartilhada |
| `fca68bb` | (b): modelo de defesa |
| `edb821f` | SPEC: resultados do modelo de defesa |
| `1b2defb` | `TuningParams` |
| `2ef1bb1` | SPEC: modelo de defesa aprovado |
| `260ed2e` | SPEC: (a) medido no CI, diagnóstico dos desarmes |
| `03270f1` | (a): `TickFrame` |
| `4e83ba1` | Toolchain 1.99.0 + stable-canary |
| `1a931d3` | Exemplo `timing` |

### No SPEC, mas não no código
- **Invariante 18 do lado do passe:** a estimativa de linha de passe ainda
  supõe a cinemática antiga (acerto 56%). Caminho A, passo 4, ou (c2).
- **Defesa acompanha corredores** (tentado e revertido; revisitar depois
  do item 9) e **goleiro saindo do gol**: o resto do passo 2 do Caminho A.
- **Item 9** (construção desde a defesa) e **itens 6–8**: só a ordem e o
  escopo.
- **Valores realistas da física** (4,5 m/s² / 200 ms / 6 rad/s): no
  código estão os conservadores (15 / 80 / 15).
- **Apito de impedimento** e **regra do kickoff**: fases próprias depois.

### Implementado, mas desligado
- Passe em profundidade (`ValueTuning.through_balls = false`).
- Contadores de diagnóstico (só com a feature `diagnostics`).

### Dívidas conhecidas
- **Custo (número da Fase 5):** 590,4M instruções (590.430.506, commit
  `f17b1d9`) ≈ 44,8 ms. **A referência atual do bench é 582.952.421**
  (`crates/fm-match/golden/instructions.txt`, atualizada no fim do 6C; ver
  "Bench" na seção da Fase 6). Na época:
  **dentro da meta desejável de 45 ms (593M)** — a dívida de custo
  registrada desde a física no lance não existe mais. O job de bench
  guarda o perfil (artefato `callgrind`), dentro do critério de saída de (c)
  (≤ 48 ms ≈ 633M, decidido em 2026-10-03). Regra por commit: +1,5%.
- **Execução de passes — dívida estrutural para depois de (c1)** (uma
  fase futura pega os três juntos):
  - **Sincronização com o receptor:** em boa parte era o bug da reação em
    `steer` (corrigido no 5D-2: o receptor já recebia o ponto de encontro
    com a bola, mas não conseguia virar). Sobra: "receptor fora do ponto"
    em 12,6% dos passes curtos (< 10 m), e o resolver e a estimativa
    continuam mirando onde o receptor está no chute.
  - **Passe pelo alto (≥ 28 m):** 21% de acerto. Causa e correção
    conhecidas (patch pronto, acima); depende da fase "Bola longa".
  - **Passe em profundidade:** 0,3% de acerto quando ligado; desligado.
- **Bola conduzida para fora em jogo corrido:** 5,9 por partida (4,2 antes
  da correção dos reinícios): o portador sai do campo com a bola. Não
  investigado. Se for barato, vira commit próprio; se for estrutural
  (falta de limite da linha para o portador), vira fase.
- **Saída forçada:** 52–53% dos passes saem na trava dos 5,5 s. É decisão
  de conduzir (92% dos casos) e não reage ao bote. Só caiu (44%) com o
  patch do passe pelo alto. Frente da Fase 6 ou de fase própria.
- **Teste de faltas:** `match_statistics_are_plausible` tem piso de 2 faltas
  por partida (era 3; relaxado em 2026-10-03). É piso de regressão, não
  meta: o real é ~22 e o motor está em 2,8 (180 partidas).
- **Acerto de passe 68,5%** (real ~80%): invariante 18 do lado do passe →
  Caminho A passo 4 ou (c2).
- **Botes 70,8 / faltas 19,4** (real ~70 / 22): `challenge_threshold` 1,01
  (2026-10-03, depois da correção da reação). Amarelos 2,86 (real ~4).
- **Posse 4-4-2 × 4-3-3 não convergida:** o 4-4-2 fica com ~60% nas
  duas orientações, com teto de corredores 1, 2 ou sem teto; sem corrida
  nenhuma, 54,3%. Com a física, parte da assimetria é da formação em si,
  não das corridas (SPEC, commit 1 do passo 2).
- **Gols 4,4** por partida (real ~2,7); xG por chute 0,21 (real ~0,10).

### Branches vivas
| Branch | Papel |
|---|---|
| `main` | o que está mergeado: Fases 0 a 6 e a 7A (`4c38095`, merge do PR #9). É a branch padrão do repositório desde 2026-10-05 |
| `fase-7b` | **branch de trabalho atual** (Fase 7B); PR só quando todos os commits estiverem prontos (falta a tela) |
| `fase-5`, `fase-6`, `fase-7a` | já mergeadas; não commitar nelas |
| `fase-6-v0` | spike de render sobre o motor de antes da física; **nunca mergeia** |
| `spike-render-v2` | render da `fase-6-v0` sobre o motor de `5d34253` (só arquivos de render, motor e `docs/` intactos); **nunca mergeia** |
| `spike-render` | primeiro spike (frame estático); **nunca mergeia** |

### Comandos para retomar
```sh
git checkout fase-7b
cargo test --workspace --release
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
# Bench de instruções (callgrind, seed 2026; falha se > +1,5% sobre
# crates/fm-match/golden/instructions.txt). UPDATE_INSTRUCTIONS=1 regrava.
cargo bench -p fm-match --bench instructions
# Golden de paridade nativo×WASM, depois de mudar comportamento:
UPDATE_GOLDEN=1 cargo test -p fm-match --release --lib parity
# Calibrador (180 partidas, 4-3-3 e 4-4-2):
FM_FORMATIONS=433,442 cargo run --release -p fm-match --features diagnostics --example calibrate
# Preview: push em qualquer branch fase-* (ou spike-*) dispara o deploy
# (.github/workflows/deploy.yml) e publica em
# https://<branch>.football-game-b5k.pages.dev
```
**Ver o jogo a partir da Fase 6 (2026-10-04):** o render agora está na
própria branch `fase-6`; os spikes ficaram para trás.
- **Remoto:** todo push em `fase-*` publica um preview no Cloudflare Pages:
  https://fase-6.football-game-b5k.pages.dev
- **Local:** `git checkout fase-6 && npm ci && npm run build && npm run
  preview`, depois http://localhost:4173 (ou `npm run dev`, porta 5173).
- O worktree `../football-game-spike` (branch `spike-render-v2`, motor do
  5D-2 com o render antigo, sem worker) não é mais atualizado.

**Golden de pixel (só Chromium) — fluxo de dois pushes:** a referência
tem de ser a imagem que o Chromium do CI produz (fontes e rasterização
desta máquina são outras).
1. 1º push com o teste novo: o job `test-e2e (chromium)` fica **vermelho**
   ("snapshot doesn't exist"); o Playwright grava a imagem obtida e o job
   a sobe no artefato `golden-chromium`.
2. Baixar o artefato (`gh run download <run> -n golden-chromium`) e
   commitar a imagem em `tests/golden/chromium/`.
3. 2º push: verde.
**Substituir um golden que já existe custa dois pushes vermelhos, não
um** (aprendido no 6C; vale para 6C-2, 6D e Fase 7): enquanto a
referência antiga existir, o teste falha na comparação, para ali (as
capturas seguintes do mesmo teste nem são tiradas) e o artefato devolve
a própria referência antiga. Então:
1. push da mudança visual — vermelho no golden (esperado);
2. push que **remove** as referências afetadas (`git rm`) — vermelho, e o
   artefato `golden-chromium` traz as novas;
3. push com as novas — verde.
**Câmera (6D), constante da suavização — não mexer antes de ver em ação:**
- Está em 100 ms (`camera::SMOOTH_MS`). O critério "assentar em ≤ 1 s" é
  sobre o **golden** (chegar exatamente no alvo para a captura ser
  reproduzível), não sobre a sensação (95% do caminho em 300 ms).
- O salto final a 1 cm é conservador: 5 cm já é 1 pixel a 2× de zoom.
- Se depois do 6D.3 o HalfPitch parecer nervoso, a primeira alavanca é
  subir a constante para 120–125 ms (alvo exato em ~0,98 s no pior caso).

**Playwright local trava ao encerrar (2 vezes até o 6D.2):** quando é o
próprio Playwright que sobe o `npm run build && npm run preview`, os
testes terminam mas o processo não sai (não derruba o servidor no
Windows) e o resumo final não é impresso. Contorno: ler o log dos testes;
com um preview já no ar na 4173 ele reaproveita e encerra normal. Se
acontecer de novo no 6D.3, investigar.

**Padrão do projeto (6C-2, 6D, Fase 7):** remover as referências **no
mesmo commit** da mudança visual reduz de dois para um push vermelho.

**Playwright local (instalado em 2026-10-04):** `npx playwright install`
foi rodado nesta máquina; `npx playwright test` roda os e2e localmente
(sobe `npm run build && npm run preview` na porta 4173) em ~45 s: fora do
CI a configuração usa 2 workers — com um worker por núcleo as páginas,
que desenham com WebGL por software, saturam a máquina (a primeira
tentativa travou por 30 min). O teste de golden de pixel é pulado fora do
CI (a referência é do Chromium do CI, Linux); `FM_GOLDEN=1` força a
comparação local.

**`npm run dev` com o worker e o SAB:** funciona sem ajuste — o servidor de
dev envia COOP/COEP (`vite.config.ts`). Só não sobe se a porta 5173
estiver ocupada (`strictPort`); nesse caso `npx vite --port 5174`.

## PRs
| PR | Conteúdo | Estado |
|---|---|---|
| Tapia03/football-game#1 | Fases 0 e 1 | mergeado |
| Tapia03/football-game#2 | Fase 2 | mergeado |
| Tapia03/football-game#4 | Fase 3 + SPEC v2.1 + glow | mergeado |
| Tapia03/football-game#5 | Fase 4 | mergeado |
| Tapia03/football-game#6 | Fase 5 (c1) | mergeado (`6ae001a`) |
| Tapia03/football-game#7 | Fase 6 (render: 6A, 6B, 6C, 6D) | mergeado (`9a27cab`) |
| Tapia03/football-game#8 | Fase 7A aberta por engano contra a branch errada | fechado sem merge |
| Tapia03/football-game#9 | Fase 7A (persistência local) | mergeado (`4c38095`) |
| — | Fase 7B (`fase-7b`) | abre quando todos os commits estiverem prontos (falta a tela), com CI verde |

Branch `spike-render`: nunca mergeia (spike visual do render).

## Invariantes ativos
Ver SPEC, Seção 0 e decisões das Fases 3–5. Os que mais pesam no dia a dia:
- Tick lógico único de 10 Hz; o LOD só muda a amostragem. RNG indexado por
  evento. **Nunca modificar.**
- Decisões (`DecisionSystem` / `ActionResolver`) leem só `MatchState` +
  `TickFrame`, nunca estado de amostragem.
- Paridade bit a bit nativo×WASM (`crates/fm-match/golden/engine_parity.txt`).
- libm para toda função transcendental. `unsafe` só em `fm-render/src/ffi/`
  e `fm-test-utils::alloc_counter`.
- Toolchain fixa em 1.99.0. Orçamento de `tick_logic` em Abstract: meta 40 ms,
  gate 50 ms (alarme não bloqueante). A meta não se ajusta. **Checkpoint
  fino = instruções (callgrind)**, +1,5% no máximo por item; régua
  543,4M ≈ 41,2 ms.
- Mudança de arquitetura atualiza o SPEC no mesmo PR, antes do código.

## Decisões pendentes
- (decidido 2026-10-03) **Critério de saída de (c): ≤ 48 ms** pela régua
  (≤ ~633M instruções). A meta desejável continua 45 ms (~593M); o gate
  de 50 ms (~660M) nunca foi violado pela régua de instruções. Hoje: 590,4M ≈
  44,8 ms.
- **xT:** conferir a cópia contra `karun.in/blog/data/open_xt_12x8_v1.json`.

## Marcos
- **2026-10-03 — bug da reação em `steer` corrigido** (`f17b1d9`): acerto de
  passe 59% → 68,5% e custo de volta abaixo de 45 ms (590,4M).
- **2026-10-03 — a defesa tira a bola na frequência do futebol** (`616b1f6`):
  74,5 botes e 21,3 faltas por partida (real ~70 / ~22), vermelhos 0,17
  (real ~0,15), e custando menos instruções (−1,28%).
- **2026-10-02 — pipeline ponta a ponta confirmado:** WASM → WebGL2 (glow) →
  Cloudflare Pages. O spike renderiza no navegador (campo, 22 jogadores,
  bola, "OK (5754 vértices)").

## Previews
- Cada branch `fase-*` tem o seu:
  `https://<branch>.football-game-b5k.pages.dev` (por exemplo
  https://fase-7b.football-game-b5k.pages.dev; a tela de saves fica em
  `/?view=saves`).
- Spike visual antigo (motor de `5d34253`):
  https://spike-render-v2.football-game-b5k.pages.dev
- Spike de render: https://spike-render.football-game-b5k.pages.dev (o sufixo
  `-b5k` do subdomínio é do Cloudflare).

## Bugs conhecidos sem correção
- **Kickoff sobreposto:** as âncoras de bola parada põem atacantes no campo
  adversário no pontapé inicial. A regra do kickoff não é aplicada. Alvo:
  Fase 5 (d) ou 6.
- **Pênaltis** 0,005 por partida (real ~0,3). Alvo: Fase 6. (Os **vermelhos**
  estão em 0,22, real ~0,15; o "0,53" antigo era de antes da física.)
- **Tempo de bola parada** 4% (real ~35%): reinícios curtos demais. Entra em
  (c2).
- **Impedimento não apitado** (decisão): infla gols em profundidade. Viés
  conhecido, não corrigir em (c2); a solução é a fase do apito.
