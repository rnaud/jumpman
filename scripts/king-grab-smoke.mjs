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
  await page.goto(new URL('?level=bob&act=1', process.env.URL || 'http://127.0.0.1:5173/').href, { waitUntil: 'networkidle0', timeout: 60000 });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'bob', { timeout: 30000 });
  await page.click('#start-button');
  await sleep(400);
  const king = await page.evaluate(() => {
    const found = window.__JUMPMAN__.playground.objects.manager.objects.find(object => object.behavior === 'kingBobomb');
    return found ? { pos: found.pos, yaw: found.moveYaw } : null;
  });
  assert(king, 'King Bob-omb missing');
  await page.evaluate(([x, y, z, yaw]) => window.__JUMPMAN__.teleport3(x, y + 10, z - 180, yaw), [...king.pos, king.yaw]);
  for (let pageIndex=0; pageIndex<6; pageIndex++) {
    await page.waitForFunction(i=>window.__JUMPMAN__.kingDialog?.page===i,{},pageIndex);
    await page.keyboard.press('j');
  }
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.action === 0x00020370, { timeout: 5000 });
  const grabbed = await page.evaluate(() => window.__JUMPMAN__.debugState);
  await page.screenshot({ path: '/tmp/jumpman-king-grab.png' });
  await sleep(1800);
  const thrown = await page.evaluate(() => window.__JUMPMAN__.debugState);
  assert.notEqual(thrown.action, 0x00020370, 'King did not throw Mario');
  assert(thrown.health < grabbed.health, 'King throw did not damage Mario');
  assert.deepEqual(errors, []);
  console.log('PASS: King Bob-omb grab and throw', { grabbed: grabbed.health, thrown: thrown.health });
} finally {
  await browser.close();
}
