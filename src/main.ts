import './style.css';
import { Game } from './game/Game';
import { COURSES, courseId, type CourseId } from './game/courses';
import { preparePlayground } from './game/sm64Playground';
import { buildPlayground } from './game/world';
import { prepareCourse, type WorldFactory } from './game/sm64World';
import { prepareMushroom2D } from './game/mushroom2d';

declare global {
  interface Window {
    __JUMPMAN__?: Game;
  }
}

const start = document.getElementById('start-button') as HTMLButtonElement;
start.disabled = true;
start.textContent = 'LOADING COURSE…';
let factory: WorldFactory = buildPlayground;
let worldName: CourseId = 'playground';
const requestedParams = new URLSearchParams(location.search);
const picker = document.querySelector<HTMLFieldSetElement>('.character-picker')!;
const choices = picker.querySelectorAll<HTMLInputElement>('input');
const lede = document.querySelector<HTMLElement>('.menu-copy .overlay-lede')!;
const marioIntro = lede.textContent;
function showChoice(name: string) {
  for (const input of choices) input.checked = input.value === name;
  lede.textContent = name === 'link'
    ? 'Run off ledges to jump. Press Space to roll, J to swing the Master Sword, K to shield, and L to focus on an enemy.'
    : marioIntro;
}
showChoice(requestedParams.get('character') ?? 'sm64');
const requested = courseId(requestedParams.get('level'));
const requestedAct = Math.max(1, Number(requestedParams.get('act') ?? 1) || 1);
let loadFailed = false;
{
  try {
    factory = requested === 'playground'
      ? await preparePlayground()
      : requested === 'mushroom2d'
        ? await prepareMushroom2D()
        : await prepareCourse(requested, requestedParams.get('entry'), requestedParams.get('push') === '1', requestedAct, Number(requestedParams.get('area')));
    worldName = requested;
  } catch (error) {
    console.warn('[jumpman] requested course unavailable; using playground', error);
    loadFailed = true;
  }
}
for (const nav of document.querySelectorAll('.course-links')) {
  for (const [id, course] of Object.entries(COURSES)) {
    const link = document.createElement('a');
    const url = new URL(location.href);
    url.searchParams.set('level', id);
    url.searchParams.delete('act');
    url.searchParams.delete('area');
    url.searchParams.delete('entry');
    url.searchParams.delete('push');
    link.href = url.href;
    link.textContent = course.name;
    if (id === worldName) link.setAttribute('aria-current', 'page');
    link.addEventListener('click', event => {
      if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
      event.preventDefault();
      void game.loadCourse(id as CourseId);
    });
    nav.append(link);
  }
}
const game = new Game(factory, worldName, requestedAct);
window.__JUMPMAN__ = game;
window.addEventListener('popstate', () => {
  const params = new URLSearchParams(location.search);
  void game.loadCourse(courseId(params.get('level')), params.get('entry'), params.get('push') === '1', 'none', Math.max(1, Number(params.get('act') ?? 1) || 1), Number(params.get('area')));
});
picker.disabled = false;
let selection = 0;
picker.addEventListener('change', async event => {
  const input = event.target;
  if (!(input instanceof HTMLInputElement)) return;
  const current = ++selection;
  const url = new URL(location.href);
  url.searchParams.set('character', input.value);
  history.replaceState(history.state, '', url);
  showChoice(input.value);
  start.disabled = true;
  start.textContent = 'LOADING CHARACTER…';
  await game.selectCharacter(input.value);
  if (current === selection) {
    start.disabled = false;
    start.textContent = 'START PLAYING';
  }
});
await game.characterReady;
if (selection === 0) {
  start.disabled = false;
  start.textContent = 'START PLAYING';
}
if (!loadFailed && new URLSearchParams(location.search).has('entry')) game.start();
(document.getElementById('course-name') as HTMLElement).textContent = COURSES[worldName].name;
(document.getElementById('star-ready') as HTMLElement).textContent = '★ REACH THE STAR ★';
(document.getElementById('load-note') as HTMLElement).textContent = COURSES[worldName].note;
if (loadFailed) (document.getElementById('load-note') as HTMLElement).textContent = 'SM64 course unavailable. The movement playground is ready instead.';


// Keep the canvas from stealing focus outlines.
document.getElementById('game')?.addEventListener('pointerdown', (event) => {
  (event.currentTarget as HTMLElement).focus?.();
});
