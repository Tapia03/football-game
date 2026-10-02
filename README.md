# Football Game

Simulador/gerenciador de futebol que roda no navegador: motor em Rust → WASM,
frontend Svelte 5 + Vite, persistência offline-first (IndexedDB + Supabase).

Status: **Fase 0 — bootstrap**. Só existe a página "Hello from Rust".

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

npx playwright install --with-deps   # uma vez
npx playwright test                  # chromium, firefox, webkit
```

Os servidores de dev e preview enviam COOP/COEP (iguais a `frontend/public/_headers`),
então `crossOriginIsolated === true` e `SharedArrayBuffer` está disponível localmente.

## Determinismo numérico

Toda função transcendental passa por `libm` (Rust puro). A tabela
`crates/fm-core/golden/libm_f32.txt` é gerada nativamente e verificada:

- nativo: `cargo test -p fm-core`
- WASM em navegador: `wasm-pack test` e o teste E2E (`libm parity: OK` na página)

Regenerar (só se a versão de `libm` mudar de propósito):
`UPDATE_GOLDEN=1 cargo test -p fm-core libm_golden`.

## Estrutura

```
crates/        fm-core, fm-entities, fm-match, fm-economy, fm-world, fm-persistence, fm-wasm
frontend/      app Svelte (index.html, src/, public/_headers)
tests/e2e/     Playwright
tests/golden/  screenshots golden por navegador (a partir da Fase 6)
```

## Deploy

`.github/workflows/deploy.yml` publica no Cloudflare Pages. Fica inativo até os
secrets `CLOUDFLARE_API_TOKEN` e `CLOUDFLARE_ACCOUNT_ID` existirem no repositório.
Nome do projeto Pages: variável `CLOUDFLARE_PAGES_PROJECT` (padrão `football-game`).
