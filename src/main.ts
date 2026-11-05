import './style.css';
import { Game } from './game/Game';
import { COURSES, courseId, type CourseId } from './game/courses';
import { preparePlayground } from './game/sm64Playground';
import { buildPlayground } from './game/world';
import { prepareCourse, type WorldFactory } from './game/sm64World';

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
const requested = courseId(new URLSearchParams(location.search).get('level'));
let loadFailed = false;
{
  try {
    factory = requested === 'playground' ? await preparePlayground() : await prepareCourse(requested, new URLSearchParams(location.search).get('entry'), new URLSearchParams(location.search).get('push') === '1');
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
const game = new Game(factory, worldName);
window.__JUMPMAN__ = game;
window.addEventListener('popstate', () => {
  const params = new URLSearchParams(location.search);
  void game.loadCourse(courseId(params.get('level')), params.get('entry'), params.get('push') === '1', 'none');
});
start.disabled = false;
if (!loadFailed && new URLSearchParams(location.search).has('entry')) game.start();
start.textContent = 'START PLAYING';
(document.getElementById('course-name') as HTMLElement).textContent = COURSES[worldName].name;
(document.getElementById('star-ready') as HTMLElement).textContent = '★ REACH THE STAR ★';
(document.getElementById('load-note') as HTMLElement).textContent = COURSES[worldName].note;
if (loadFailed) (document.getElementById('load-note') as HTMLElement).textContent = 'SM64 course unavailable. The movement playground is ready instead.';


// Keep the canvas from stealing focus outlines.
document.getElementById('game')?.addEventListener('pointerdown', (event) => {
  (event.currentTarget as HTMLElement).focus?.();
});
