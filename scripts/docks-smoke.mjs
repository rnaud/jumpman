import puppeteer from 'puppeteer-core';
import assert from 'node:assert/strict';

const browser = await puppeteer.launch({
  executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  headless: true,
  args: ['--no-sandbox', '--enable-unsafe-swiftshader', '--use-gl=angle', '--use-angle=swiftshader'],
});
try {
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(new URL('?level=castle_inside&area=3', process.env.URL || 'http://127.0.0.1:5173/').href, {waitUntil: 'networkidle0'});
  await page.click('#start-button');
  const touchFloor = async (type, swimming = false) => page.evaluate((t, swim) => {
    const g = window.__JUMPMAN__;
    const tri = g.playground.collision.surfaces.find(s => s.type === t && s.normal[1] > 0.01);
    const mid = tri.v1.map((v,i) => (v + tri.v2[i] + tri.v3[i]) / 3);
    g.teleport3(mid[0], mid[1] + 25, mid[2]);
    if (swim) g.mario.setAction(0x380022c0);
    return mid;
  }, type, swimming);
  const front = await touchFloor(0xe8);
  await page.waitForFunction(() => window.__JUMPMAN__.worldName === 'ddd');
  await page.evaluate(() => {
    const g = window.__JUMPMAN__;
    const coin = g.playground.coins[0].position;
    g.teleport3(coin.x, coin.y, coin.z);
    g.mario.setAction(0x380022c0);
  });
  await page.waitForFunction(() => window.__JUMPMAN__.playground.coins[0].collected);
  const count = await page.evaluate(() => window.__JUMPMAN__.coinsCollected);
  const entry = await touchFloor(0x1e, true);
  await page.waitForFunction(() => window.__JUMPMAN__.playground.area === 2);
  const pos = await page.evaluate(() => window.__JUMPMAN__.mario.pos);
  assert(Math.abs(pos[0] - (entry[0] - 8192)) < 10);
  await touchFloor(0x1d, true);
  await page.waitForFunction(() => window.__JUMPMAN__.playground.area === 1);
  assert(await page.evaluate(() => window.__JUMPMAN__.playground.coins[0].collected));
  assert.equal(await page.evaluate(() => window.__JUMPMAN__.coinsCollected), count);
  await touchFloor(0x1e, true);
  await page.waitForFunction(() => window.__JUMPMAN__.playground.area === 2);
  await page.evaluate(() => {
    const g = window.__JUMPMAN__;
    const {x, y, z} = g.playground.star.position;
    g.teleport3(x, y - 50, z);
  });
  await page.waitForSelector('#win-overlay:not([hidden])');
  assert(await page.evaluate(() => window.__JUMPMAN__.save.hasStar('ddd', 1)));
  await page.click('#castle-button');
  await page.waitForFunction(() => window.__JUMPMAN__.worldName === 'castle_inside');
  assert.equal(new URL(page.url()).searchParams.get('entry'), 'WARP_NODE_35');
  const backX = await page.evaluate(() => window.__JUMPMAN__.playground.collision.surfaces.find(s => s.type === 0xe8).v1[0]);
  assert(backX > front[0] + 1800);
  await page.evaluate(() => window.__JUMPMAN__.teleport3(4147, -1280, 1997));
  await page.waitForFunction(() => window.__JUMPMAN__.worldName === 'bitfs');
  assert.deepEqual(errors, []);
  console.log('PASS: moving DDD entrance, two-way tunnel, persistent pickups, submarine star and Fire Sea entrance');
} finally {
  await browser.close();
}
