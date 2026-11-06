import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args: ['--no-sandbox', '--enable-unsafe-swiftshader'],
});
try {
  const page = await browser.newPage();
  await page.setViewport({width: 480, height: 320});
  await page.goto(new URL('?level=bob', process.env.URL || 'http://127.0.0.1:5173/').href);
  await page.click('#start-button');
  await page.waitForFunction(() => window.__JUMPMAN__.sfx.node);
  await page.evaluate(() => {
    const g = window.__JUMPMAN__, s = g.sfx;
    s.startMusic({...s.currentMusic, sequence: 0}); // Leave only the original sound-effects player running.
    const analyser = s.context.createAnalyser();
    s.node.connect(analyser);
    const samples = new Float32Array(analyser.fftSize);
    window.effectPeak = 0;
    window.sampleTimer = setInterval(() => {
      analyser.getFloatTimeDomainData(samples);
      for (const value of samples) window.effectPeak = Math.max(window.effectPeak, Math.abs(value));
    }, 15);
  });
  await page.waitForFunction(() => window.__JUMPMAN__.mario.action === 0x0c400201);
  await page.evaluate(() => { window.effectPeak = 0; });
  await page.keyboard.press('Space');
  await page.waitForFunction(() => window.effectPeak > 0.01);
  await page.waitForFunction(() => window.__JUMPMAN__.mario.action === 0x0c400201);
  const jumpPeak = await page.evaluate(() => window.effectPeak);
  const count = await page.evaluate(() => {
    const g = window.__JUMPMAN__, coin = g.playground.redCoins.find(c => !c.collected);
    window.effectPeak = 0;
    g.teleport3(coin.position.x, coin.position.y, coin.position.z);
    return g.coinsCollected;
  });
  await page.waitForFunction(n => {
    const g = window.__JUMPMAN__;
    return g.redCoinsCollected === 1 && g.hudCoins === n+2 && window.effectPeak > 0.01;
  }, {}, count);
  const coinPeak = await page.evaluate(() => { clearInterval(window.sampleTimer); return window.effectPeak; });
  assert(jumpPeak > 0.01 && coinPeak > 0.01);
  console.log({jumpPeak, coinPeak});
  await page.close();
} finally { await browser.close(); }
