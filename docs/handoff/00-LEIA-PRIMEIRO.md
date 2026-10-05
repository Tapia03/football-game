# Passagem de contexto — leia primeiro

Escrito em **2026-10-05** para uma sessão nova (de outro modelo ou de outra
pessoa) conseguir continuar o projeto sem ter visto a conversa anterior.

Estes arquivos são um **resumo orientado**. As fontes de verdade continuam
sendo:

- [`docs/SPEC.md`](../SPEC.md) — arquitetura, decisões travadas e o desenho
  aprovado de cada fase (quase 3.000 linhas; é longo de propósito).
- [`docs/STATE.md`](../STATE.md) — "onde estamos", dívidas, comandos.

Se algo aqui contradisser o SPEC ou o STATE, **valem o SPEC e o STATE**, e
este arquivo deve ser corrigido.

## Ordem de leitura

1. Este arquivo.
2. [`01-o-que-e-o-projeto.md`](01-o-que-e-o-projeto.md) — o que é, stack,
   arquitetura, invariantes.
3. [`02-historico.md`](02-historico.md) — o que cada fase entregou e o que
   se aprendeu.
4. [`03-estado-atual-e-proximos-passos.md`](03-estado-atual-e-proximos-passos.md)
   — o ponto exato em que o trabalho parou e o que vem depois.
5. [`04-como-trabalhar.md`](04-como-trabalhar.md) — regras do dono do
   projeto, fluxo de commits, comandos, armadilhas do ambiente.

## O projeto em um parágrafo

Um **jogo de gerenciamento de futebol que roda inteiro no navegador**. O
motor de partida é escrito em Rust, compilado para WebAssembly, e é
**determinístico bit a bit** (mesma seed, mesmo jogo, no nativo e no WASM).
A partida é vista em 2D (WebGL2). O frontend é Svelte 5. Os saves ficam no
próprio navegador, em SQLite. O jogo ainda **não tem nome**: nos documentos,
"o jogo" ou "o projeto". Repositório: `Tapia03/football-game`.

## Onde estamos, em cinco linhas

- **Fases 0 a 6 fechadas e mergeadas**: núcleo, entidades, motor de partida,
  comportamento dos jogadores, render 2D com HUD, painel tático e câmera.
- **Fase 7 em andamento — "MVP de gerenciamento"**, em quatro sub-fases:
  - **7A (persistência local): feita e mergeada** (PR #9).
  - **7B (mundo mínimo e calendário): em andamento**, branch `fase-7b`.
  - 7C (telas de gerenciamento) e 7D (assistir a partida) não começaram.
- **Há trabalho não commitado** na `fase-7b` (o crate `fm-world`) e **uma
  decisão pendente do dono do projeto**. Ver o arquivo 03.

## A decisão que está aberta agora

A primeira temporada inteira simulada mostrou que **a formação tática decide
o campeonato, não a qualidade do elenco**, e que 4-3-3 contra 4-3-3 dá 12,5
gols por partida. Isso é defeito do motor (a corrigir na Fase 8), mas obriga
a escolher como o mundo da 7B distribui as formações. A pergunta feita ao
dono, ainda sem resposta:

1. todos os clubes em 4-4-2 (recomendado);
2. manter o sorteio entre as cinco formações;
3. sortear só entre 4-4-2 e 5-3-2.

**Não commitar o `fm-world` antes dessa resposta.** Detalhes e números no
arquivo 03.

## Três coisas que não se negociam

1. **Determinismo bit a bit** entre LODs e entre nativo e WASM. Há testes
   que travam isso; nunca "consertar" um deles ajustando o esperado.
2. **Não mexer no motor de partida durante a Fase 7.** Se o MVP precisar de
   mudança no motor: parar e levar a proposta ao dono.
3. **Desenho antes de código, e parar quando o CI fica vermelho.** O dono
   aprova o desenho de cada sub-fase; teste que falha é reportado, nunca
   contornado em silêncio. O arquivo 04 tem a lista completa.
