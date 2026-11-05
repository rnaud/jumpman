import puppeteer from 'puppeteer-core';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';

const CHROME = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const URL = process.env.URL || 'http://127.0.0.1:5173/';
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

  assert.deepEqual(errors, []);
  console.log('PASS: grab, carry, throw, explosion, crate break and Chain Chomp in the browser.');
} finally {
  await browser.close();
}
