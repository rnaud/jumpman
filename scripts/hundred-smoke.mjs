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

  await page.goto(new URL('?level=playground', process.env.URL || 'http://127.0.0.1:5173/').href, { waitUntil: 'networkidle0', timeout: 60000 });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'playground', { timeout: 30000 });
  await page.click('#start-button');
  await sleep(400);
  await page.keyboard.press('h');
  await sleep(400);
  let snap = await state();
  assert(snap.hundred.enabled, 'hundred Marios did not enable');
  assert.equal(snap.hundred.count, 48, 'unexpected agent count');
  await page.screenshot({ path: '/tmp/jumpman-hundred.png' });
  await page.keyboard.press('h');
  await sleep(200);
  snap = await state();
  assert.equal(snap.hundred.count, 0, 'agents remained after disabling');
  assert.deepEqual(errors, []);
  console.log('PASS: 48 autonomous Marios spawn and disperse');
} finally {
  await browser.close();
}
