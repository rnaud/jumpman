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
  await page.goto(new URL('?level=castle_inside', process.env.URL || 'http://127.0.0.1:5173/').href, { waitUntil: 'networkidle0', timeout: 60000 });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'castle_inside', { timeout: 30000 });
  await page.click('#start-button');
  await new Promise(resolve => setTimeout(resolve, 400));
  await page.evaluate(() => window.__JUMPMAN__.teleport3(2013, 768, -2014, 0));
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'pss', { timeout: 15000 });
  assert.deepEqual(errors, []);
  console.log('PASS: castle entrance warp to Princess’s Secret Slide');
} finally {
  await browser.close();
}
