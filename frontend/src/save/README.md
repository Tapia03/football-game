Persistência local (spec Fase 7A): o Worker de banco
(`../workers/db.worker.ts`) é o único que abre os arquivos de save e fala
SQL. Aqui ficam o protocolo (`protocol.ts`), o cliente (`client.ts`), os
schemas como cadeias de migrações (`schema.ts`) e a aplicação da cadeia no
open (`migrate.ts`). Os dois armazenamentos — OPFS e o fallback IndexedDB —
ficam atrás de uma interface em `files.ts`. A tela mínima é
`../app/Saves.svelte` (`?view=saves`).
