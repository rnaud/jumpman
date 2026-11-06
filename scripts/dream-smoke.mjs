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

  await page.goto(new URL('?level=playground&dreamSeed=42', process.env.URL || 'http://127.0.0.1:5173/').href, { waitUntil: 'networkidle0', timeout: 60000 });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'playground', { timeout: 30000 });
  await page.click('#start-button');
  await sleep(400);
  await page.keyboard.press('n');
  await sleep(300);
  let snap = await state();
  assert(snap.dream.enabled, 'dream mode did not enable');
  assert(snap.dream.islands > 0, 'no dream islands generated');
  await page.screenshot({ path: '/tmp/jumpman-dream.png' });
  await page.evaluate(() => window.__JUMPMAN__.teleport3(0, 0, 0, 0));
  await sleep(400);
  snap = await state();
  assert(snap.pos[1] > 50, 'dream fall recovery did not return Mario to an island');
  await page.keyboard.press('n');
  await sleep(200);
  snap = await state();
  assert.equal(snap.dream.islands, 0, 'dream islands remained after disabling');
  assert.deepEqual(errors, []);
  console.log('PASS: infinite dream island streaming');
} finally {
  await browser.close();
}
