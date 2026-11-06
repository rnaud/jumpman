import puppeteer from 'puppeteer-core';
import assert from 'node:assert/strict';

const browser = await puppeteer.launch({
  executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  headless: true,
  args: ['--no-sandbox', '--enable-unsafe-swiftshader', '--use-gl=angle', '--use-angle=swiftshader'],
  defaultViewport: { width: 1440, height: 900 },
});
try {
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
  const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
  const state = () => page.evaluate(() => window.__JUMPMAN__.debugState);
  const metalActions = [0x080042f0, 0x000044f2, 0x000042f4, 0x000044f8];

  await page.goto(new URL('?level=castle_grounds', process.env.URL || 'http://127.0.0.1:5173/').href, { waitUntil: 'networkidle0', timeout: 60000 });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'castle_grounds', { timeout: 30000 });
  await page.click('#start-button');
  await sleep(400);
  const water = await page.evaluate(() => {
    const collision = window.__JUMPMAN__.playground.collision;
    const box = collision.waterBoxes[0];
    const x = (box[0] + box[3]) / 2;
    const z = (box[2] + box[5]) / 2;
    return { x, z, level: collision.waterLevel(x, z) };
  });
  await page.keyboard.press('u');
  await page.evaluate(w => window.__JUMPMAN__.teleport3(w.x, w.level - 180, w.z), water);
  await sleep(1200);
  const snap = await state();
  assert(metalActions.includes(snap.action), `not in metal water action: ${snap.action.toString(16)}`);
  assert(snap.health > 0x100, 'metal Mario drowned');
  await page.screenshot({ path: '/tmp/jumpman-metal-water.png' });
  assert.deepEqual(errors, []);
  console.log('PASS: metal water walking', { action: snap.action.toString(16), y: snap.pos[1], health: snap.health });
} finally {
  await browser.close();
}
