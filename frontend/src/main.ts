import { mount } from 'svelte';
import App from './app/App.svelte';
import Saves from './app/Saves.svelte';
import './app/global.css';
import { DbClient, startDatabase } from './save/client';

const target = document.getElementById('app');
if (target === null) {
  throw new Error('#app mount point missing from index.html');
}

// Test hook (Fase 7A): the e2e tests start the database worker themselves,
// with the options each one needs (on the blank view, where nothing else
// holds the save files).
(globalThis as { fmSave?: unknown }).fmSave = { startDatabase, DbClient };

// `?view=saves`: the saves screen. `?view=blank`: nothing at all — a quiet
// page for tests that drive a worker directly. Otherwise, the match.
const view = new URLSearchParams(location.search).get('view');

function blank(into: HTMLElement): void {
  const note = document.createElement('p');
  note.dataset['testid'] = 'blank';
  note.textContent = 'Página em branco (testes).';
  into.append(note);
}

export default view === 'saves'
  ? mount(Saves, { target })
  : view === 'blank'
    ? blank(target)
    : mount(App, { target });
