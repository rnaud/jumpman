// Screenshots of the MK64 free roam and race at phone sizes (touch emulation) for layout checks.
// Usage: URL=http://127.0.0.1:5176/ OUT=/tmp/kart-shots node scripts/kart-shots.mjs
import puppeteer from 'puppeteer-core';
import { mkdirSync } from 'node:fs';

const out = process.env.OUT || '/tmp/kart-shots';
mkdirSync(out, { recursive: true });
const browser = await puppeteer.launch({
  executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', headless: true,
  args: ['--no-sandbox', '--enable-unsafe-swiftshader', '--use-gl=angle', '--use-angle=swiftshader'],
});
try {
  for (const [w, h] of [[874, 402], [844, 390]]) {
    const page = await browser.newPage();
    await page.setViewport({ width: w, height: h, isMobile: true, hasTouch: true, deviceScaleFactor: 1 });
    await page.goto((process.env.URL || 'http://127.0.0.1:5174/') + '?level=playground&experiment=kart', { waitUntil: 'networkidle0', timeout: 90000 });
    await page.waitForFunction(() => window.__JUMPMAN__?.debugState.experiment?.ready, { timeout: 60000 });
    const step = n => page.evaluate(n => { for (let i = 0; i < n; i++) window.__JUMPMAN__.stepWorld(); }, n);
    const paint = () => page.evaluate(() => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r))));
    await step(30); await paint();
    await page.screenshot({ path: `${out}/free-${w}x${h}.png` });
    // Pick up an item and use it, then start a race.
    await page.evaluate(() => {
      const e = window.__JUMPMAN__.activeExperiment, b = e.race.race.boxes[1];
      e.kart.pos = [b.pos[0], e.kart.pos[1], b.pos[1]];
    });
    await step(20); await paint();
    await page.screenshot({ path: `${out}/roulette-${w}x${h}.png` });
    // A drift: full throttle, then hop and hold a turn so the sparks and smoke show.
    await page.evaluate(() => window.__JUMPMAN__.activeExperiment.race.race.racers[0].roulette = 0);
    await page.keyboard.down('w');
    await step(80);
    await page.keyboard.down('Space'); await page.keyboard.down('d');
    await step(30); await paint();
    await page.screenshot({ path: `${out}/drift-${w}x${h}.png` });
    await page.keyboard.up('Space'); await page.keyboard.up('d'); await page.keyboard.up('w');
    await page.click('[data-kart-race]');
    await step(100); await paint();
    await page.screenshot({ path: `${out}/race-${w}x${h}.png` });
    await page.close();
  }
  console.log('Wrote screenshots to', out);
} finally {
  await browser.close();
}
