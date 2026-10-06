# 02 — Histórico: como chegamos aqui

Resumo por fase. O detalhe de cada uma (desenho, medições, o que foi tentado
e revertido) está no SPEC, Seção 5, e no STATE.

## Fases 0 a 4 — fundação e motor

| Fase | Entrega |
|---|---|
| 0 | Bootstrap: workspace Rust, Vite + Svelte, CI, deploy, página "hello" que prova WASM e paridade da libm |
| 1 | `fm-core`: geometria, campo, RNG, matemática via libm |
| 2 | `fm-entities`: jogadores em SoA, cinco blocos de atributos, estado dinâmico em ponto fixo, `weekly_update`, gerador determinístico |
| 3 | Espaço relativo ao time, formações, fases de jogo, âncoras |
| 4 | **Fase-chave:** tick lógico, `DecisionSystem`, `ActionResolver`, bola analítica, eventos, os três LODs e a paridade bit a bit |

PRs #2, #4 e #5.

## Fase 5 (c1) — comportamento e física

Foi a fase mais longa e a que mais ensinou. Começou como "papéis dos
jogadores" e virou uma sequência de diagnósticos do motor.

O que ficou (PR #6):

- **Física de movimento** com reação, aceleração e giro.
- **Modelo de defesa:** contenção por zona, cobertura, decisão de bote.
- **Correção da reação em `steer`**, que fez o acerto de passe subir de 59%
  para 68,5%.
- **Reinícios** dentro das linhas (antes, quase metade era perdida na hora).
- Ferramentas de medição em `crates/fm-match/examples/`.

Números do motor ao fim da fase, em 180 partidas (4-4-2 contra 4-3-3):

| Métrica por partida | Motor | Real (aprox.) |
|---|---|---|
| Passes (acerto) | 970 (68,5%) | 900 (80%) |
| Botes / faltas | 70,8 / 19,4 | 70 / 22 |
| Chutes (no alvo) | 26,4 (8,1) | 25 (9) |
| Gols | 4,43 | 2,7 |
| Saída forçada | 52–53% | — |

O que **não** foi feito, por decisão do dono: (c2) calibração e (d)
comportamentos por papel (Overlap, Counter Attack, Tight Marking, Offside
Trap).

### A lição que virou regra

Quatro defeitos apareceram em sequência, cada um mais fundo que o anterior,
e todos se apresentaram primeiro como "problema de decisão ou de
calibração":

1. passe em profundidade (a estimativa supunha uma inércia que a execução
   não tinha);
2. marcação de corredores (tentada e revertida: custo 2× o estimado);
3. reinícios perdidos;
4. reação recomeçando a cada tick.

**Regra:** diante de um sintoma, perguntar primeiro "é modelo ou é
constante?", e medir a camada de baixo (execução, física) de fora, tick a
tick, antes de mexer em número.

### Um patch pronto e não aplicado

`docs/patches/intercept-point-height.patch` corrige o passe pelo alto (21% →
57% de acerto), mas faz os gols subirem para 8,5 por partida, porque falta
quem defenda a bola longa. Fica para a Fase 8.

## Fase 6 — render (PR #7)

Fase "visual + controle", em quatro partes:

- **6A — infraestrutura:** motor no Worker, anel de snapshots em
  `SharedArrayBuffer`, interpolação em TypeScript, malha como função WASM
  pura. Latência tick → desenho: ~19 ms.
- **6B — HUD e overlays:** placar, relógio, cartões, posse, painel de
  estatísticas (com xG relido de fora do tick e testado bit a bit), F1
  rótulos, F2 vetores de velocidade, F3 linha de impedimento pela regra, F4
  linhas de formação.
- **6C — painel tático:** mentalidade trocada ao vivo, pressing em versão
  mínima, "tempo" visível e desabilitado. Critérios medidos: Defensive →
  Attacking sobe o bloco pelo menos 10 m em 5 s; o pressing muda a distância
  ao portador já no primeiro segundo.
- **6D — câmera:** campo inteiro, meio campo seguindo a bola (zoom 2×, com
  zona morta) e tática (zoom-out com rótulos e linhas automáticos). Tecla
  `C` cicla, `1`/`2`/`3` vão direto. Velocidades 1×, 2×, 5×, 10×, 30×, 60×.

Cinco goldens de pixel em `tests/golden/chromium/`.

Achado do 6C que vale lembrar: o pressing "muito alto" **não** é
estritamente melhor mesmo sem fadiga — em 12 partidas sofreu 6,58 gols
contra 2,67 no nível médio. Amostra pequena; é estimativa.

## A virada de ordem (2026-10-05)

Depois da Fase 6 a ordem combinada era: motor ("Bola longa") antes da
interface. O dono **inverteu**:

> depois de tudo isso, o projeto ainda não é um jogo de gerenciamento
> jogável — é um motor de partida com um spike visual.

A ordem que vale:

1. **Fase 7 — MVP de gerenciamento** (em andamento).
2. **Fase 8 — Bola longa + contraparte defensiva** (motor).
3. **Fase 9 — Resto do gerenciamento** (mercado, contratos, finanças, ligas
   múltiplas, copas; e também PWA e demo/deploy).
4. **Fase 10 — Polimento e comunidade** (packs, auth, sync).

Os três motivos: provar que é um jogo de gerenciamento; descobrir os
problemas estruturais (persistência, custo de simulação, avançar dia,
Worker ↔ Worker) enquanto o código é pequeno; e ver o motor no contexto
real para orientar o próximo refinamento.

O SPEC ainda tem títulos com a numeração antiga abaixo da Fase 6; há uma
tabela de renumeração no começo da Seção 5.

## Fase 7A — persistência local (PR #9)

Entregue em cinco commits: SPEC; Worker de banco com SQLite sobre
`opfs-sahpool`; fallback IndexedDB; export, import e troca atômica; tela
mínima de saves.

O que o CI mostrou:

| | Chromium | Firefox | WebKit (Playwright) |
|---|---|---|---|
| OPFS | sim | sim | **não** (sem `navigator.storage`) |
| Testes sobre OPFS | passam | passam | pulados |
| Testes sobre IndexedDB | passam | passam | passam |

Decisões que mudaram o SPEC:

- A fonte de verdade local passou a ser **o arquivo SQLite do save** (era
  "IndexedDB", decisão da Fase 0). O sync por chunk será redesenhado na
  Fase 10.
- **O IndexedDB não é caminho de segunda classe.** A Fase 10 deve assumir
  IndexedDB como backend primário no Safari. Ressalva registrada: isso foi
  medido no WebKit do Playwright; ninguém verificou num Safari real.

Três coisas saíram diferentes do desenho, todas aprovadas depois:

- não existe arquivo `.tmp`: o que decide se um arquivo é save ou órfão é a
  referência no catálogo;
- o Worker atende um pedido por vez, em fila;
- o cliente tenta de novo ao iniciar o Worker, porque o WebKit do Playwright
  recusa carregar o mesmo script duas vezes em seguida.

Depois do merge entraram, na `fase-7b`: `view` desconhecido avisa em vez de
mostrar a partida (`?view=save` sem o "s" mostrava a partida e confundiu o
dono), link da tela de saves para a partida, e tolerância de ±2 por canal no
smoke test do WebGL2.

## Fase 7B — em andamento

Ver o arquivo 03: é onde o trabalho está parado. O que já aconteceu:

- **`fm-world` nativo.** A primeira temporada inteira mostrou que a formação
  decidia o campeonato (4-3-3 × 4-3-3 com 12,5 gols por partida); o dono
  fixou todos os clubes em 4-4-2 e o defeito foi para a Fase 8.
- **Persistência do mundo:** codecs em `fm-persistence`, migração v2 (a
  primeira de verdade, testada com um save v1 exportado pelo código da 7A),
  operações `world.*`. Save com mundo: 160 kB.
- **Worker de mundo**, com progresso e cancelamento. A medição no CI deu
  1.453 ms por rodada no Chromium (4 núcleos), acima do limite de 1 s
  combinado: o pool virou obrigatório.
- **Pool de Workers de partida (7B.4b).** Mesmo save com 0, 1, 2 e 4
  workers. Na máquina do dono: 1.676 ms → 567 ms com 10 workers (a meta de
  300 ms não foi atingida; dívida da Fase 8).
- **Incidente do GitHub (2026-10-05):** runners em fila, jobs cancelados sem
  rodar; normalizou no dia seguinte.
- **Pool medido no CI (2026-10-06):** 435 a 674 ms por rodada no Chromium
  com pool de 4, contra 939 a 1.495 ms sem pool. Aprovado.
- **Pool observável (7B.4c).** Na primeira medição, o WebKit do CI teve um
  Worker de partida mudo por 30 s. A investigação pôs confirmação de
  recebimento, escuta de erros e prazo adaptativo no pool, e achou a causa
  em três passos: não era o prazo; o `catch` do Worker escondia o erro
  original atrás de um `free()` que lançava outro; e o erro original era
  **um trap dentro do motor** (`TickFrame::compute_anchors`), só no WebKit
  do CI. O dono encerrou ali: o trap foi para a Fase 8, sem tocar no motor.
  Lição: um catch que limpa antes de reportar pode esconder o que importa.
