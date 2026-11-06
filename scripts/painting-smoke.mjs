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

  for (const [type, destination] of [[0x00d3, 'bob'], [0x00d6, 'ccm'], [0x00d9, 'wf'], [0x00dc, 'jrb']]) {
    await page.goto(new URL('?level=castle_inside', process.env.URL || 'http://127.0.0.1:5173/').href, { waitUntil: 'networkidle0', timeout: 60000 });
    await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'castle_inside', { timeout: 30000 });
    await page.click('#start-button');
    await sleep(300);
    const painting = await page.evaluate(t => {
      const surfaces = window.__JUMPMAN__.playground.collision.surfaces;
      const surface = surfaces.find(candidate => candidate.type === t);
      if (!surface) return null;
      return {
        type: surface.type,
        x: (surface.v1[0] + surface.v2[0] + surface.v3[0]) / 3,
        y: (surface.v1[1] + surface.v2[1] + surface.v3[1]) / 3,
        z: (surface.v1[2] + surface.v2[2] + surface.v3[2]) / 3,
      };
    }, type);
    assert(painting, `painting surface ${type.toString(16)} missing`);
    await page.evaluate(p => window.__JUMPMAN__.teleport3(p.x, p.y + 20, p.z), painting);
    await page.waitForFunction(world => window.__JUMPMAN__?.debugState.world === world, { timeout: 15000 }, destination);
    if (destination === 'jrb') {
      assert(await page.evaluate(() => window.__JUMPMAN__.debugState.waterSurfaces > 0),
        'water should render when entering Jolly Roger Bay through its painting');
    }
  }
  assert.deepEqual(errors, []);
  console.log('PASS: castle painting warps to all four lobby painting courses');
} finally {
  await browser.close();
}
