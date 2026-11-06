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
  assert.equal(await page.$eval('#star-count', element => element.textContent), '0 / 120');

  snap = await open(3);
  const gate = snap.objects.find(object => object.behavior === 'chainChompGate');
  assert(gate, 'chain chomp gate missing');
  assert(Math.hypot(snap.starPos[0] - gate.pos[0], snap.starPos[2] - gate.pos[2]) < 800, 'act 3 star is not at the gate');

  const red = await open(4);
  await page.click('#start-button');
  await sleep(300);
  assert.equal(red.redCoins, 0);
  const redPositions = await page.evaluate(() => window.__JUMPMAN__.playground.redCoins.map(coin => [coin.position.x, coin.position.y, coin.position.z]));
  assert.equal(redPositions.length, 8, 'act 4 did not spawn 8 red coins');
  for (const [x, y, z] of redPositions) {
    await page.evaluate(([px, py, pz]) => window.__JUMPMAN__.teleport3(px, py - 55, pz), [x, y, z]);
    await sleep(120);
  }
  const redDone = await page.evaluate(() => window.__JUMPMAN__.debugState);
  assert.equal(redDone.redCoins, 8, 'red coins were not all collected');
  assert(redDone.coins >= 16, 'red coins should award two coins each');
  assert.deepEqual([redDone.starPos[0], redDone.starPos[2]], [-6000, 2400]);
  assert(redDone.missionComplete && redDone.starReady, '8 red coins did not unlock the star');

  await page.evaluate(() => {
    const game = window.__JUMPMAN__;
    const [x, y, z] = game.debugState.starPos;
    game.teleport3(x, y - 55, z);
  });
  await page.waitForSelector('#castle-button', { visible: true });
  await page.click('#castle-button');
  await page.waitForFunction(() => window.__JUMPMAN__.debugState.world === 'castle_inside');
  assert.equal(new URL(page.url()).searchParams.get('entry'), 'WARP_NODE_32');
  await page.waitForFunction(() => window.__JUMPMAN__.mario.action === 0x0c400201);
  assert.equal(await page.evaluate(() => window.__JUMPMAN__.debugState.world), 'castle_inside');

  await page.evaluate(() => localStorage.setItem('jumpman.save.v1.slot1', JSON.stringify({ version: 1, stars: ['bob'], bestCoins: {} })));
  await open(1);
  assert.equal(await page.evaluate(() => window.__JUMPMAN__.playground.star.visible), false, 'King Bob-omb must award the star before it appears');
  assert.equal(await page.$eval('#star-count', element => element.textContent), '1 / 120');
  snap = await open(2);
  assert.equal(await page.evaluate(() => window.__JUMPMAN__.playground.star.visible), false, 'the race reward must stay hidden before Koopa loses');
  await page.evaluate(() => localStorage.removeItem('jumpman.save.v1.slot1'));
  await page.goto(new URL('?level=wf', page.url()).href, { waitUntil: 'networkidle0' });
  await page.click('#start-button');
  // Seed only the running coin count to exercise the threshold crossing on a red pickup.
  await page.evaluate(() => { window.__JUMPMAN__.coinsCollected = 99; });
  for (let i = 0; i < 8; i++) {
    await page.evaluate(index => {
      const g = window.__JUMPMAN__;
      const coin = g.playground.redCoins[index].position;
      g.teleport3(coin.x, coin.y, coin.z);
    }, i);
    await page.waitForFunction(index => window.__JUMPMAN__.playground.redCoins[index].collected, {}, i);
  }
  await page.waitForFunction(() => window.__JUMPMAN__.coinStarFlight?.tangible);
  await page.evaluate(() => {
    const g = window.__JUMPMAN__;
    const {x, y, z} = g.playground.hundredCoinStar.group.position;
    g.teleport3(x, y - 30, z);
  });
  await page.waitForFunction(() => window.__JUMPMAN__.save.hasStar('wf', 7));
  assert.equal(await page.evaluate(() => window.__JUMPMAN__.debugState.won), false);
  assert.equal(await page.evaluate(() => window.__JUMPMAN__.playground.star.visible), false, 'red coins must not award the boss star');
  await page.evaluate(() => {
    const g = window.__JUMPMAN__;
    const {x, y, z} = g.playground.sideStars.find(s => s.redCoins).group.position;
    g.teleport3(x, y - 30, z);
  });
  await page.waitForFunction(() => window.__JUMPMAN__.save.hasStar('wf', 4));
  assert.equal(await page.evaluate(() => window.__JUMPMAN__.save.hasStar('wf', 1)), false);
  assert.deepEqual(errors, []);
  console.log('PASS: per-act Bob stars and mission gating');
} finally {
  await browser.close();
}
