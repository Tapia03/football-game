import { mount } from 'svelte';
import App from './app/App.svelte';
import './app/global.css';

const target = document.getElementById('app');
if (target === null) {
  throw new Error('#app mount point missing from index.html');
}

export default mount(App, { target });
