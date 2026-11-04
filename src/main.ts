import './style.css';
import { Game } from './game/Game';
import { buildPlayground } from './game/world';
import { prepareBattlefield, type WorldFactory } from './game/sm64World';

declare global {
  interface Window {
    __JUMPMAN__?: Game;
  }
}

const start = document.getElementById('start-button') as HTMLButtonElement;
start.disabled = true;
start.textContent = 'LOADING COURSE…';
let factory: WorldFactory = buildPlayground;
let worldName = 'playground';
let loadFailed = false;
if (new URLSearchParams(location.search).get('level') !== 'playground') {
  try {
    factory = await prepareBattlefield();
    worldName = 'bob';
  } catch (error) {
    console.warn('[jumpman] battlefield unavailable; using playground', error);
    loadFailed = true;
  }
}
const game = new Game(factory, worldName);
window.__JUMPMAN__ = game;
start.disabled = false;
start.textContent = 'START PLAYING';
(document.getElementById('course-name') as HTMLElement).textContent = worldName === 'bob' ? 'Bob-omb Battlefield' : 'Movement Playground';
(document.getElementById('star-ready') as HTMLElement).textContent = worldName === 'bob' ? '★ STAR READY — REACH THE SUMMIT ★' : '★ STAR READY — CLIMB THE TOWER ★';
if (loadFailed) (document.getElementById('load-note') as HTMLElement).textContent = 'SM64 course unavailable. The movement playground is ready instead.';


// Keep the canvas from stealing focus outlines.
document.getElementById('game')?.addEventListener('pointerdown', (event) => {
  (event.currentTarget as HTMLElement).focus?.();
});
