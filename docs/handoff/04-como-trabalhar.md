# 04 — Como trabalhar neste projeto

## As regras do dono

Foram ditas ao longo das sessões e continuam valendo. Nas palavras dele,
quando há citação.

### Antes de escrever código

- **Desenho antes de código.** Cada sub-fase começa com o desenho; "Não
  codifica antes de eu aprovar".
- **SPEC antes do código.** O desenho aprovado entra no SPEC num commit
  próprio, antes da implementação.
- **Uma pergunta por vez.** Se algo estiver ambíguo, perguntar antes de
  codar — e uma pergunta, não uma lista.
- **Calibração:** "é modelo ou constante? Se for modelo, reescopa antes de
  calibrar."

### Enquanto escreve

- **Uma tarefa por vez, testar antes de commitar.**
- **Um push por commit lógico.** Esperar o CI ficar verde antes do próximo.
- **Não mexer no motor na Fase 7.** Se o MVP precisar: parar e levar a
  proposta.
- **Dados sintéticos.** A base real de jogadores não entra antes da Fase 9.
- O jogo não tem nome: "o jogo" ou "o projeto".

### Quando parar

- **CI vermelho: parar.** Reportar o que falhou, com a hipótese. Não
  corrigir em silêncio. (Vermelho *esperado*, como golden sem referência,
  não conta — mas tem de ser só o esperado.)
- **Custo do tick acima de +1,5%: parar** e trazer o perfil.
- **Algo diferente do desenho aprovado: parar e reportar antes de
  commitar**, mesmo que pareça detalhe.
- **Teste que falha é informação.** Nunca ajustar o esperado para passar.
- Na dúvida entre escolher sozinho e perguntar, em coisa que muda o estado
  do jogo: "PARA e reporta. Não escolhe sozinho."

### Relatórios

O dono lê os números. Ao fim de cada sub-fase ele espera: o que entrou, o
que o CI mediu (bench, latência, custo por quadro), o que saiu diferente do
desenho e o que não foi possível verificar. Dizer com clareza o que **não**
foi testado.

### Coisas que são dele

- Mergear PRs. Abrir PR só quando ele pedir ou quando a sub-fase estiver
  completa.
- A integração "Workers Builds" da Cloudflare, que aparece vermelha em todo
  PR: ele vai desligar no painel. **Não mexer no repositório por causa
  dela.**
- As branches `fase-6-v0`, `spike-render` e `spike-render-v2`.
- Processos e servidores na máquina dele: não matar o que não foi iniciado
  pela própria sessão.

## Fluxo de uma sub-fase

1. Conferir que o PR anterior foi mergeado **no GitHub** (`gh pr view N`),
   não só que ele disse que foi. Já aconteceu de não estar.
2. `git fetch` e criar `fase-7x` a partir de `origin/main`.
3. Commit 1: SPEC (e STATE) com o desenho aprovado.
4. Commits de código, um push cada, CI verde entre eles.
5. Último commit: medições no SPEC e no STATE.
6. Abrir o PR `fase-7x → main` com descrição: o que entra, o que o CI
   mostrou, o que **não** entra, o que falta verificar.
7. Esperar o merge.

Mensagens de commit em português, no formato
`fase-7b (7B.2): o que mudou`. A `main` é a base dos PRs. (A branch padrão
do repositório no GitHub era `claude/fervent-meitner-r4q36d`; o dono disse
que ia trocar para `main`. Conferir a base ao abrir PR.)

## Comandos

```bash
# Rust
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p fm-match --all-features --all-targets -- -D warnings
cargo clippy -p fm-wasm --target wasm32-unknown-unknown -- -D warnings
cargo test --workspace

# Temporada inteira no nativo (só quando pedido)
cargo test --release -p fm-world --test season -- --ignored --nocapture

# Frontend
npm run wasm        # wasm-pack → frontend/src/engine-bridge/pkg
npm run check       # svelte-check, falha com aviso
npm run dev         # wasm + Vite na porta 5173
npm run build       # wasm + check + bundle em dist/
npm run preview     # serve dist/ na porta 4173
npx playwright test # e2e nos três navegadores
```

Ler o CI pela linha de comando, nunca pedir ao dono para ler:

```bash
gh run list --workflow CI --commit <SHA COMPLETO>
gh run watch <id> --exit-status
gh run view <id> --log | grep "baseline\|\[6A latency"
gh run view <id> --log-failed
```

`gh run list --commit` só funciona com o SHA completo.

## CI

Jobs: `test-rust` (fmt, clippy, testes), `stable-canary` (informativo),
`bench` (callgrind + gate de +1,5%), `test-wasm`, `test-e2e` para cada
navegador. O deploy gera um preview por branch em
`https://<branch>.football-game-b5k.pages.dev`.

O que cada navegador do CI consegue:

| | Chromium | Firefox | WebKit |
|---|---|---|---|
| WebGL | sim (software, determinístico) | **não** | sim |
| Golden de pixel | **sim** | não | não |
| OPFS | sim | sim | **não** |

Por isso: testes de desenho pulam o Firefox; golden só no Chromium; testes
de OPFS pulam o WebKit, que roda o fallback IndexedDB.

Gates de tempo (latência, custo por quadro) valem no Chromium. O WebKit do
CI oscila mais que os limites e só reporta no log.

## Goldens de pixel

A referência tem de ser a imagem que o Chromium **do CI** produz. Fluxo:

1. No **mesmo commit** da mudança visual, remover as referências afetadas
   (`git rm tests/golden/chromium/...`). O push fica vermelho no Chromium,
   de propósito.
2. Baixar o artefato: `gh run download <id> -n golden-chromium`.
3. Commitar as imagens novas. Push verde.

Enquanto a referência antiga existir, o teste para na primeira comparação e
o artefato devolve a própria referência antiga.

Qualquer elemento novo na página da partida invalida os cinco goldens.
Localmente o teste de golden é pulado (rodar com `FM_GOLDEN=1` se precisar).

## Armadilhas do ambiente (Windows)

- **Os arquivos têm CRLF.** Edições por `sed`/`perl` de uma linha quebram
  com frequência. O que funcionou: escrever um arquivo de edições e aplicar
  com um script de substituição exata (casa uma vez, preserva CRLF).
- **Playwright local trava ao encerrar** quando é ele que sobe o servidor de
  preview: os testes terminam, o processo não sai e o resumo não é impresso.
  Contorno: deixar um `npm run preview` no ar na 4173 (ele reaproveita) e
  rodar com a saída num arquivo de log.
- **Playwright local é instável com muita coisa rodando.** Uma aba aberta na
  página da partida disputa CPU com os testes e derruba os de tempo. Os três
  navegadores juntos às vezes dão timeout; o CI é o juiz.
- **`?view=` errado.** `?view=saves` é a tela de saves; valor desconhecido
  agora mostra um aviso.
- **A tela de saves segura um lock de aba única.** Testes que dirigem o
  Worker de banco direto usam `?view=blank`.
- **Svelte 5:** não chamar uma variável de `state` (conflita com a rune
  `$state`).
- **MSRV 1.80:** sem `Option::is_none_or` e outras APIs novas.
- **Clippy pedante com `-D warnings`:** casts precisam de `try_from` ou de
  uma justificativa; `powf`, `sin`, `exp` da std são proibidos (usar
  `fm_core::math`).

## Ganchos de teste na página

Expostos em `globalThis`: `fmMatch` (controle da partida e leitor do anel),
`fmReferenceSlot(seed, tick, comandos?)` (o slot que um motor novo
produziria), `fmLatency`, `fmPerf`, `fmCamera`, e `fmSave`
(`startDatabase`, `DbClient`).

## Como reportar um achado do motor

O MVP existe, entre outras coisas, para achar defeitos do motor. Quando
aparecer um (como o das formações):

1. medir com números, em mais de uma seed;
2. separar o que é do motor do que é do código novo;
3. **não corrigir no motor**;
4. levar ao dono com a tabela, as opções e uma recomendação;
5. depois da decisão, registrar no SPEC e no STATE como item da Fase 8.
