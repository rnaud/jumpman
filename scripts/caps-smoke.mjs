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
  await page.keyboard.press('k');
  await page.keyboard.press('l');
  await sleep(200);
  const snap = await state();
  assert(snap.caps.metal > 0, 'metal cap was not granted');
  assert(snap.caps.vanish > 0, 'vanish cap was not granted');
  const badge = await page.$eval('#cap-badge', element => ({ hidden: element.hidden, text: element.textContent }));
  assert.equal(badge.hidden, false);
  assert(badge.text.includes('METAL') && badge.text.includes('VANISH'));
  await page.screenshot({ path: '/tmp/jumpman-caps.png' });
  assert.deepEqual(errors, []);
  console.log('PASS: metal and vanish caps', snap.caps);
} finally {
  await browser.close();
}
