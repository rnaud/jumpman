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
  await page.click('[data-experiment="dream"]');
  await sleep(400);
  let snap = await state();
  assert.equal(await page.$eval('#start-overlay', element => element.hidden), true);
  assert(snap.dream.enabled, 'dream experiment button did not enable dream mode');
  assert(snap.dream.islands > 0, 'dream experiment produced no islands');
  assert.equal(await page.$eval('#experiment-badge', element => element.hidden), false);
  assert(new URL(page.url()).searchParams.get('experiment') === 'dream', 'experiment URL state missing');
  await page.screenshot({ path: '/tmp/jumpman-experiment-dream.png' });

  await page.goto(new URL('?level=playground', process.env.URL || 'http://127.0.0.1:5173/').href, { waitUntil: 'networkidle0' });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'playground', { timeout: 30000 });
  await page.click('[data-experiment="hundred"]');
  await sleep(400);
  snap = await state();
  assert.equal(snap.hundred.count, 48, 'hundred experiment button did not spawn agents');
  assert.equal(await page.$eval('#agent-inspector', element => element.hidden), true);
  await page.screenshot({ path: '/tmp/jumpman-experiment-hundred.png' });

  await page.goto(new URL('?level=playground', process.env.URL || 'http://127.0.0.1:5173/').href, { waitUntil: 'networkidle0' });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'playground', { timeout: 30000 });
  await page.click('[data-experiment="dimensions"]');
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'mushroom2d', { timeout: 30000 });
  await sleep(300);
  snap = await state();
  assert.equal(snap.world, 'mushroom2d', 'dimensions experiment did not load the authored level');
  assert.equal(snap.viewMode, 'side', 'dimensions experiment did not enter side view');
  assert.deepEqual(errors, []);
  await page.goto(new URL('?level=playground&experiment=dream', process.env.URL || 'http://127.0.0.1:5173/').href, { waitUntil: 'networkidle0' });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'playground', { timeout: 30000 });
  await sleep(300);
  snap = await state();
  assert(snap.dream.enabled, 'experiment URL did not auto-start dream mode');
  console.log('PASS: experiment quick-start buttons and URL persistence');
} finally {
  await browser.close();
}
