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

  await page.goto(new URL('?level=bob', process.env.URL || 'http://127.0.0.1:5173/').href, { waitUntil: 'networkidle0', timeout: 60000 });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'bob', { timeout: 30000 });
  await page.click('#start-button');
  await sleep(400);
  await page.evaluate(() => window.__JUMPMAN__.teleport3(-6000, 0, 6000, 0));
  await sleep(150);

  await page.keyboard.press('m');
  await sleep(250);
  let snap = await state();
  assert.equal(snap.viewMode, 'side');
  const start = snap.pos;
  await page.keyboard.down('d');
  await sleep(800);
  await page.keyboard.up('d');
  snap = await state();
  assert(snap.pos[0] > start[0] + 30, 'side view did not move along world X');
  assert(Math.abs(snap.pos[2] - start[2]) < 2, 'side view leaked depth');
  assert(Math.abs(snap.cameraPos[2] - start[2]) > 700, 'side camera is not offset along Z');
  await page.screenshot({ path: '/tmp/jumpman-side.png' });

  await page.keyboard.press('m');
  await sleep(250);
  snap = await state();
  assert.equal(snap.viewMode, 'top');
  const topStart = snap.pos;
  await page.keyboard.down('w');
  await sleep(700);
  await page.keyboard.up('w');
  snap = await state();
  assert(snap.pos[2] < topStart[2] - 30, 'top-down up did not move toward -Z');
  assert(snap.cameraPos[1] > snap.pos[1] + 1000, 'top-down camera is not overhead');
  await page.screenshot({ path: '/tmp/jumpman-topdown.png' });

  await page.keyboard.press('m');
  await sleep(200);
  assert.equal((await state()).viewMode, '3d');
  assert.deepEqual(errors, []);
  console.log('PASS: 3D -> side -> top-down movement and camera switching');
} finally {
  await browser.close();
}
