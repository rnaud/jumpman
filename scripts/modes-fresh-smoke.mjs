// Pressing a mode button must always start that mode from a clean slate (no carry-over from the previous one).
import puppeteer from 'puppeteer-core';
import assert from 'node:assert/strict';
const browser = await puppeteer.launch({
  executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', headless: true,
  args: ['--no-sandbox', '--enable-unsafe-swiftshader', '--use-gl=angle', '--use-angle=swiftshader'],
  defaultViewport: { width: 1280, height: 800 },
});
try {
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.goto((process.env.URL || 'http://127.0.0.1:5174/') + '?level=bob', { waitUntil: 'networkidle0', timeout: 90000 });
  await page.waitForFunction(() => window.__JUMPMAN__ && !document.querySelector('#start-button').disabled, { timeout: 90000 });
  const snap = () => page.evaluate(() => {
    const g = window.__JUMPMAN__;
    return {
      exp: g.activeExperiment?.id ?? null, body: document.body.dataset.experiment ?? null,
      ui: document.getElementById('experiment-ui').children.length,
      badgeHidden: document.getElementById('experiment-badge').hidden,
      lives: g.mario.lives, health: g.mario.health, maxHealth: g.mario.maxHealth, world: g.debugState.world,
      freeze: document.body.classList.contains('xp-freeze'),
    };
  });
  const pause = () => page.evaluate(() => document.getElementById('settings-button').click());
  const pick = async id => { await pause(); await page.waitForSelector(`#settings[open] [data-experiment="${id}"]`); await page.click(`#settings [data-experiment="${id}"]`); await new Promise(r => setTimeout(r, 2500)); };

  // Title screen: MODES is the default tab, LEVELS holds the course picker and START PLAYING.
  assert.equal(await page.$eval('#start-tab-modes', e => e.getAttribute('aria-selected')), 'true');
  assert(await page.$eval('#start-panel-levels', e => e.hidden), 'levels panel hidden by default');
  await page.click('#start-tab-levels');
  assert(await page.$eval('#start-button', e => e.offsetParent !== null), 'START PLAYING reachable from LEVELS');
  await page.click('#start-tab-modes');
  await page.click('#start-overlay [data-experiment="runner"]');
  await new Promise(r => setTimeout(r, 2500));
  let s = await snap();
  assert.equal(s.exp, 'runner'); assert.equal(s.body, 'runner');

  await page.evaluate(() => { const m = window.__JUMPMAN__.mario; m.lives = 1; m.health = 0x100; });
  await pick('god');
  s = await snap();
  assert.equal(s.exp, 'god'); assert.equal(s.body, 'god');
  assert.equal(s.lives, 4, 'lives reset'); assert.equal(s.health, s.maxHealth, 'health reset');

  await pick('planet');
  s = await snap();
  assert.equal(s.exp, 'planet'); assert.equal(s.body, 'planet'); assert(!s.badgeHidden);

  await pause();
  await page.click('#leave-experiment');
  await new Promise(r => setTimeout(r, 800));
  s = await snap();
  assert.equal(s.exp, null); assert.equal(s.body, null); assert.equal(s.ui, 0); assert(s.badgeHidden); assert(!s.freeze);

  await pick('dimensions');
  await pause();
  await page.click('#pause-tab-levels');
  await page.evaluate(() => [...document.querySelectorAll('#settings .course-links a')].find(a => a.textContent.includes('Whomp')).click());
  await new Promise(r => setTimeout(r, 5000));
  s = await snap();
  assert.equal(s.exp, null, 'choosing a course exits the experiment'); assert.equal(s.body, null); assert.equal(s.ui, 0);

  await pause();
  await page.click('#main-menu-button');
  assert(await page.$eval('#start-tab-modes', e => e.getAttribute('aria-selected')) === 'true', 'main menu returns to MODES');
  assert.deepEqual(errors, []);
  console.log('modes-fresh-smoke ok');
} finally {
  await browser.close();
}
