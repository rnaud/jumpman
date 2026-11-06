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
  const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
  const state = () => page.evaluate(() => window.__JUMPMAN__.debugState);

  await page.goto(new URL('?level=playground', process.env.URL || 'http://127.0.0.1:5173/').href, { waitUntil: 'networkidle0', timeout: 60000 });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'playground', { timeout: 30000 });
  await page.click('#start-button');
  await sleep(500);
  await page.keyboard.press('e');
  await sleep(120);
  assert.equal(await page.$eval('#editor-hint', element => element.hidden), false);
  assert((await state()).editorActors.length > 0, 'editor did not expose draggable actors/coins');
  await page.keyboard.press(']');
  await sleep(80);
  assert.equal((await state()).gravityScale, 1.25);
  await page.keyboard.press('[');
  await sleep(80);
  assert.equal((await state()).gravityScale, 1);

  await page.mouse.click(720, 520);
  await sleep(200);
  let snap = await state();
  assert(snap.editorEnabled, 'editor did not enable');
  assert.equal(snap.editorBlocks.length, 1, 'click did not spawn an editor block');
  const before = snap.editorBlocks[0].pos;

  await page.mouse.move(720, 520);
  await page.mouse.down();
  await page.mouse.move(820, 560, { steps: 6 });
  await page.mouse.up();
  await sleep(150);
  snap = await state();
  assert(Math.abs(snap.editorBlocks[0].pos[0] - before[0]) > 5 || Math.abs(snap.editorBlocks[0].pos[2] - before[2]) > 5, 'block did not move');

  await page.keyboard.down('Control');
  await page.keyboard.press('z');
  await page.keyboard.up('Control');
  await sleep(120);
  snap = await state();
  assert(Math.abs(snap.editorBlocks[0].pos[0] - before[0]) < 1 && Math.abs(snap.editorBlocks[0].pos[2] - before[2]) < 1, 'undo did not restore the block');

  await page.reload({ waitUntil: 'networkidle0' });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'playground', { timeout: 30000 });
  await page.click('#start-button');
  await sleep(300);
  snap = await state();
  assert.equal(snap.editorBlocks.length, 1, 'editor block did not persist across reload');
  await page.keyboard.press('e');
  await page.keyboard.press('Delete');
  await sleep(100);
  snap = await state();
  assert.equal(snap.editorBlocks.length, 0, 'Delete did not remove the persistent block');
  await page.keyboard.press('2');
  await page.mouse.click(880, 480);
  await sleep(200);
  snap = await state();
  assert.equal(snap.editorBlocks.length, 1, 'ramp did not spawn');
  assert.equal(snap.editorBlocks[0].type, 'ramp');
  await page.keyboard.press('Delete');
  await page.keyboard.press('3');
  await page.mouse.click(780, 540);
  await sleep(200);
  snap = await state();
  assert.equal(snap.editorBlocks.length, 1, 'bounce block did not spawn');
  assert.equal(snap.editorBlocks[0].type, 'bounce');
  await page.keyboard.press('Delete');
  const beforeCoins = snap.coinCount;
  await page.keyboard.press('5');
  await page.mouse.click(700, 580);
  await sleep(200);
  snap = await state();
  assert.equal(snap.coinCount, beforeCoins + 1, 'editor did not spawn a coin');
  await page.keyboard.press('e');
  await page.screenshot({ path: '/tmp/jumpman-editor.png' });
  assert.deepEqual(errors, []);
  console.log('PASS: runtime editor spawn, drag and delete');
} finally {
  await browser.close();
}
