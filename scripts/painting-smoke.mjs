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

  const painting = await page.evaluate(() => {
    const surfaces = window.__JUMPMAN__.playground.collision.surfaces;
    const surface = surfaces.find(candidate => candidate.type >= 0x00d3 && candidate.type <= 0x00d5);
    if (!surface) return null;
    return {
      type: surface.type,
      x: (surface.v1[0] + surface.v2[0] + surface.v3[0]) / 3,
      y: (surface.v1[1] + surface.v2[1] + surface.v3[1]) / 3,
      z: (surface.v1[2] + surface.v2[2] + surface.v3[2]) / 3,
    };
  });
  assert(painting, 'no BoB painting surface in the castle lobby');
  await page.evaluate(p => window.__JUMPMAN__.teleport3(p.x, p.y + 20, p.z), painting);
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'bob', { timeout: 15000 });
  assert.deepEqual(errors, []);
  console.log('PASS: castle painting warp', painting);
} finally {
  await browser.close();
}
