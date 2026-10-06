# Football Game

Simulador/gerenciador de futebol que roda no navegador: motor em Rust → WASM,
frontend Svelte 5 + Vite, saves locais em SQLite (OPFS, com fallback para
IndexedDB).

Status: **Fase 7B — mundo mínimo e calendário**, em andamento (ver
[`docs/STATE.md`](docs/STATE.md); quem chega agora começa por
[`docs/handoff/`](docs/handoff/00-LEIA-PRIMEIRO.md)). A página sem `?view`
mostra uma partida ao vivo: o motor roda num Web Worker, publica snapshots num
`SharedArrayBuffer` e a main thread desenha (WebGL2) e mostra o HUD.

## Pré-requisitos

- Rust stable + target `wasm32-unknown-unknown` (o `rust-toolchain.toml` cuida disso)
- [`wasm-pack`](https://github.com/drager/wasm-pack) (`cargo install wasm-pack`)
- Node 22+

## Comandos

```sh
npm ci
npm run dev        # wasm-pack build + Vite em http://localhost:5173
npm run build      # wasm-pack build + svelte-check + bundle de produção em dist/
npm run preview    # serve dist/ em http://localhost:4173

cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
wasm-pack test --headless --chrome crates/fm-wasm

npx playwright install               # uma vez (no CI/Linux: --with-deps)
npx playwright test                  # chromium, firefox, webkit
```

Os servidores de dev e preview enviam COOP/COEP (iguais a `frontend/public/_headers`),
então `crossOriginIsolated === true` e `SharedArrayBuffer` está disponível localmente:
o worker do motor e o anel de snapshots funcionam tanto em `npm run dev` quanto em
`npm run preview`, sem configuração extra.

- **Porta ocupada:** as portas são fixas (`strictPort`). Se já houver outro servidor
  na 5173, o `npm run dev` não sobe; use outra porta: `npx vite --port 5174`
  (depois de um `npm run wasm`).
- **`?seed=N`** na URL escolhe a partida de demonstração (padrão 7).
- **Golden de pixel:** `tests/golden/chromium/` guarda imagens produzidas pelo
  Chromium do CI (Linux). Fora do CI o teste de golden é pulado, porque fontes e
  rasterização de outra máquina diferem (`FM_GOLDEN=1` força a comparação). Os
  demais testes e2e valem em qualquer lugar. Para atualizar uma referência, ver
  "Golden de pixel" em `docs/STATE.md`.
- **e2e locais:** fora do CI o Playwright usa 2 workers (as páginas desenham com
  WebGL por software; mais que isso satura a máquina).

## Determinismo numérico

Toda função transcendental passa por `libm` (Rust puro). A tabela
`crates/fm-core/golden/libm_f32.txt` é gerada nativamente e verificada:

- nativo: `cargo test -p fm-core`
- WASM em navegador: `wasm-pack test` e o teste E2E (`libm parity: OK` na página)

Regenerar (só se a versão de `libm` mudar de propósito):
`UPDATE_GOLDEN=1 cargo test -p fm-core libm_golden`.

## Estrutura

```
crates/        fm-core, fm-entities, fm-match, fm-economy, fm-world, fm-persistence,
               fm-render (malha e backend WebGL2), fm-wasm (bindings, anel de snapshots)
frontend/      app Svelte (index.html, src/, public/_headers)
tests/e2e/     Playwright
tests/golden/  screenshots golden por navegador (a partir da Fase 6)
```

## Deploy

`.github/workflows/deploy.yml` publica no Cloudflare Pages. Fica inativo até os
secrets `CLOUDFLARE_API_TOKEN` e `CLOUDFLARE_ACCOUNT_ID` existirem no repositório.
Nome do projeto Pages: variável `CLOUDFLARE_PAGES_PROJECT` (padrão `football-game`).
