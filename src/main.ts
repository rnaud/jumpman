import './style.css';
import { Game } from './game/Game';
import { COURSES, courseId, type CourseId } from './game/courses';
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
for (const nav of document.querySelectorAll('.course-links')) {
  for (const [id, course] of Object.entries(COURSES)) {
    const link = document.createElement('a');
    const url = new URL(location.href);
    url.searchParams.set('level', id);
    link.href = url.href;
    link.textContent = course.name;
    if (id === requested) link.setAttribute('aria-current', 'page');
    nav.append(link);
  }
}
let loadFailed = false;
if (requested !== 'playground') {
  try {
    factory = await prepareCourse(requested);
    worldName = requested;
  } catch (error) {
    console.warn('[jumpman] requested course unavailable; using playground', error);
    loadFailed = true;
  }
}
const game = new Game(factory, worldName);
window.__JUMPMAN__ = game;
start.disabled = false;
start.textContent = 'START PLAYING';
(document.getElementById('course-name') as HTMLElement).textContent = COURSES[worldName].name;
(document.getElementById('star-ready') as HTMLElement).textContent = '★ REACH THE STAR ★';
(document.getElementById('load-note') as HTMLElement).textContent = COURSES[worldName].note;
if (loadFailed) (document.getElementById('load-note') as HTMLElement).textContent = 'SM64 course unavailable. The movement playground is ready instead.';


// Keep the canvas from stealing focus outlines.
document.getElementById('game')?.addEventListener('pointerdown', (event) => {
  (event.currentTarget as HTMLElement).focus?.();
});
