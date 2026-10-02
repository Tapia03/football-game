# STATE — estado oficial do projeto

Resumo de uma tela que sobrevive a compactações de sessão. A fonte de verdade
de arquitetura e regras é o [`docs/SPEC.md`](SPEC.md); este arquivo só diz
*onde estamos*. Atualizado em **2026-10-02** (pausa por orçamento).

## Fase atual — PAUSADA (orçamento)
**Fase 5 — Role Behaviors**, branch `fase-5` (sem PR aberto). Ponto de
retorno: tag `pre-pause-2026-10` (o commit deste STATE; o código é o de
`5d34253`, com CI verde em todos os jobs, inclusive paridade WASM e bench).

### Onde cada parte está
- (a) `TickFrame` — feito. (b) Modelo de defesa — feito. Otimização (cache
  de voo + trajetória compartilhada) — feita.
- (c) dividido em (c1) modelo de criação de jogadas e (c2) calibração.
  - (c1) item 1 (xG, cobertura, domínio): **pronto**.
  - (c1) item 2 (xT + moeda comum + cadência 3): **pronto**.
  - (c1) item 3 (linha de impedimento, calculada só nos ticks com
    corredor): **pronto**.
  - (c1) item 4 (estado `Run`): **pronto**.
  - (c1) item 5: **parcial/pendente**. Passo 1 (`lofted_lane`) ligado;
    passo 2 (passe em profundidade) commitado **desligado**
    (`through_balls = false`, acerto ≈ 0,3%). Refazer na física nova.
  - (c1) itens 6–8: **não iniciados**.
  - (c1) item 9 (construção desde a defesa): **previsto** (Caminho A).
- (c2) calibração e (d) comportamentos por papel — não iniciados.

### Caminho A (ordem aprovada) e onde paramos
1. Física de movimento — **feito** (commit 1 `8e5e7e8` + commit 2
   `5d34253`): física (15 m/s² / 80 ms / 15 rad/s) só para quem está no
   lance; o resto arcade na cadência 3.
2. Defesa ajustada à física — **em andamento**. Feito: engajamento (o
   escolhido pela nota do bote, a até 4,5 m, parte para cima; o bote sai a
   1,8 m). **Paramos aqui.** Falta: acompanhar corredores e goleiro saindo
   do gol. **Retomar por aqui, com a regra de +1,5% por item de volta.**
3. Item 9: construção desde a defesa.
4. Refazer o item 5 em cima da física nova.
5. Itens 6–8.
Depois: (c2), (d), PR `fase-5` → `main`.

### Commits na `fase-5` (sobre `main`, mais recente primeiro)
| Hash | Resumo |
|---|---|
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

### Não commitado
- Árvore de trabalho limpa; **nenhum stash**.
- Patches de experimento só no scratchpad da sessão (efêmero, **some com
  o container**): física completa rejeitada (+37%) e variantes do item 5.
  Nada neles é necessário: o código aprovado está commitado e os números
  dos experimentos estão no SPEC.

### Dívidas
- **Explícita (custo):** 596,3M instruções, **3,3M acima do teto de saída
  de (c) (593M ≈ 45 ms)**. Precisa ser devolvido antes de fechar (c).
- **Frágil (teste):** `match_statistics_are_plausible` exige ≥ 3 faltas
  por partida. Nas 6 seeds do teste mede **6,0** (margem 3,0); na média de
  180 partidas a taxa é **3,6** (margem 0,6). As 6 seeds estão acima da
  média, e um item que baixe as faltas pode quebrar o teste sem aviso.

### Sinais pendentes para (c2) (não calibrar antes)
- **Posse do mandante não convergida:** 73% (arcade) → 56,9% (física) →
  42,5% (física + engajamento). Troca de lado a cada mudança de movimento.
- **Acerto de passe 56%** (real ~80%): invariante 18 do lado do passe →
  passo 4 do Caminho A ou (c2).
- **Botes/faltas 10 / 3,6** (real ~70 / 22).

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
