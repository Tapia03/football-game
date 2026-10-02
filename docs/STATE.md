# STATE — estado oficial do projeto

Resumo de uma tela que sobrevive a compactações de sessão. A fonte de verdade
de arquitetura e regras é o [`docs/SPEC.md`](SPEC.md); este arquivo só diz
*onde estamos*. Atualizado em **2026-10-02** (pausa por orçamento, fim da sessão).

## Fase atual — PAUSADA (orçamento)
**Fase 5 — Role Behaviors**, branch `fase-5` (sem PR aberto). Ponto de
retorno: tag `pre-pause-fase5-item5` (este commit de STATE). O código é o
de `5d34253`: CI verde em todos os jobs, inclusive paridade WASM e bench.

### Onde exatamente paramos
Há **duas numerações de "passo"** diferentes. Não confundir:
- **Passos do Caminho A** (a ordem de (c1), abaixo). Paramos no **passo 2**
  (defesa ajustada à física): o engajamento está feito; falta **acompanhar
  corredores** e **goleiro saindo do gol**.
- **Passos do item 5** (passe em profundidade). Passo 1 (`lofted_lane`)
  ligado; passo 2 (tempo de chegada + corredor legal) implementado e
  **desligado**. O item 5 não andou desde então: ele é refeito no **passo 4
  do Caminho A**, sobre a física nova.

A física no lance e o engajamento (`5d34253`) são os **passos 1–2 do
Caminho A**, não o passo 2 do item 5.

### Caminho A (ordem aprovada)
1. Física de movimento — **feito** (`8e5e7e8` + `5d34253`): reação,
   aceleração, giro e chegada (15 m/s² / 80 ms / 15 rad/s), só para quem
   está no lance; o resto arcade na cadência 3.
2. Defesa ajustada à física — **em andamento.** Feito: engajamento (o
   escolhido pela nota do bote, a até 4,5 m, parte para cima; o bote sai a
   1,8 m). **RETOMAR AQUI:** acompanhar corredores (com o equilíbrio
   corrida × apoio) e goleiro saindo do gol. Regra de custo: +1,5% por
   item, de volta.
3. Item 9: construção desde a defesa.
4. Refazer o item 5 (passe em profundidade) sobre a física nova.
5. Itens 6–8 (apoio sem bola → drible 1×1 → tabela).
Depois: (c2) calibração, (d) comportamentos por papel, PR `fase-5` → `main`.

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
| (este) | docs: consolida estado no fim da Fase 5 (item 5 parcial) |
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
- **Defesa acompanha corredores + equilíbrio corrida × apoio** e **goleiro
  saindo do gol**: o resto do passo 2 do Caminho A.
- **Item 9** (construção desde a defesa) e **itens 6–8**: só a ordem e o
  escopo.
- **Valores realistas da física** (4,5 m/s² / 200 ms / 6 rad/s): no
  código estão os conservadores (15 / 80 / 15).
- **Apito de impedimento** e **regra do kickoff**: fases próprias depois.

### Implementado, mas desligado
- Passe em profundidade (`ValueTuning.through_balls = false`).
- Contadores de diagnóstico (só com a feature `diagnostics`).

### Dívidas conhecidas
- **Custo:** 596,3M instruções, **3,3M acima do teto de saída de (c)
  (593M ≈ 45 ms)**. Devolver antes de fechar (c).
- **Teste frágil:** `match_statistics_are_plausible` (≥ 3 faltas por
  partida) usa só 6 seeds e mede **6,0**; a média de 180 partidas é
  **3,6**. As 6 seeds não representam a média.
- **Acerto de passe 56%** (real ~80%): invariante 18 do lado do passe →
  Caminho A passo 4 ou (c2).
- **Botes 10 / faltas 3,6** (real ~70 / 22): calibração em (c2).
- **Posse do mandante não convergida:** 73% (arcade) → 56,9% (física) →
  42,5% (física + engajamento). Troca de lado a cada mudança de movimento.
- **Gols 5,3** por partida (real ~2,7); xG por chute 0,23 (real ~0,10).

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
| — | Fase 5 (`fase-5`) | abre quando a fase fechar com CI verde |

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
- **Critério de saída de (c):** ≤ 45 ms, medido em instruções pela régua
  (≤ ~593M). Hoje: 596,3M ≈ 45,2 ms (3,3M de dívida a devolver antes de
  fechar (c)).
- **xT:** conferir a cópia contra `karun.in/blog/data/open_xt_12x8_v1.json`.

## Marcos
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
- **Vermelhos** 0,53/partida (real 0,15) e **pênaltis** 0,02 (real 0,3).
  Alvo: Fase 6.
- **Tempo de bola parada** 4% (real ~35%): reinícios curtos demais. Entra em
  (c2).
- **Impedimento não apitado** (decisão): infla gols em profundidade. Viés
  conhecido, não corrigir em (c2); a solução é a fase do apito.
