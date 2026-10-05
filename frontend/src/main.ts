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
// with the options each one needs.
(globalThis as { fmSave?: unknown }).fmSave = { startDatabase, DbClient };

// `?view=saves`: the saves view, without the match.
const view = new URLSearchParams(location.search).get('view');

export default view === 'saves' ? mount(Saves, { target }) : mount(App, { target });
