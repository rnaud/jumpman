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
  await sleep(500);
  await page.keyboard.press('i');
  const before = await page.evaluate(() => {
    const game = window.__JUMPMAN__;
    const bomb = game.playground.objects.manager.objects.find(object => object.behavior === 'bobomb' && object.active);
    return { health: game.debugState.health, pos: bomb.pos, action: bomb.action };
  });
  await page.evaluate(([x, z]) => window.__JUMPMAN__.teleport3(x, 0, z + 180, 0x8000), [before.pos[0], before.pos[2]]);
  await sleep(700);
  const after = await page.evaluate(() => {
    const game = window.__JUMPMAN__;
    const bomb = game.playground.objects.manager.objects.find(object => object.behavior === 'bobomb' && object.active);
    return { health: game.debugState.health, action: bomb.action, vanished: game.mario.vanished };
  });
  assert(after.vanished, 'vanish cap was not active');
  assert.equal(after.health, before.health, 'vanish Mario took damage from a Bob-omb');
  assert.notEqual(after.action, 2, 'Bob-omb chased a vanished Mario');
  assert.deepEqual(errors, []);
  console.log('PASS: vanish cap hides Mario from Bob-omb aggro', { health: after.health, action: after.action });
} finally {
  await browser.close();
}
