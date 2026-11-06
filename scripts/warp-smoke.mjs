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
  const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
  await page.goto(process.env.URL || 'http://127.0.0.1:5173/', { waitUntil: 'networkidle0', timeout: 60000 });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'castle_grounds', { timeout: 30000 });
  await page.click('#start-button');
  await page.evaluate(() => {
    const game = window.__JUMPMAN__;
    const door = game.playground.doors.find(d => d.node === 'WARP_NODE_00');
    game.teleport3(door.pos[0], door.pos[1], door.pos[2] + 75, 0x8000);
  });
  await page.keyboard.down('w');
  await page.waitForFunction(() => window.__JUMPMAN__.debugState.world === 'castle_inside', { timeout: 15000 });
  await page.keyboard.up('w');
  await sleep(300);
  assert.equal((await page.evaluate(() => window.__JUMPMAN__.debugState)).cameraContext, 'interior', 'castle camera interior context missing');
  await page.evaluate(() => window.__JUMPMAN__.teleport3(2013, 768, -2014, 0));
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'pss', { timeout: 15000 });
  assert.equal((await page.evaluate(() => window.__JUMPMAN__.debugState)).cameraContext, 'slide', 'PSS did not use the slide camera context');
  await page.goto(new URL('?level=castle_inside', page.url()).href, { waitUntil: 'networkidle0' });
  await page.click('#start-button');
  await page.evaluate(() => window.__JUMPMAN__.teleport3(1963, 819, 1280));
  await page.waitForFunction(() => window.__JUMPMAN__.worldName === 'sa');
  for (let i = 0; i < 8; i++) {
    await page.evaluate(index => {
      const g = window.__JUMPMAN__;
      const {x, y, z} = g.playground.redCoins[index].position;
      g.teleport3(x, y, z);
      g.mario.setAction(0x380022c0); // ACT_WATER_IDLE at the submerged test position
    }, i);
    await page.waitForFunction(index => window.__JUMPMAN__.playground.redCoins[index].collected, {}, i);
  }
  await page.evaluate(() => {
    const g = window.__JUMPMAN__;
    g.teleport3(...g.debugState.starPos);
    g.mario.setAction(0x380022c0);
  });
  await page.waitForFunction(() => window.__JUMPMAN__.worldName === 'castle_inside');
  assert.equal(new URL(page.url()).searchParams.get('entry'), 'WARP_NODE_27');
  for (const [course, arena] of [['bitdw', 'bowser_1'], ['bitfs', 'bowser_2'], ['bits', 'bowser_3']]) {
    await page.goto(new URL('?level=' + course, page.url()).href, { waitUntil: 'networkidle0' });
    await page.click('#start-button');
    assert.equal(await page.evaluate(() => window.__JUMPMAN__.playground.redCoins.length), 8);
    if (course === 'bitdw') {
      for (let i = 0; i < 8; i++) {
        await page.evaluate(index => {
          const game = window.__JUMPMAN__;
          const coin = game.playground.redCoins[index].position;
          game.teleport3(coin.x, coin.y, coin.z);
        }, i);
        await page.waitForFunction(index => window.__JUMPMAN__.playground.redCoins[index].collected, {}, i);
      }
      await page.evaluate(() => {
        const game = window.__JUMPMAN__;
        const [x, y, z] = game.debugState.starPos;
        game.teleport3(x, y - 50, z);
      });
      await page.waitForFunction(() => window.__JUMPMAN__.save.hasStar('bitdw'));
      assert.equal(await page.evaluate(() => window.__JUMPMAN__.debugState.won), false);
    }
    await page.evaluate(() => {
      const game = window.__JUMPMAN__;
      const entrance = game.playground.objects.manager.objects.find(object => object.behavior === 'warp');
      game.teleport3(...entrance.pos);
    });
    await page.waitForFunction(id => window.__JUMPMAN__?.debugState.world === id, { timeout: 15000 }, arena);
    assert.equal(new URL(page.url()).searchParams.get('entry'), 'WARP_NODE_0A');
    await page.waitForFunction(() => window.__JUMPMAN__.mario.action === 0x0c400201, { timeout: 10000 });
    assert(await page.evaluate(() => {
      const mario = window.__JUMPMAN__.mario;
      return mario.respawns === 0 && mario.floor?.type !== 1 && mario.pos[1] >= 0;
    }), 'Mario should land on the arena above the lava');
    assert(await page.evaluate(() => window.__JUMPMAN__.debugState.objects.some(
      object => object.behavior === 'bowser' && object.active)));
  }
  assert.deepEqual(errors, []);
  console.log('PASS: castle front doors, secret slide and all three Bowser arena entrances');
} finally {
  await browser.close();
}
