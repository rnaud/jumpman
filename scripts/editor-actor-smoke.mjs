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
  await sleep(250);
  const before = (await state()).objects.filter(object => object.behavior === 'bobomb').length;
  await page.keyboard.press('e');
  await page.keyboard.press('4');
  await page.mouse.click(720, 520);
  await sleep(400);
  const after = (await state()).objects.filter(object => object.behavior === 'bobomb').length;
  assert.equal(after, before + 1, 'editor did not spawn a Bob-omb');
  await page.keyboard.press('e');
  assert.deepEqual(errors, []);
  console.log('PASS: editor actor spawn', { before, after });
} finally {
  await browser.close();
}
