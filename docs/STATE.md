# STATE — estado oficial do projeto

Resumo de uma tela que sobrevive a compactações de sessão. A fonte de verdade
de arquitetura e regras é o [`docs/SPEC.md`](SPEC.md); este arquivo só diz
*onde estamos*. Atualizado em **2026-10-04** (fechamento de (c1) da Fase 5; PR `fase-5` → `main` aberto, aguardando merge).

## Fase atual — Fase 5 (c1) FECHADA, PR aberto
**Fase 5 — Role Behaviors**, branch `fase-5`. O usuário decidiu (2026-10-04)
fechar (c1) no motor do 5D-2 (`f17b1d9`) e abrir o PR `fase-5` → `main`.
**Depois do merge: Fase 6 (render completo).** Antes de começar a Fase 6,
verificar que o PR foi mergeado (SPEC, Seção 0, item 5).

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

### Fase futura (sem prazo): "Bola longa + contraparte defensiva"
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
- **Custo:** 590,4M instruções (590.430.506, commit `f17b1d9`) ≈ 44,8 ms:
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
| `main` | o que está mergeado (até a Fase 4, `d0a6823`) |
| `fase-5` | branch de trabalho; PR só no fim da fase |
| `fase-6-v0` | spike de render sobre o motor de antes da física; **nunca mergeia** |
| `spike-render-v2` | render da `fase-6-v0` sobre o motor de `5d34253` (só arquivos de render, motor e `docs/` intactos); **nunca mergeia** |
| `spike-render` | primeiro spike (frame estático); **nunca mergeia** |

### Comandos para retomar
```sh
git checkout fase-5
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
# Spike visual: push em qualquer branch spike-* dispara o deploy
# (.github/workflows/deploy.yml) e publica em
# https://<branch>.football-game-b5k.pages.dev
```
**Regra do spike local (2026-10-03):** o usuário vê o jogo no worktree
`../football-game-spike` (branch `spike-render-v2`, `npm run dev` em
`http://localhost:5173`). Sempre que ele pedir para atualizar o spike:
(1) `git merge fase-5` nesse worktree; (2) `npm run wasm` nele; (3)
informar qual commit da `fase-5` está rodando. Sem isso o navegador mostra
um motor antigo sem avisar. O deploy remoto (`spike-render-v2` no
Cloudflare) só muda com push, que é pedido à parte.

Para atualizar o spike com um motor novo: refazer `spike-render-v2` a
partir da `fase-5` e copiar só os arquivos de render da `fase-6-v0` (ver o
commit `d6f433a`).

## PRs
| PR | Conteúdo | Estado |
|---|---|---|
| Tapia03/football-game#1 | Fases 0 e 1 | mergeado |
| Tapia03/football-game#2 | Fase 2 | mergeado |
| Tapia03/football-game#4 | Fase 3 + SPEC v2.1 + glow | mergeado |
| Tapia03/football-game#5 | Fase 4 | mergeado |
| — | Fase 5 (`fase-5`) | aberto em 2026-10-04 ((c1) fechada); aguardando merge |

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
- Spike visual atual (motor de `5d34253`):
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
