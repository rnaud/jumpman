import puppeteer from 'puppeteer-core';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';

const CHROME = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const URL = process.env.URL || 'http://127.0.0.1:5173/?level=bob';
const OUT = process.env.SCREENSHOT_DIR || '/tmp/jumpman-object-shots';
await fs.mkdir(OUT, { recursive: true });

const browser = await puppeteer.launch({
  executablePath: CHROME,
  headless: true,
  args: ['--no-sandbox', '--enable-unsafe-swiftshader', '--use-gl=angle', '--use-angle=swiftshader'],
  defaultViewport: { width: 1440, height: 900 },
});

try {
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => {
    if (message.type() === 'error') errors.push(message.text());
  });

  const state = () => page.evaluate(() => window.__JUMPMAN__.debugState);
  const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

  await page.goto(URL, { waitUntil: 'networkidle0', timeout: 60000 });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.characterLoaded, { timeout: 30000 });

  // Persistent save: the HUD counts saved stars. King Bob-omb must be defeated each run.
  await page.evaluate(() => localStorage.setItem('jumpman.save.v1', JSON.stringify({
    version: 1, stars: ['bob'], bestCoins: { bob: 3 },
  })));
  await page.reload({ waitUntil: 'networkidle0' });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.characterLoaded, { timeout: 30000 });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'bob', { timeout: 30000 });
  await sleep(250);
  assert.equal(await page.$eval('#star-count', element => element.textContent), '1 / 120');
  assert.equal(await page.evaluate(() => window.__JUMPMAN__.playground.star.visible), false);
  assert.equal(await page.$$eval('#save-slots button', buttons => buttons.length), 4);

  await page.evaluate(() => localStorage.removeItem('jumpman.save.v1'));
  await page.reload({ waitUntil: 'networkidle0' });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.characterLoaded, { timeout: 30000 });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'bob', { timeout: 30000 });
  await sleep(250);
  assert.equal(await page.$eval('#star-count', element => element.textContent), '0 / 120');
  assert.equal(await page.evaluate(() => window.__JUMPMAN__.playground.star.visible), false);

  await page.click('#start-button');
  await sleep(500);

  let snap = await state();
  assert.equal(snap.world, 'bob');
  const bobombs = snap.objects.filter(object => object.behavior === 'bobomb' && object.active);
  assert(bobombs.length >= 10, `expected Bob-ombs, got ${bobombs.length}`);

  // The caged Bob-omb at the summit is stationary, so the grab is deterministic.
  const stationary = bobombs.find(object =>
    Math.hypot(object.pos[0] - 1127, object.pos[1] - 2897, object.pos[2] + 2495) < 200,
  ) ?? bobombs.find(object => object.action === 0);
  assert(stationary, 'no stationary Bob-omb found');

  const [bx, by, bz] = stationary.pos;
  await page.evaluate(([x, y, z]) => window.__JUMPMAN__.teleport3(x, y, z - 80, 0), [bx, by, bz]);
  await sleep(160);
  await page.keyboard.press('Shift');
  await sleep(900);

  snap = await state();
  assert(snap.held, `Bob-omb was not grabbed; action=${snap.actionName}`);
  assert.equal(snap.held.behavior, 'bobomb');
  console.log('GRAB', JSON.stringify({ action: snap.actionName, held: snap.held }));
  await page.screenshot({ path: `${OUT}/held-bobomb.png` });

  await page.keyboard.press('Shift');
  await sleep(1200);
  snap = await state();
  assert.equal(snap.held, null, 'Bob-omb was not thrown');
  console.log('THROW', JSON.stringify({ action: snap.actionName, health: snap.health }));
  await sleep(1200);
  snap = await state();
  const exploded = snap.objects.some(object => object.behavior === 'bobomb' && !object.active);
  assert(exploded, 'Bob-omb did not explode after landing');
  console.log('EXPLODE', JSON.stringify({ health: snap.health }));
  await page.screenshot({ path: `${OUT}/exploded-bobomb.png` });

  // Break a large crate with a punch.
  const crate = snap.objects.find(object => object.behavior === 'breakableBox' && object.active);
  assert(crate, 'no breakable crate found');
  const [cx, cy, cz] = crate.pos;
  await page.evaluate(([x, y, z]) => window.__JUMPMAN__.teleport3(x, y, z - 100, 0), [cx, cy, cz]);
  await sleep(160);
  await page.keyboard.press('Shift');
  await sleep(700);
  snap = await state();
  const broken = snap.objects.find(object => object.id === crate.id);
  assert(broken && !broken.active, 'crate did not break');
  console.log('BREAK', JSON.stringify({ crate: crate.id, action: snap.actionName }));
  await page.screenshot({ path: `${OUT}/broken-crate.png` });

  // The Chain Chomp lazily spawns its post and five chain segments when Mario is near.
  await page.evaluate(() => window.__JUMPMAN__.teleport3(260, 735, 2200, 0x8000));
  await sleep(900);
  snap = await state();
  const chomp = snap.objects.find(object => object.behavior === 'chainChomp');
  assert(chomp, 'Chain Chomp did not spawn');
  assert(chomp.links >= 5, `Chain Chomp links missing: ${chomp.links}`);
  await page.screenshot({ path: `${OUT}/chain-chomp.png` });
  console.log('CHOMP', JSON.stringify({ links: chomp.links, pos: chomp.pos.map(Math.round) }));

  // Step on the purple switch to open the cage grill.
  await page.evaluate(() => window.__JUMPMAN__.teleport3(-2283, 0, -3582, 0x8000));
  await sleep(1200);
  snap = await state();
  const purple = snap.objects.find(object => object.behavior === 'purpleSwitch');
  const grill = snap.objects.find(object => object.behavior === 'openableGrill');
  assert(purple && grill, 'switch or grill did not spawn');
  assert.equal(purple.vars.action, 2, `switch action ${purple.vars.action}`);
  assert.equal(grill.vars.open, 1, 'grill did not open');
  console.log('SWITCH', JSON.stringify({ switch: purple.vars.action, grill: grill.vars.open }));
  await page.screenshot({ path: `${OUT}/purple-switch.png` });

  const billboardState = await page.evaluate(() => {
    const co = window.__JUMPMAN__.playground.objects;
    let total = 0;
    let registered = 0;
    for (const [id, view] of co.views) {
      const object = co.manager.objects.find(candidate => candidate.id === id);
      if (object?.behavior !== 'bobomb') continue;
      view.group.traverse(node => {
        if (String(node.userData.sm64Command ?? '').startsWith('GEO_BILLBOARD')) {
          total++;
          if (co.billboards.seen.has(node)) registered++;
        }
      });
    }
    const grillSides = [];
    for (const [id, view] of co.views) {
      const object = co.manager.objects.find(candidate => candidate.id === id);
      if (object?.behavior !== 'grillDoor' && object?.behavior !== 'openableGrill') continue;
      view.group.traverse(node => {
        if (node.isMesh) grillSides.push(node.material.side);
      });
    }
    return { total, registered, grillSides };
  });
  assert(billboardState.total > 0, 'no Bob-omb billboard nodes');
  assert.equal(billboardState.registered, billboardState.total, 'Bob-omb billboards were not registered');
  assert(billboardState.grillSides.length > 0, 'no grill meshes');
  assert(billboardState.grillSides.every(side => side === 2), 'grill materials are not double-sided');
  console.log('RENDER', JSON.stringify(billboardState));

  const ballState = await page.evaluate(() => {
    const co = window.__JUMPMAN__.playground.objects;
    for (const [id, view] of co.views) {
      const object = co.manager.objects.find(candidate => candidate.id === id);
      if (object?.behavior !== 'pitBowlingBall') continue;
      return {
        registered: co.billboards.seen.has(view.group),
        offsetY: view.group.position.y - object.pos[1],
      };
    }
    return null;
  });
  assert(ballState, 'no pit bowling ball view');
  assert(ballState.registered, 'bowling ball billboard was not registered');
  assert.equal(ballState.offsetY, 130, 'bowling ball graph offset missing');
  console.log('BALL', JSON.stringify(ballState));

  // King Bob-omb at the summit.
  await page.evaluate(() => window.__JUMPMAN__.teleport3(1636, 4242, -5367, 0x8000));
  await sleep(700);
  snap = await state();
  const king = snap.objects.find(object => object.behavior === 'kingBobomb');
  assert(king && king.active, 'King Bob-omb did not spawn');
  assert(king.render?.visible, 'King Bob-omb is not rendered');
  assert.equal((await state()).cameraContext, 'boss');
  assert((await page.$eval('#toast', element => element.textContent)).includes('KING BOB-OMB'), 'King intro dialogue missing');
  console.log('KING', JSON.stringify({ pos: king.pos.map(Math.round), health: king.vars.health }));
  await page.screenshot({ path: `${OUT}/king-bobomb.png` });

  const cannon = snap.objects.find(object => object.behavior === 'waterBombCannon');
  assert(cannon, 'water bomb cannon did not spawn');
  assert(!snap.objects.some(object => object.behavior === 'koopaTheQuick'), 'Koopa should not spawn in act 1');
  await page.evaluate(() => window.__JUMPMAN__.teleport3(-5694, 128, 5900, 0x8000));
  await sleep(700);
  await page.screenshot({ path: `${OUT}/water-bomb-cannon.png` });
  console.log('CANNON', JSON.stringify({ pos: cannon.pos.map(Math.round) }));
  await page.evaluate(() => window.__JUMPMAN__.teleport3(-5723, 140, 6217, 0x8000));
  await sleep(700);
  assert((await page.$eval('#toast', element => element.textContent)).includes('BOB-OMB BUDDY'), 'buddy dialogue missing');
  console.log('BUDDY', await page.$eval('#toast', element => element.textContent));
  await page.evaluate(() => window.__JUMPMAN__.loadCourse('bob', null, false, 'none', 2));
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'bob' && window.__JUMPMAN__?.act === 2, { timeout: 30000 });
  await sleep(400);
  snap = await state();
  assert(!snap.objects.some(object => object.behavior === 'kingBobomb'), 'King Bob-omb should not spawn in act 2');
  const koopa = snap.objects.find(object => object.behavior === 'koopaTheQuick');
  const flag = snap.objects.find(object => object.behavior === 'koopaFlag');
  assert(koopa && koopa.active, 'Koopa the Quick did not spawn');
  assert(flag, 'Koopa race flag did not spawn');
  await page.evaluate(() => window.__JUMPMAN__.teleport3(-4004, 0, 5421, 0x8000));
  await sleep(500);
  snap = await state();
  console.log('KOOPA', JSON.stringify({ pos: koopa.pos.map(Math.round), race: snap.objects.find(object => object.behavior === 'koopaTheQuick').vars.raceState }));
  assert.equal(await page.$eval('#race-timer', element => element.hidden), false, 'race timer HUD did not appear');
  assert((await page.$eval('#toast', element => element.textContent)).includes('KOOPA THE QUICK'), 'Koopa proposition dialogue missing');
  await page.screenshot({ path: `${OUT}/koopa-the-quick.png` });

  await page.evaluate(() => window.__JUMPMAN__.loadCourse('bob', null, false, 'none', 3));
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'bob' && window.__JUMPMAN__?.act === 3, { timeout: 30000 });
  await sleep(400);
  snap = await state();
  const normalKoopa = snap.objects.find(object => object.behavior === 'koopa');
  assert(normalKoopa && normalKoopa.active, 'normal Koopa did not spawn in act 3');
  await page.evaluate(() => window.__JUMPMAN__.teleport3(3400, 770, 6800, 0x8000));
  await sleep(500);
  await page.screenshot({ path: `${OUT}/koopa-normal.png` });
  console.log('KOOPA_NORMAL', JSON.stringify({ pos: normalKoopa.pos.map(Math.round) }));

  const treeState = await page.evaluate(() => {
    const co = window.__JUMPMAN__.playground.objects;
    for (const [id, view] of co.views) {
      const object = co.manager.objects.find(candidate => candidate.id === id);
      if (object?.behavior !== 'tree') continue;
      return { registered: co.billboards.seen.has(view.group), active: object.active };
    }
    return null;
  });
  assert(treeState?.active, 'no climbable tree view');
  assert(treeState.registered, 'tree billboard was not registered');
  console.log('TREE', JSON.stringify(treeState));

  assert.deepEqual(errors, []);
  console.log('PASS: objects, Chain Chomp, switch, billboards, bowling ball, water bomb cannon, Bob-omb Buddy dialogue, act-gated King Bob-omb/Koopa the Quick/normal Koopa, and tree poles in the browser.');
} finally {
  await browser.close();
}
