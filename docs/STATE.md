# STATE — estado oficial do projeto

Resumo de uma tela que sobrevive a compactações de sessão. A fonte de verdade
de arquitetura e regras é o [`docs/SPEC.md`](SPEC.md); este arquivo só diz
*onde estamos*. Atualizado em **2026-10-02**.

## Fase atual
**Fase 5 — Role Behaviors**, branch `fase-5` (sem PR aberto ainda).
- (a) `TickFrame` — feito. 37,0 ms no CI.
- (b) Modelo de defesa — feito. 75 botes/partida, 19,4 faltas.
- Otimização de performance (cache de voo + trajetória compartilhada) —
  feito. **41,2 ms no CI** (meta 40, gate 50).
- (c) Chutes + passes — dividido em **(c1)** (modelo de criação de
  jogadas, escopo B) e **(c2)** (calibração).
  - (c1) item 1 (xG, cobertura, domínio): feito, 566,6M instruções.
  - (c1) item 2 (xT + moeda comum + cadência de decisão): feito, 540,5M.
  - (c1) item 3 (linha de impedimento): feito, 540,2M; calculada só nos
    ticks com corredor (desvio documentado no SPEC).
  - (c1) item 4 (estado `Run`): feito, 535,6M (−0,85%).
  - Próximo: item 5 (passe em profundidade).
- (d) Comportamentos por papel — não iniciado.

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
  (≤ ~593M). Hoje: 540,5M ≈ 41 ms.
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
