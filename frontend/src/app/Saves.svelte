<script lang="ts">
  // Saves (spec Fase 7A): the minimal screen over the database worker —
  // list, create, open, delete, export, import, and where the files live.
  // The real screen is Fase 7C's; this one exists to exercise persistence
  // and to let it be seen working.
  import { DbOpError, startDatabase, type Database } from '../save/client';
  import type { OpenedSave, SaveInfo, StorageInfo } from '../save/protocol';

  /** One tab at a time: the save files are held by a single worker. */
  const LOCK = 'fm-saves';
  /** Where the answer of the one `persist()` request is kept. */
  const PERSIST_KEY = 'fm.storage.persist';

  type Phase = 'starting' | 'ready' | 'other-tab' | 'failed';
  let phase: Phase = $state('starting');
  let failure = $state('');
  let storage: StorageInfo | undefined = $state();
  /** `navigator.storage.persisted()`: the browser will not evict the saves. */
  let persistent: boolean | undefined = $state();
  let saves: readonly SaveInfo[] = $state([]);
  let opened: OpenedSave | undefined = $state();
  let digest = $state('');
  let newName = $state('');
  let message = $state('');
  /** Save waiting for the second click on "Apagar". */
  let confirming = $state('');
  let db: Database | undefined;

  const forced = new URLSearchParams(location.search).get('storage') === 'idb';
  const when = (iso: string): string => new Date(iso).toLocaleString('pt-BR');

  async function refresh(): Promise<void> {
    if (db !== undefined) saves = await db.client.request('save.list', {});
  }

  /** Runs one action; a failed one is shown, not thrown. */
  async function act(what: string, action: (database: Database) => Promise<void>): Promise<void> {
    if (db === undefined) return;
    try {
      await action(db);
      message = what;
    } catch (err: unknown) {
      message = err instanceof DbOpError ? `Falhou (${err.code}): ${err.message}` : `Falhou: ${String(err)}`;
    }
    confirming = '';
    await refresh();
  }

  const create = (): Promise<void> =>
    act(`Save "${newName.trim()}" criado.`, async (database) => {
      await database.client.request('save.create', { name: newName });
      newName = '';
    });

  const open = (save: SaveInfo): Promise<void> =>
    act(`Save "${save.name}" aberto.`, async (database) => {
      opened = await database.client.request('save.open', { id: save.id });
      digest = await database.client.request('save.digest', {});
    });

  const remove = (save: SaveInfo): Promise<void> => {
    if (confirming !== save.id) {
      confirming = save.id;
      return Promise.resolve();
    }
    return act(`Save "${save.name}" apagado.`, async (database) => {
      await database.client.request('save.delete', { id: save.id });
      if (opened?.save.id === save.id) opened = undefined;
    });
  };

  const exportSave = (save: SaveInfo): Promise<void> =>
    act(`Save "${save.name}" exportado.`, async (database) => {
      const { fileName, bytes } = await database.client.request('save.export', { id: save.id });
      const url = URL.createObjectURL(new Blob([bytes], { type: 'application/vnd.sqlite3' }));
      const link = document.createElement('a');
      link.href = url;
      link.download = fileName;
      link.click();
      URL.revokeObjectURL(url);
    });

  const importSave = (save: SaveInfo, input: HTMLInputElement): Promise<void> => {
    const file = input.files?.[0];
    if (file === undefined) return Promise.resolve();
    return act(`Save "${save.name}" substituído por ${file.name}.`, async (database) => {
      try {
        const bytes = await file.arrayBuffer();
        const replaced = await database.client.request('save.import', { id: save.id, bytes }, [bytes]);
        if (opened?.save.id === replaced.id) {
          opened = await database.client.request('save.open', { id: replaced.id });
          digest = await database.client.request('save.digest', {});
        }
      } finally {
        input.value = '';
      }
    });
  };

  /**
   * Asks the browser, once, not to evict the saves. The answer is kept so
   * that the request (a permission prompt in Firefox) is not repeated on
   * every visit; what is shown is always what the browser says now.
   */
  async function askToPersist(): Promise<void> {
    const manager = navigator.storage as StorageManager | undefined;
    if (manager?.persisted === undefined) return;
    persistent = await manager.persisted();
    if (persistent || localStorage.getItem(PERSIST_KEY) !== null) return;
    localStorage.setItem(PERSIST_KEY, 'asked');
    persistent = await manager.persist();
    localStorage.setItem(PERSIST_KEY, persistent ? 'granted' : 'denied');
  }

  $effect(() => {
    let closed = false;
    /** Resolved when the screen goes away: that releases the lock. */
    let release: () => void = () => undefined;
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    const begin = async (): Promise<void> => {
      const database = await startDatabase(forced ? { storage: 'idb' } : {});
      if (closed) {
        database.terminate();
        return;
      }
      db = database;
      storage = await database.client.request('storage.info', {});
      await refresh();
      phase = storage.backend === 'none' ? 'failed' : 'ready';
      failure = storage.detail ?? '';
      void askToPersist();
    };
    const fail = (err: unknown): void => {
      phase = 'failed';
      failure = err instanceof Error ? err.message : String(err);
    };
    if (navigator.locks === undefined) {
      // No Web Locks: nothing to tell the tabs apart with; go ahead.
      begin().catch(fail);
    } else {
      navigator.locks
        .request(LOCK, { ifAvailable: true }, (lock) => {
          if (lock === null) {
            phase = 'other-tab';
            return undefined;
          }
          begin().catch(fail);
          return held;
        })
        .catch(fail);
    }
    return () => {
      closed = true;
      db?.terminate();
      db = undefined;
      release();
    };
  });
</script>

<main class="saves">
  <h1>Saves</h1>
  <p class="nav"><a href="/" data-testid="saves-to-match">← Partida (demo da Fase 6)</a></p>

  {#if phase === 'starting'}
    <p data-testid="saves-status">Abrindo o armazenamento…</p>
  {:else if phase === 'other-tab'}
    <p class="warning" data-testid="saves-other-tab">
      O jogo já está aberto em outra aba. Feche a outra aba e recarregue esta.
    </p>
  {:else if phase === 'failed'}
    <p class="warning" data-testid="saves-status">Sem armazenamento: {failure}</p>
  {:else}
    <p class="storage" data-testid="saves-storage">
      Armazenamento:
      <strong data-testid="saves-backend">{storage?.backend === 'opfs' ? 'OPFS' : 'IndexedDB'}</strong>
      {#if storage?.detail !== undefined}<span class="muted">({storage.detail})</span>{/if}
      · persistente:
      <strong data-testid="saves-persistent"
        >{persistent === undefined ? 'desconhecido' : persistent ? 'sim' : 'não'}</strong
      >
      · SQLite {storage?.sqlite}
    </p>

    <form
      class="create"
      onsubmit={(e) => {
        e.preventDefault();
        void create();
      }}
    >
      <input data-testid="saves-new-name" placeholder="Nome do novo save" bind:value={newName} />
      <button data-testid="saves-create" disabled={newName.trim() === ''}>Criar</button>
    </form>

    {#if saves.length === 0}
      <p class="muted" data-testid="saves-empty">Nenhum save ainda.</p>
    {:else}
      <table data-testid="saves-list">
        <thead>
          <tr><th>Nome</th><th>Criado</th><th>Último acesso</th><th>Schema</th><th></th></tr>
        </thead>
        <tbody>
          {#each saves as save (save.id)}
            <tr data-testid="saves-row" class:open={opened?.save.id === save.id}>
              <td data-testid="saves-name">{save.name}</td>
              <td>{when(save.createdAt)}</td>
              <td>{when(save.lastOpenedAt)}</td>
              <td>v{save.schemaVersion}</td>
              <td class="actions">
                <button data-testid="saves-open" onclick={() => open(save)}>Abrir</button>
                <button data-testid="saves-export" onclick={() => exportSave(save)}>Exportar</button>
                <label class="import">
                  Importar
                  <input
                    data-testid="saves-import"
                    type="file"
                    accept=".sqlite,application/vnd.sqlite3"
                    onchange={(e) => importSave(save, e.currentTarget)}
                  />
                </label>
                <button data-testid="saves-delete" class:danger={confirming === save.id} onclick={() => remove(save)}
                  >{confirming === save.id ? 'Confirmar' : 'Apagar'}</button
                >
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}

    {#if opened !== undefined}
      <section class="opened" data-testid="saves-opened">
        <h2>Aberto: <span data-testid="saves-opened-name">{opened.save.name}</span></h2>
        <p>
          Schema v{opened.save.schemaVersion} · tabelas: {opened.tables.join(', ')}
          {#if opened.migrated.length > 0}· migrado agora: v{opened.migrated.join(', v')}{/if}
        </p>
        <p class="muted">Conteúdo (SHA-256): <code data-testid="saves-digest">{digest}</code></p>
      </section>
    {/if}

    <p class="message" data-testid="saves-message" aria-live="polite">{message}</p>
  {/if}
</main>

<style>
  .saves {
    max-width: 60rem;
    margin: 0 auto;
    padding: 2rem 1rem;
  }
  h1 {
    font-size: 1.75rem;
    margin: 0 0 1rem;
  }
  h2 {
    font-size: 1.125rem;
    margin: 0 0 0.5rem;
  }
  .muted {
    color: var(--muted);
  }
  .nav {
    font-size: 0.875rem;
    margin: -0.5rem 0 1rem;
  }
  .nav a {
    color: var(--muted);
  }
  .warning {
    color: #ffd166;
    font-weight: 700;
  }
  .storage {
    font-size: 0.875rem;
  }
  .create {
    display: flex;
    gap: 0.5rem;
    margin: 1rem 0;
  }
  input:not([type='file']) {
    flex: 1;
    font: inherit;
    color: inherit;
    background: var(--bg);
    border: 1px solid var(--muted);
    border-radius: 4px;
    padding: 0.25rem 0.5rem;
  }
  button,
  .import {
    font: inherit;
    font-size: 0.875rem;
    color: inherit;
    background: transparent;
    border: 1px solid var(--muted);
    border-radius: 4px;
    padding: 0.125rem 0.5rem;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  button.danger {
    border-color: #ff6b6b;
    color: #ff6b6b;
  }
  /* The file input is driven through its label. */
  .import input {
    position: absolute;
    width: 1px;
    height: 1px;
    opacity: 0;
    pointer-events: none;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.875rem;
  }
  th,
  td {
    text-align: left;
    padding: 0.375rem 0.5rem;
    border-top: 1px solid rgb(255 255 255 / 12%);
  }
  thead th {
    border-top: 0;
    color: var(--muted);
    font-weight: 400;
  }
  tr.open td {
    background: rgb(255 255 255 / 6%);
  }
  .actions {
    display: flex;
    gap: 0.25rem;
    justify-content: flex-end;
    flex-wrap: wrap;
  }
  .opened {
    margin-top: 1.5rem;
    padding: 0.75rem 1rem;
    border: 1px solid rgb(255 255 255 / 12%);
    border-radius: 6px;
    font-size: 0.875rem;
  }
  code {
    font-size: 0.75rem;
    word-break: break-all;
  }
  .message {
    min-height: 1.5rem;
    margin-top: 1rem;
    font-size: 0.875rem;
  }
</style>
