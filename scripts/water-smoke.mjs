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
  await page.goto(new URL('?level=castle_grounds', process.env.URL || 'http://127.0.0.1:5173/').href, { waitUntil: 'networkidle0', timeout: 60000 });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'castle_grounds', { timeout: 30000 });
  await page.click('#start-button');
  await new Promise(resolve => setTimeout(resolve, 500));
  const water = await page.evaluate(() => {
    const collision = window.__JUMPMAN__.playground.collision;
    const box = collision.waterBoxes[0];
    const x = (box[0] + box[3]) / 2;
    const z = (box[2] + box[5]) / 2;
    return { x, z, level: collision.waterLevel(x, z) };
  });
  assert(water.level !== null, 'no water box');
  await page.evaluate(w => window.__JUMPMAN__.teleport3(w.x, w.level - 180, w.z), water);
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.underwater === true, { timeout: 5000 });
  await page.screenshot({ path: '/tmp/jumpman-water.png' });
  assert.deepEqual(errors, []);
  console.log('PASS: underwater fog/splash', water);
} finally {
  await browser.close();
}
