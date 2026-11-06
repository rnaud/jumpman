import puppeteer from 'puppeteer-core';
import assert from 'node:assert/strict';

const base = process.env.URL || 'http://127.0.0.1:5173/';
const browser = await puppeteer.launch({
  executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  headless: true,
  args: ['--no-sandbox', '--enable-unsafe-swiftshader', '--use-gl=angle', '--use-angle=swiftshader'],
});
try {
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const enterDoor = async node => {
    await page.evaluate(id => {
      const game = window.__JUMPMAN__;
      const door = game.playground.doors.find(d => d.node === id);
      game.teleport3(door.pos[0], door.pos[1], door.pos[2] + 75, 0x8000);
    }, node);
    await page.keyboard.down('w');
  };
  for (const key of [1, 2]) {
    await page.goto(new URL('?level=bowser_' + key, base).href, { waitUntil: 'networkidle0' });
    await page.click('#start-button');
    // Arrange a falling Mario over the boss. Damage, drop flight and pickup
    // all run through the gameplay interaction loop.
    await page.evaluate(() => {
      const g = window.__JUMPMAN__;
      const boss = g.playground.objects.manager.findByBehavior('bowser');
      g.teleport3(boss.pos[0], boss.pos[1] + 430, boss.pos[2]);
      g.mario.setAction(0x0100088c); // ACT_FREEFALL
    });
    await page.waitForFunction(() => window.__JUMPMAN__.missionComplete, { timeout: 15000 });
    await page.waitForFunction(() => window.__JUMPMAN__.keyFlight === null, { timeout: 15000 });
    await page.evaluate(() => {
      const g = window.__JUMPMAN__;
      g.teleport3(...g.debugState.starPos);
    });
    await page.waitForSelector('#win-overlay:not([hidden])');
    assert.equal(await page.evaluate(k => window.__JUMPMAN__.save.hasKey(k), key), true);
    assert.equal(await page.evaluate(() => window.__JUMPMAN__.save.stars), key === 1 ? 0 : 1);
    await page.click('#castle-button');
    const returnArea = key === 1 ? 1 : 3;
    await page.waitForFunction(area => window.__JUMPMAN__.playground.area === area
      && window.__JUMPMAN__.worldName === 'castle_inside', {}, returnArea);
    await page.waitForFunction(() => window.__JUMPMAN__.mario.action === 0x0c400201);
    if (returnArea === 3) {
      await enterDoor('WARP_NODE_00');
      await page.waitForFunction(() => window.__JUMPMAN__.playground.area === 1);
      await page.keyboard.up('w');
    }
    await enterDoor(key === 1 ? 'WARP_NODE_05' : 'WARP_NODE_03');
    const destinationArea = key === 1 ? 3 : 2;
    await page.waitForFunction(area => window.__JUMPMAN__.playground.area === area, { timeout: 20000 }, destinationArea);
    await page.keyboard.up('w');
    await page.reload({ waitUntil: 'networkidle0' });
    assert.equal(await page.evaluate(k => window.__JUMPMAN__.save.doorUnlocked(k), key), true);
    assert.equal(await page.evaluate(k => window.__JUMPMAN__.save.hasKey(k), key), false);
    const destination = key === 1 ? 'hmc' : 'wdw';
    await page.evaluate(id => {
      const g = window.__JUMPMAN__;
      const index = Number(Object.keys(g.playground.paintings).find(i => g.playground.paintings[i] === id));
      const floor = g.playground.collision.surfaces.find(s => s.type === 0xd3 + index && s.normal[1] > 0.01);
      const center = floor.v1.map((v, k) => (v + floor.v2[k] + floor.v3[k]) / 3);
      g.teleport3(center[0], center[1] + 15, center[2]);
    }, destination);
    await page.waitForFunction(id => window.__JUMPMAN__.worldName === id, {}, destination);
    if (key === 1) {
      // Collect the sixth cave star while the first act is selected.
      await page.evaluate(() => {
        const g = window.__JUMPMAN__;
        const reward = g.playground.sideStars.find(star => star.act === 6);
        const { x, y, z } = reward.group.position;
        g.teleport3(x, y - 30, z);
      });
      await page.waitForFunction(() => window.__JUMPMAN__.save.hasStar('hmc', 6));
      assert.equal(await page.evaluate(() => window.__JUMPMAN__.save.hasStar('hmc', 1)), false);
    }
  }
  assert.deepEqual(errors, []);
  console.log('PASS: both Bowser rewards, castle return nodes and permanent key doors');
} finally {
  await browser.close();
}
