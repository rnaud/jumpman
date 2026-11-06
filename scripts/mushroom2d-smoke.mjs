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
  await page.goto(new URL('?level=mushroom2d', process.env.URL || 'http://127.0.0.1:5173/').href, { waitUntil: 'networkidle0', timeout: 60000 });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'mushroom2d', { timeout: 30000 });
  await page.click('#start-button');
  await sleep(500);
  let snap = await page.evaluate(() => window.__JUMPMAN__.debugState);
  assert.equal(snap.viewMode, 'side', 'mushroom level did not enter side view');
  assert(snap.coinPositions.length >= 10, 'mushroom coin count');
  assert(snap.goombas.length >= 4, 'mushroom goombas missing');
  assert(snap.goombas.every(goomba => Math.abs(goomba.z) < 1), 'mushroom goombas left the 2D lane');
  assert(snap.floorNormal, 'mushroom floor missing');
  const lowCoin = snap.coinPositions.find(coin => !coin.collected && coin.y < 200);
  assert(lowCoin, 'mushroom low coin missing');
  await page.evaluate(coin => window.__JUMPMAN__.teleport3(coin.x, coin.y - 55, coin.z), lowCoin);
  await sleep(180);
  snap = await page.evaluate(() => window.__JUMPMAN__.debugState);
  assert(snap.coinPositions.find(coin => coin.x === lowCoin.x && coin.z === lowCoin.z).collected, 'mushroom coin was not collected');
  await page.evaluate(() => window.__JUMPMAN__.teleport3(-1700, 320, 0));
  await sleep(180);
  snap = await page.evaluate(() => window.__JUMPMAN__.debugState);
  assert(snap.floorHeight > 250, 'mushroom platform collision missing');
  await page.keyboard.press('Space');
  await sleep(180);
  snap = await page.evaluate(() => window.__JUMPMAN__.debugState);
  assert(snap.pos[1] > 20, 'mushroom jump did not leave the ground');
  await page.evaluate(() => window.__JUMPMAN__.teleport3(-1800, 0, 0, 0x4000));
  await sleep(150);
  const startX = snap.pos[0];
  await page.keyboard.down('d');
  await sleep(500);
  await page.keyboard.up('d');
  snap = await page.evaluate(() => window.__JUMPMAN__.debugState);
  assert(snap.pos[0] > startX + 30, 'mushroom level did not move right');
  await page.screenshot({ path: '/tmp/jumpman-mushroom2d.png' });
  assert.deepEqual(errors, []);
  console.log('PASS: authored Mushroom Kingdom 1-1 side level');
} finally {
  await browser.close();
}
