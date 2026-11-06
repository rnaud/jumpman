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

  const open = async (act) => {
    await page.goto(new URL(`?level=bob&act=${act}`, process.env.URL || 'http://127.0.0.1:5173/').href, { waitUntil: 'networkidle0', timeout: 60000 });
    await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'bob', { timeout: 30000 });
    await sleep(250);
    return page.evaluate(() => window.__JUMPMAN__.debugState);
  };

  let snap = await open(2);
  assert.equal(snap.act, 2);
  assert.equal(snap.starReady, false, 'Bob act 2 should not start star-ready');
  let flag = snap.objects.find(object => object.behavior === 'koopaFlag');
  assert(flag, 'koopa flag missing');
  assert(Math.hypot(snap.starPos[0] - flag.pos[0], snap.starPos[2] - flag.pos[2]) < 800, 'act 2 star is not at the race flag');
  assert.equal(await page.$eval('#star-count', element => element.textContent), '0 / 7');

  snap = await open(3);
  const gate = snap.objects.find(object => object.behavior === 'chainChompGate');
  assert(gate, 'chain chomp gate missing');
  assert(Math.hypot(snap.starPos[0] - gate.pos[0], snap.starPos[2] - gate.pos[2]) < 800, 'act 3 star is not at the gate');

  await page.evaluate(() => localStorage.setItem('jumpman.save.v1.slot1', JSON.stringify({ version: 1, stars: ['bob'], bestCoins: {} })));
  await open(1);
  assert.equal(await page.evaluate(() => window.__JUMPMAN__.playground.star.visible), false, 'act 1 star should be hidden by legacy save');
  assert.equal(await page.$eval('#star-count', element => element.textContent), '1 / 7');
  snap = await open(2);
  assert.equal(await page.evaluate(() => window.__JUMPMAN__.playground.star.visible), true, 'act 2 star should remain visible');
  await page.evaluate(() => localStorage.removeItem('jumpman.save.v1.slot1'));
  assert.deepEqual(errors, []);
  console.log('PASS: per-act Bob stars and mission gating');
} finally {
  await browser.close();
}
