# 03 — Estado atual e próximos passos

Fotografia de **2026-10-05**, atualizada depois do pool de Workers de
partida (7B.4b).

## Git

- Branch de trabalho: **`fase-7b`**, criada da `main` depois do merge do
  PR #9.
- `main`: `4c38095` (merge da 7A).
- Commits da 7B no remoto, sobre a `main`, em ordem:
  - documentos e correções pós-7A: `3eeb2c0`, `42fd813`, `2a29d6b` (SPEC da
    7B), `80ddd99` (esta pasta), `c0ecd89`;
  - **commit 2** — `599403f`: `fm-world` nativo, todos os clubes em 4-4-2;
  - **commit 3** — `0238834` (SPEC) e 7B.3.1 a 7B.3.5: `fm-persistence`,
    `WorldSave`, migração v2, operações `world.*`;
  - **commit 4** — `ef8f413` (SPEC) e 7B.4.1 a 7B.4.5, mais `23590c0`:
    `WorldHost`, Worker de mundo e os dois protocolos, progresso,
    cancelamento, e a medição que tornou o pool obrigatório;
  - **commit 5, o pool (7B.4b)** — `bece92d` (7B.4b.1, SPEC do pool),
    `eb0bcc6` (7B.4b.2, `WorldHost` com `play` puro, `finish_day(resultados)`
    e `sync_day`), `3659da1` (7B.4b.3, Worker de partida, pool e fila
    dinâmica), `bbdbf94` (7B.4b.4, cancelamento e falhas com o pool) e
    `187b6dd` (7B.4b.5, medição com e sem pool).
- Não há PR aberto da 7B. **O PR da 7B só é aberto quando todos os commits
  estiverem prontos** (pedido do dono).
- Branches que **não são para mexer**: `fase-6-v0`, `spike-render`,
  `spike-render-v2` (são de sessões anteriores; o dono cuida).
- Não há trabalho não commitado.

### Incidente de infraestrutura do GitHub em 2026-10-05

Runners hospedados em fila: os jobs esperam 10–15 min e são cancelados sem
rodar ("The job was not acquired by Runner of type hosted"). Nos pushes do
7B.4b.2 e do 7B.4b.3 os jobs que conseguiram runner passaram; os outros
ficaram vermelhos sem ter rodado (no 7B.4b.2: `test-rust` e
`test-e2e (firefox)`; no 7B.4b.3: `test-wasm`, `test-e2e (firefox)`,
`test-e2e (webkit)` e `stable-canary`). Esses jobs **não são reexecutados**:
ficam vermelhos por infra.

- **Exceção pontual à regra "um push por commit":** os sub-commits 4 e 5 do
  pool (7B.4b.4 e 7B.4b.5) foram empurrados juntos, com a atualização destes
  documentos.
- **Regra provisória, enquanto durar a fila:** job cancelado por infra não
  conta como vermelho; só conta job que rodou e falhou.
- **Não é precedente.** Quando o GitHub normalizar, volta a regra normal.
- **O push 4+5 (`b9326d5`, run 37370161249) também não rodou os testes:**
  `bench` passou (582.952.421 instruções, +0,00%) e `stable-canary` passou;
  `test-rust`, `test-wasm` e os três `test-e2e` foram cancelados por infra.
  Consequências: **os e2e do 7B.4b.4 e do 7B.4b.5 nunca rodaram no CI**, e
  **a medição do pool no CI não existe**. Localmente, `cargo test
  --workspace --release` e o clippy passam em `b9326d5`.
- **O dono autorizou reexecutar só os jobs cancelados desse run** (os dos
  runs anteriores não são reexecutados). O rerun (2026-10-06) rodou tudo:
  suíte normal verde nos três navegadores; **pool medido no Chromium do CI
  em 435 ms (meta 2 atingida)**; e a medição do WebKit falhou com um Worker
  de partida mudo por 30 s, de causa indeterminada.
- **Em andamento: 7B.4c, pool observável** (desenho e resultados no SPEC):
  confirmação de recebimento, `error` e `messageerror` escutados a vida
  toda, prazo adaptativo, medição repetida 20 vezes no WebKit do CI. O
  código está feito e não custou tempo mensurável (Chromium do CI com pool
  de 4: 660–674 ms). **Em aberto:** com o prazo adaptativo, o WebKit do CI
  retirou um Worker na primeira rodada de um teste da suíte, sem imprimir o
  motivo; o push seguinte existe para imprimi-lo. Os números do prazo (piso
  3 s, teto 30 s, 10×) só mudam por decisão do dono. A tela espera isto
  fechar.
- Os runners do GitHub voltaram ao normal em 2026-10-06; a regra provisória
  acima deixou de ser necessária.

## O que o `fm-world` já faz

- **`World::new(seed, clube_do_usuario)`** — o mundo inteiro como função da
  seed: 500 jogadores do gerador existente, por cotas de posição (25 por
  clube: 3 goleiros, 8 defensores, 8 meio-campistas, 6 atacantes); 20
  clubes; elencos por um draft por posição com ruído; onze inicial por
  aptidão.
- **Calendário** (`fixtures.rs`): 380 partidas pelo método do círculo, uma
  rodada por semana (dia 6), temporada de 266 dias; a seed de cada partida
  deriva da seed do mundo.
- **Um dia**: `matches_today()` → `play(id)` (puro: só lê o mundo, roda o
  `MatchEngine` em LOD Abstract) → `finish_day(resultados)` (grava
  resultados, credita minutos, roda o `weekly_update` quando a semana
  fecha).
- **Classificação** calculada dos resultados.
- **`overall`** por posição (só para as telas; o motor não usa) e tabelas de
  nomes sintéticos.
- **`WorldSave`**: `to_save()` / `from_save()`, com os blobs de
  `fm-persistence` e as colunas que as telas consultam.
- **`tests/season.rs`**: mede uma temporada inteira; só roda quando pedido
  (`cargo test --release -p fm-world --test season -- --ignored --nocapture`).

## O que existe no navegador

- **Worker de banco** (7A) com as operações `world.create`, `world.load`,
  `world.commitDay`, `world.standings` e `world.round`; migração v2; save
  sem mundo responde `no-world`.
- **Worker de mundo** (`world.worker.ts`): dono do `WorldHost`; `world.new`,
  `world.open`, `world.advance` (dia a dia, um `commitDay` por dia),
  progresso por partida, cancelamento, prazo de 10 s para o banco.
- **Pool de Workers de partida** (7B.4b): `min(núcleos, 10)` Workers, cada um
  com um `WorldHost` sincronizado antes da rodada; fila dinâmica; com pool o
  Worker de mundo coordena e não joga; com 1 núcleo não há pool. O mesmo
  `save.digest` com 0, 1, 2 e 4 workers. Worker de partida que morre é
  retirado e a partida dele é jogada pelo Worker de mundo; pool que não
  responde, o mundo segue sozinho.
- **Ainda não existe:** a tela `?view=world`.

## As formações (decidido: todos em 4-4-2)

Simulando temporadas inteiras (380 partidas, nativo):

| Formações dos clubes | Seed | Gols por partida | Correlação força do elenco × posição final |
|---|---|---|---|
| Sorteadas entre as cinco (desenho original) | 2026 | 4,56 | 0,19 |
| Sorteadas | 7 | 5,34 | 0,65 |
| Todos em 4-4-2 | 2026 | 2,98 | 0,94 |
| Todos em 4-4-2 | 7 | 3,29 | 0,71 |
| Todos em 5-3-2 | 2026 | 2,73 | 0,76 |
| Todos em 5-3-2 | 7 | 2,76 | 0,84 |
| Todos em 4-3-3 | 2026 | **12,64** | 0,79 |
| Todos em 4-3-3 | 7 | **12,45** | 0,73 |

Na temporada sorteada da seed 2026, os três clubes em 4-3-3 terminaram em
1º, 2º e 4º (o 4º com o elenco mais fraco da liga), os três em 3-5-2 em
15º, 16º e 20º, e o elenco mais forte terminou em 7º.

Leitura: **4-3-3 contra 4-3-3 não defende**, e com todos em 4-4-2 o motor
fica perto do real (~3,0 gols por partida; real ~2,7). É defeito do motor,
para a Fase 8. **Não foi tocado.** O dono decidiu: todos os clubes da 7B em
4-4-2 (`fm_world::LEAGUE_FORMATION`); na 7C a tela de tática só oferece
4-4-2 habilitada.

Outras duas observações da mesma medição: o motor **não tem vantagem de
mando** (162 vitórias em casa, 158 fora), e a distância entre campeão e
lanterna ficou larga (93 a 15 pontos com todos em 4-4-2), o que é ajuste do
ruído do draft (`DRAFT_NOISE`).

## Custo de simulação medido

| Medida | Valor | Onde |
|---|---|---|
| Uma partida em Abstract, nativo | ~50 ms | máquina do dono, 16 núcleos |
| Uma rodada (10 partidas), nativo | mediana 455–494 ms | idem |
| Temporada (380 partidas), nativo | 17,4–18,9 s | idem |
| Uma partida no WASM de release | 169–182 ms | main thread, com HUD |
| Rodada no navegador, **sem pool** | **1.453 ms** (141 ms por partida) | Chromium do CI, 4 núcleos |
| Idem, WebKit / Firefox | 1.536 ms / 8.784 ms | CI |
| Rodada no navegador, sem pool | 1.676 ms | Chromium, máquina do dono |
| Rodada no navegador, **pool de 10** | 567 ms | Chromium, máquina do dono |
| Rodada no navegador, com pool | **pendente** (sub-commit 5) | Chromium do CI |

- O WASM está ~3,4× mais lento que o nativo e o Firefox roda este WASM ~6–7×
  mais devagar que o Chromium. São **dívidas de desempenho para a Fase 8;
  não mexer agora**. Alavanca conhecida: `wasm-opt`, desligado de propósito.
- **Metas do pool** (SPEC, 7B.4b): (1) mesmo digest com qualquer tamanho de
  pool — cumprida; (2) rodada ≤ 700 ms no Chromium do CI — **a medir**;
  (3) ≤ 300 ms em máquina de 8 núcleos ou mais — **não atingida** (567 ms com
  10 workers em 16 núcleos: os Workers não escalam linearmente), dívida da
  Fase 8; (4) sem regressão com 1 núcleo.
- **Decisão com o número do CI:** ≤ 700 ms, aprovado; 700 ms a 1 s, aprovado
  com ressalva (registrar a diferença); > 1 s, **parar e trazer o perfil**.
- O Firefox fica fora da meta: uma partida custa 871 ms no CI, então a
  rodada nunca fica abaixo disso. É a dívida do WASM no Firefox, não falha
  do pool.

## Próximos passos

1. **Obter e ler a medição do pool no CI** (o push dos sub-commits 4 e 5
   não a produziu; ver o incidente acima) e registrar
   no SPEC e no STATE: mediana da rodada com e sem pool, por partida, custo
   do `commitDay`, núcleos do runner, memória do pool; WebKit e Firefox só
   informam.
2. **Commit 6 — a tela** (o "commit 5" do desenho original da 7B; o pool
   entrou antes). **Desenho apresentado ao dono em 2026-10-05, ainda não
   aprovado; nada codificado.** Resumo do que foi proposto: rota
   `?view=world&save=<id>`; `World.svelte` com o mesmo lock de aba única da
   tela de saves; clube escolhido por número na criação do mundo (os nomes
   só existem depois de o mundo ser gerado); uma operação de leitura nova
   no Worker de mundo para a classificação do Rust (o `WorldHost` já tem
   `standings()`, o Worker não a expõe); a tela lê resultados e tabela
   direto do banco. Depois de aprovado, o desenho entra no SPEC antes do
   código.
   - Tela mínima `?view=world`: criar mundo (seed e clube), "Avançar dia"
     com a barra de progresso, dia e rodada atuais, resultados da última
     rodada, classificação em tabela crua.
   - **A navegação tem de fechar visualmente:** a tela de saves com link
     para o mundo, a tela do mundo com link de volta. O dono se perdeu entre
     URLs na 7A.
   - Comparar a classificação do Rust (`WorldHost`) com a do banco.
   - Testes pelo navegador: determinismo (dia 10, recarregar, dia 20 = direto
     ao dia 20, mesmo `save.digest`) e crash (matar o Worker de mundo no
     meio de uma rodada; o save está no dia anterior e a rodada refeita dá o
     mesmo resultado).
3. Medições finais no SPEC e no STATE, e o PR `fase-7b → main`.

## Depois da 7B

- **7C — telas de gerenciamento:** meus saves, elenco, tática (formação e
  papel por slot), calendário, classificação, botão "Avançar dia" sempre
  visível. Já registrado para a 7C: "Avançar até a próxima partida" (com uma
  rodada por semana, avançar dia passa por seis dias vazios), o roteador de
  verdade e o link da partida para os saves (que invalida os cinco goldens).
- **7D — assistir a partida:** abrir o render da Fase 6 em LOD Full a partir
  do calendário e voltar. Nenhum motor novo. Com o Worker de engine ativo, o
  pool de partidas reduz para `min(núcleos − 1, 9)` ou pausa.
- Cada sub-fase: desenho aprovado antes do código, um PR, CI verde.

## Fase 8 — Bola longa + contraparte defensiva (motor)

Quatro componentes, com time-box por componente:

1. aplicar o patch do passe pelo alto (pronto em `docs/patches/`);
2. goleiro saindo do gol;
3. impedimento apitado;
4. marcação de corredores redesenhada (a tentativa anterior foi revertida).

Para fechar: gols ≤ 3,5 por partida **e** os quatro componentes no lugar;
acima de 3,5, parar e reescopar. 3,5 é "aceitável", não "bom".

A Fase 8 vai receber também o que o MVP descobrir. Já na lista:

- **desequilíbrio entre formações** (a tabela acima) — o achado mais grave
  até agora;
- ausência de vantagem de mando;
- o WASM 3,4× mais lento que o nativo;
- o Firefox ~7× mais lento que o Chromium neste WASM;
- o pool acima da meta de 300 ms em máquina com muitos núcleos.

## Fases 9 e 10

- **Fase 9:** mercado, contratos, finanças, ligas múltiplas, copas; a UI da
  partida (botão "Simular partida", painel tático só do clube do usuário,
  zoom e pan manuais); PWA; demo e deploy; a base de dados real.
- **Fase 10:** packs e modding, autenticação, sync. O sync por chunk do SPEC
  antigo será redesenhado, assumindo IndexedDB como backend primário no
  Safari.

## Dívidas abertas (resumo)

| Dívida | Onde entra |
|---|---|
| Gols 4,4 por partida (4-4-2 × 4-3-3); acerto de passe 68,5% | Fase 8 |
| Formação decide o campeonato; 4-3-3 × 4-3-3 dá 12,5 gols | Fase 8 |
| Passe pelo alto 21% (patch pronto) | Fase 8 |
| Saída forçada 52–53% | Fase 8 ou própria |
| Sem fadiga na partida; pressing alto não custa nada | fase de fadiga / (c2) |
| "Tempo" não existe no motor | a definir |
| Sem IA tática (o adversário não reage) | "IA tática" / (d) |
| (c2) calibração e (d) comportamentos por papel | sem lugar na ordem |
| 6C-2: overlays de zonas de pressing e opções de passe | depois da Fase 8 |
| WASM 3,4× mais lento que o nativo | Fase 8 |
| Firefox ~7× mais lento que o Chromium neste WASM | Fase 8 |
| Pool: 567 ms com 10 workers em 16 núcleos (meta 300 ms) | Fase 8 |
| OPFS e recarga da página nunca verificados no Safari real | verificação manual |
| Pênaltis 0,005 por partida; kickoff sobreposto; bola parada 3% | (c2) / Fase 8 |
