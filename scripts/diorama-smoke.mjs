import puppeteer from 'puppeteer-core';
import assert from 'node:assert/strict';

const browser = await puppeteer.launch({
  executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  headless: true,
  args: ['--no-sandbox', '--enable-unsafe-swiftshader', '--use-gl=angle', '--use-angle=swiftshader'],
  defaultViewport: { width: 1440, height: 900 },
});
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
try {
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
  await page.goto(new URL('?level=bob&experiment=diorama', process.env.URL || 'http://127.0.0.1:5173/').href, { waitUntil: 'networkidle0', timeout: 60000 });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.diorama?.ready === true, { timeout: 30000 });
  const before = await page.evaluate(() => window.__JUMPMAN__.debugState.diorama);
  assert.equal(before.enabled, true);
  assert(before.actors >= 15, 'diorama actors missing');
  assert(before.kinds.includes('mario') && before.kinds.includes('goomba'), 'diorama needs Marios and Goombas');
  await page.screenshot({ path: '/tmp/jumpman-diorama.png' });

  const clicked = await page.evaluate(() => window.__JUMPMAN__.diorama.clickActor(0));
  assert(clicked, 'diorama actor was not clickable');
  await sleep(250);
  const after = await page.evaluate(() => window.__JUMPMAN__.debugState.diorama);
  assert.equal(after.selected, 0, 'diorama selection missing');
  await page.screenshot({ path: '/tmp/jumpman-diorama-selected.png' });

  await page.keyboard.press('v');
  await sleep(200);
  const closed = await page.evaluate(() => window.__JUMPMAN__.debugState.diorama);
  assert.equal(closed.enabled, false, 'diorama did not close');
  assert.deepEqual(errors, []);
  console.log('PASS: diorama board, actors, selection and free-fly mode');
} finally {
  await browser.close();
}
