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
  await page.goto(new URL('?level=bob&act=3', process.env.URL || 'http://127.0.0.1:5173/').href, { waitUntil: 'networkidle0', timeout: 60000 });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'bob', { timeout: 30000 });
  await page.click('#start-button');
  await sleep(500);

  await page.evaluate(() => {
    const game = window.__JUMPMAN__;
    const mario = game.mario;
    const object = game.playground.objects.manager.spawn({
      behavior: 'koopaShell',
      model: 'actors/koopa_shell/koopa_shell_geo.glb',
      pos: [mario.pos[0], mario.pos[1], mario.pos[2]],
      yaw: 0,
    });
    object.vars.sliding = 1;
    object.forwardVel = 12;
    mario.pos = [object.pos[0], object.pos[1] + 110, object.pos[2]];
    mario.vel = [0, -1, 0];
  });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.action === 0x20810446, { timeout: 5000 });
  const riding = await page.evaluate(() => window.__JUMPMAN__.debugState);
  assert.equal(riding.action, 0x20810446, 'Mario did not mount the shell');
  await page.screenshot({ path: '/tmp/jumpman-shell.png' });
  await page.keyboard.press(' ');
  await sleep(150);
  const jumped = await page.evaluate(() => window.__JUMPMAN__.debugState);
  assert.equal(jumped.action, 0x0281089a, 'Mario did not jump off the shell');
  assert.deepEqual(errors, []);
  console.log('PASS: Koopa shell riding', { pos: riding.pos, jumpVy: jumped.vel[1] });
} finally {
  await browser.close();
}
