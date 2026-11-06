import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  headless: true,
  args: ['--no-sandbox', '--enable-unsafe-swiftshader', '--use-gl=angle', '--use-angle=swiftshader'],
});
try {
  const page = await browser.newPage();
  await page.setViewport({width: 440, height: 320});
  const errors = [];
  page.on('console', msg => { if (msg.type() === 'error') console.error(msg.text()); });
  page.on('pageerror', e => errors.push(e.message));
  const open = async id => {
    await page.goto(new URL('?level=' + id, process.env.URL || 'http://127.0.0.1:5173/').href, {waitUntil: 'networkidle0'});
    await page.click('#start-button');
  };
  const solve = async () => {
    for (let i = 1; i <= 4; i++) {
      await page.evaluate(n => {
        const g = window.__JUMPMAN__;
        const box = g.playground.objects.manager.objects.find(o => o.behavior === 'chest' && o.param === n);
        g.teleport3(...box.pos, box.moveYaw + 0x8000);
        if (g.mario.pos[1] < g.playground.collision.waterLevel(box.pos[0], box.pos[2]) - 100) g.mario.setAction(0x380022c0);
      }, i);
      await page.waitForFunction(n => window.__JUMPMAN__.playground.objects.manager.chestPuzzle.next === n + 1, {}, i);
    }
  };
  for (const [id, act] of [['ddd', 2], ['jrb', 3]]) {
    await open(id);
    await solve();
    await page.waitForFunction(() => window.__JUMPMAN__.playground.sideStars.find(s => s.chest).group.visible);
    await page.evaluate(() => {
      const g = window.__JUMPMAN__;
      const {x,y,z} = g.playground.sideStars.find(s => s.chest).group.position;
      g.teleport3(x, y - 30, z);
      if (g.mario.pos[1] < g.playground.collision.waterLevel(x,z) - 100) g.mario.setAction(0x380022c0);
    });
    await page.waitForFunction(() => window.__JUMPMAN__.worldName === 'castle_inside');
    assert(await page.evaluate((id, act) => window.__JUMPMAN__.save.hasStar(id, act), id, act));
    assert.equal(await page.evaluate(id => window.__JUMPMAN__.save.hasStar(id, 1), id), false);
  }

  await open('jrb');
  await page.evaluate(() => {
    const g = window.__JUMPMAN__;
    const tri = g.playground.collision.surfaces.find(s => s.type === 0x32 && s.normal[1] > 0);
    const [x,y,z] = tri.v1.map((v,i) => (v + tri.v2[i] + tri.v3[i]) / 3);
    g.teleport3(x, y + 10, z);
    g.mario.setAction(0x380022c0);
  });
  await page.waitForFunction(() => window.__JUMPMAN__.playground.area === 2);
  await solve();
  await page.waitForFunction(() => window.__JUMPMAN__.playground.collision.waterBoxes[0][5] === -335, {timeout: 60000});
  await page.evaluate(() => {
    const g = window.__JUMPMAN__;
    const [x,y,z] = g.playground.objects.manager.findByBehavior('starBox').pos;
    g.teleport3(x, y - 80, z);
    g.mario.setAction(0x0100088c); // rising into the underside of the star box
    g.mario.vel[1] = 30;
  });
  await page.waitForFunction(() => window.__JUMPMAN__.boxFlights.get(1)?.tangible);
  await page.evaluate(() => {
    const g = window.__JUMPMAN__;
    const {x,y,z} = g.playground.sideStars.find(s => s.box).group.position;
    g.teleport3(x, y - 30, z);
  });
  await page.waitForFunction(() => window.__JUMPMAN__.worldName === 'castle_inside');
  assert(await page.evaluate(() => window.__JUMPMAN__.save.hasStar('jrb', 1)));
  assert.deepEqual(errors, []);
  console.log('PASS: DDD current chests, JRB ocean cave, ship entry, drained water and first ship star');
} finally {
  await browser.close();
}
