# 03 — Estado atual e próximos passos

Fotografia de **2026-10-05**, no fim da sessão que escreveu estes arquivos.

## Git

- Branch de trabalho: **`fase-7b`**, criada da `main` depois do merge do
  PR #9.
- Commits no remoto, sobre a `main`: `3eeb2c0` (STATE), `42fd813` (correções
  pós-7A), `2a29d6b` (SPEC da 7B), `80ddd99` (esta pasta) e a correção do
  STATE. CI verde em `42fd813`, o último commit com código.
- `main`: `4c38095` (merge da 7A).
- Não há PR aberto da 7B. **O PR da 7B só é aberto quando os cinco commits
  estiverem prontos** (pedido do dono).
- Branches que **não são para mexer**: `fase-6-v0`, `spike-render`,
  `spike-render-v2` (são de sessões anteriores; o dono cuida).

### Trabalho não commitado (commit 2 da 7B)

```
 M Cargo.lock
 M crates/fm-world/Cargo.toml
 M crates/fm-world/src/lib.rs
?? crates/fm-world/src/fixtures.rs
?? crates/fm-world/src/names.rs
?? crates/fm-world/src/squad.rs
?? crates/fm-world/src/world.rs
?? crates/fm-world/tests/season.rs
```

(A pasta `docs/handoff/` já está commitada.)

O `fm-world` está pronto e testado localmente: 13 testes nativos passando,
clippy nativo e wasm32 limpos. **Não foi commitado porque há uma decisão
pendente** (abaixo).

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
  fecha). `advance_day(callback)` faz os três e avisa a cada partida.
- **Classificação** calculada dos resultados.
- **`overall`** por posição (só para as telas; o motor não usa) e tabelas de
  nomes sintéticos.
- **`tests/season.rs`**: mede uma temporada inteira; só roda quando pedido
  (`cargo test --release -p fm-world --test season -- --ignored --nocapture`).

## A decisão das formações (tomada: todos em 4-4-2)

> Atualização de 2026-10-05: o dono escolheu a opção 1. O `fm-world` foi
> ajustado (`LEAGUE_FORMATION`) e commitado; o trabalho listado acima como
> "não commitado" já está no git. Depois disso o **commit 3 também foi
> feito** (codecs, `WorldSave`, migração v2, operações `world.*`; save com
> mundo de 160 kB). O próximo passo é o **commit 4**: `WorldHost`, Worker
> de mundo e progresso, com a medição no WASM do CI que decide o pool. O
> STATE e o SPEC (seção 7B) têm o registro completo.
>
> Atualização seguinte, mesmo dia: o **commit 4 foi feito** e a medição
> saiu: uma rodada leva **1.453 ms no Chromium do CI** (4 núcleos, um
> Worker de mundo), acima do limite de 1 s. Pela regra do dono, o **pool
> de Workers de partida é obrigatório**: o próximo passo é o desenho do
> pool, a aprovar, e só depois o commit 5 (tela). No Firefox a rodada leva
> 8,8 s (dívida registrada, causa não investigada).

Simulando temporadas inteiras (380 partidas, nativo):

| Formações dos clubes | Seed | Gols por partida | Correlação força do elenco × posição final |
|---|---|---|---|
| Sorteadas entre as cinco (desenho aprovado) | 2026 | 4,56 | 0,19 |
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
para a Fase 8. **Não foi tocado.**

Pergunta feita ao dono, **sem resposta ainda**:

1. **Todos os clubes em 4-4-2** — recomendado. Consequência: quando a 7C
   deixar o usuário trocar de formação, escolher 4-3-3 volta a quebrar a
   liga, até a Fase 8.
2. Manter o sorteio e registrar o achado.
3. Sortear só entre 4-4-2 e 5-3-2 (o confronto entre as duas não foi
   medido).

Hoje o código sorteia entre as cinco (`Formation::ALL`, em
`World::new`). Com a resposta: ajustar essa linha se for o caso, registrar o
achado no SPEC e no STATE (item da Fase 8, com a tabela), e commitar.

Outras duas observações da mesma medição: o motor **não tem vantagem de
mando** (162 vitórias em casa, 158 fora), e a distância entre campeão e
lanterna ficou larga (93 a 15 pontos com todos em 4-4-2), o que é ajuste do
ruído do draft (`DRAFT_NOISE`).

## Custo de simulação medido

| Medida | Valor | Onde |
|---|---|---|
| Uma partida em Abstract, nativo | ~50 ms | máquina do dono, 16 núcleos |
| Uma rodada (10 partidas), nativo | mediana 494 ms | idem |
| Temporada (380 partidas), nativo | 18,9 s | idem |
| Uma partida no WASM de release | 169–182 ms | main thread, com HUD |

O WASM está ~3,4× mais lento que o nativo. É **dívida de desempenho para a
Fase 8 ou 9; não mexer agora**. Alavanca conhecida: `wasm-opt`, desligado de
propósito.

**Regra do pool de Workers** (decisão do dono): fazer o Worker de mundo
**único** primeiro e medir no CI.

- Rodada no CI **> 1 s**: o pool é obrigatório — levar o número ao dono
  **antes** de implementar.
- Rodada no CI **< 500 ms**: o pool fica para depois.
- No meio: o dono decide com o número.
- Se uma temporada inteira for proibitiva: parar e reportar.

## Os commits que faltam na 7B

O desenho completo está no SPEC, seção "7B — Mundo mínimo e calendário".

1. ~~SPEC~~ — feito (`2a29d6b`).
2. **`fm-world` nativo** — pronto, aguardando a decisão das formações.
3. **`fm-persistence` + migração v2 + operações `world.*` no Worker de
   banco.**
   - A migração v2 é a primeira de verdade: em `players`, a ficha estática
     inteira em blob e o potencial; em `clubs`, força e formação; em
     `matches`, chutes e chutes no alvo.
   - **Teste extra pedido pelo dono:** um save criado pelo código da 7A (um
     arquivo v1 de verdade, guardado como fixture) aberto pelo código da 7B,
     migrando sem quebrar.
   - **Save sem mundo** (todo save da 7A): `world.load` recusa com mensagem
     clara (erro `no-world`), sem quebrar; a tela oferece criar um mundo
     nele.
   - `world.create`, `world.load`, `world.commitDay` — o dia inteiro numa
     transação.
4. **`WorldHost` em `fm-wasm`, `world.worker.ts` e progresso**, com a
   medição de uma rodada e de uma temporada **no WASM do CI**. É aqui que o
   pool se decide.
5. **Tela mínima `?view=world`** e os testes de determinismo e de crash pelo
   navegador.
   - **A navegação tem de fechar visualmente:** a tela de saves com link
     para o mundo, a tela do mundo com link de volta. O dono se perdeu entre
     URLs na 7A.

## Depois da 7B

- **7C — telas de gerenciamento:** meus saves, elenco, tática (formação e
  papel por slot), calendário, classificação, botão "Avançar dia" sempre
  visível. Já registrado para a 7C: "Avançar até a próxima partida" (com uma
  rodada por semana, avançar dia passa por seis dias vazios), o roteador de
  verdade e o link da partida para os saves (que invalida os cinco goldens).
- **7D — assistir a partida:** abrir o render da Fase 6 em LOD Full a partir
  do calendário e voltar. Nenhum motor novo.
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
- o WASM 3,4× mais lento que o nativo.

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
| WASM 3,4× mais lento que o nativo | Fase 8 ou 9 |
| OPFS e recarga da página nunca verificados no Safari real | verificação manual |
| Pênaltis 0,005 por partida; kickoff sobreposto; bola parada 3% | (c2) / Fase 8 |
