import puppeteer from 'puppeteer-core';
import assert from 'node:assert/strict';

const browser = await puppeteer.launch({
  executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  headless: true,
  args: ['--no-sandbox', '--enable-unsafe-swiftshader', '--use-gl=angle', '--use-angle=swiftshader'],
  defaultViewport: { width: 520, height: 360 },
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
  await page.click('#start-button');
  await page.evaluate(() => {
    const g = window.__JUMPMAN__, box = g.playground.objects.manager.findByBehavior('starBox');
    g.teleport3(box.pos[0],box.pos[1]-80,box.pos[2]);
    g.mario.setAction(0x0100088c);
    g.mario.vel[1] = 30;
  });
  await page.waitForFunction(() => window.__JUMPMAN__.boxFlights.get(3)?.tangible);
  await page.evaluate(() => {
    const g = window.__JUMPMAN__, {x,y,z} = g.playground.sideStars.find(s => s.box).group.position;
    g.teleport3(x,y-30,z);
  });
  await page.waitForFunction(() => window.__JUMPMAN__.save.hasStar('bob',3));
  assert.equal(await page.evaluate(() => window.__JUMPMAN__.save.hasStar('bob',6)),false);

  await open(1); // Secrets and gate are available outside their selected missions.
  await page.click('#start-button');
  await page.evaluate(() => {
    const g = window.__JUMPMAN__;
    for (const pos of g.playground.secretStar.triggers) {
      g.teleport3(...pos);
      g.stepSimulation();
      g.stepSimulation();
    }
    if (g.secrets.size !== 5) throw Error('Missing flying-ring trigger');
    for (let i=0;i<120;i++) g.stepSimulation();
    const {x,y,z} = g.playground.sideStars.find(s => s.secret).group.position;
    g.teleport3(x,y-30,z);
  });
  await page.waitForFunction(() => window.__JUMPMAN__.save.hasStar('bob',5));
  await open(6);
  await page.click('#start-button');
  await page.evaluate(() => {
    const g = window.__JUMPMAN__, manager = g.playground.objects.manager;
    const chomp = manager.findByBehavior('chainChomp');
    g.teleport3(chomp.home[0]+500,chomp.home[1]+50,chomp.home[2]);
    g.stepSimulation();
    const post = chomp.links[0];
    for (let pound=0; pound<3; pound++) {
      g.teleport3(post.pos[0],post.pos[1]+340,post.pos[2]);
      g.mario.setAction(0x008008a9);
      for(let i=0;i<65;i++) g.stepSimulation();
    }
    g.teleport3(post.home[0]+420,post.home[1]+40,post.home[2]);
    for(let i=0;i<350;i++) g.stepSimulation();
    if (manager.findByBehavior('chainChompGate')) throw Error('Gate did not break: '+JSON.stringify([chomp.vars, post.vars, chomp.pos, post.pos, g.mario.action]));
    const {x,y,z} = g.playground.star.position;
    g.teleport3(x,y-30,z);
  });
  await page.waitForFunction(() => window.__JUMPMAN__.save.hasStar('bob',6));

  const red = await open(4);
  await page.click('#start-button');
  await sleep(300);
  assert.equal(red.redCoins, 0);
  const flight = await page.evaluate(() => {
    const g = window.__JUMPMAN__, world = g.playground, star = world.star;
    const marker = world.redReward.marker;
    const [mx,my,mz] = world.redReward.pos;
    const markerFloor = world.collision.findFloor(mx,my+200,mz).height;
    if (marker.position.y !== markerFloor+60 || marker.rotation.x !== Math.PI/2) throw Error('Missing floor marker');
    for (const coin of world.redCoins) {
      if (star.visible) throw Error('Red star appeared before the last coin');
      g.teleport3(coin.position.x,coin.position.y-55,coin.position.z);
      g.stepSimulation();
      if (!coin.collected) throw Error('Red coin pickup missed');
    }
    if (star.visible || g.starReady) throw Error('Red star skipped the spawn delay');
    const originalFov = g.camera.fov;
    let peak = 0, readyDuringFlight = false, frozen, cutawayFov, markerYaw;
    for (let i=0; i<120; i++) {
      g.stepSimulation();
      if (g.secretCamera) {
        markerYaw ??= marker.rotation.y;
        if (markerYaw !== marker.rotation.y) throw Error('Marker advanced during time stop');
        frozen ??= [...g.mario.pos];
        if (g.mario.pos.some((p,j) => p !== frozen[j])) throw Error('Mario moved during the star shot');
        readyDuringFlight ||= g.starReady;
        if (i === 34) cutawayFov = g.camera.fov;
      }
      peak = Math.max(peak,star.position.y);
    }
    return {peak, cutawayFov, readyDuringFlight, landed:star.position.y, restored:!g.secretCamera && g.camera.fov === originalFov};
  });
  assert.deepEqual(flight,{peak:1400, cutawayFov:60, readyDuringFlight:false, landed:1000, restored:true});
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
  await page.waitForFunction(() => window.__JUMPMAN__.coinStarFlight?.tangible && window.__JUMPMAN__.redStarTimer === 120);
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
