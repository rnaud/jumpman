// Captures the title menu, in-game HUD, pause dialog and win overlay at a
// wide and a phone-sized viewport. Usage: URL=http://127.0.0.1:5173/ node scripts/ui-screenshots.mjs
import puppeteer from 'puppeteer-core';
import { mkdir } from 'node:fs/promises';

const base = process.env.URL || 'http://127.0.0.1:5173/';
const out = process.env.OUT || 'screenshots/ui';
await mkdir(out, { recursive: true });
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const browser = await puppeteer.launch({
  executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  headless: true,
  args: ['--no-sandbox', '--enable-unsafe-swiftshader', '--use-gl=angle', '--use-angle=swiftshader'],
});
const errors = [];
try {
  for (const [name, viewport] of [['wide', { width: 1440, height: 900 }], ['phone', { width: 375, height: 812, deviceScaleFactor: 2, isMobile: true, hasTouch: true }]]) {
    const page = await browser.newPage();
    page.on('pageerror', error => errors.push(error.message));
    await page.setViewport(viewport);
    await page.goto(new URL('?level=castle_grounds', base).href, { waitUntil: 'networkidle0', timeout: 90000 });
    await page.waitForFunction(() => !document.getElementById('start-button')?.disabled, { timeout: 60000 });
    await page.evaluate(() => document.fonts.ready);
    await sleep(900);
    await page.screenshot({ path: `${out}/${name}-menu.jpg`, quality: 72 });

    await page.click('#start-button');
    await sleep(2500);
    await page.evaluate(() => {
      const game = window.__JUMPMAN__;
      document.body.classList.remove('classic-look');
      const update = game.hud.update.bind(game.hud);
      game.hud.update = (m, state) => update(m, { ...state, interaction: 'B · READ THE SIGN' });
      game.hud.showToast('GO!  A = JUMP · HOLD FOR HEIGHT', 60000);
    });
    await sleep(700);
    await page.screenshot({ path: `${out}/${name}-hud.jpg`, quality: 72 });

    await page.click('#settings-button');
    await sleep(600);
    await page.screenshot({ path: `${out}/${name}-pause.jpg`, quality: 72 });
    await page.evaluate(() => document.getElementById('settings').close());

    await page.evaluate(() => {
      document.getElementById('win-text').textContent = 'You collected 8 coins and reached the star in 1:24.6';
      document.getElementById('win-overlay').hidden = false;
    });
    await sleep(800);
    await page.screenshot({ path: `${out}/${name}-win.jpg`, quality: 72 });
    await page.close();
  }
} finally {
  await browser.close();
}
if (errors.length) console.warn('page errors:', errors);
console.log(`screenshots written to ${out}/`);
