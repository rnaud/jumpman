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
  await page.evaluate(() => window.__JUMPMAN__.teleport3(-5694, 0, 5600, 0));
  await sleep(200);
  const barrel = (await state()).objects.find(object => object.behavior === 'cannonBarrel');
  assert(barrel, 'cannon barrel missing');
  await page.keyboard.press('Shift');
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.action === 0x10880899, { timeout: 5000 });
  await sleep(200);
  const snap = await state();
  assert.equal(snap.action, 0x10880899, 'cannon launch did not enter flight');
  assert(snap.caps.wing > 0, 'cannon launch did not grant flight time');
  await page.screenshot({ path: '/tmp/jumpman-cannon.png' });
  assert.deepEqual(errors, []);
  console.log('PASS: cannon launch into flight', { pos: snap.pos, caps: snap.caps });
} finally {
  await browser.close();
}
