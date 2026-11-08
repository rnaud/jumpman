// Checks that the MK64 kart synth builds its Web Audio graph and follows the speed (no listening involved).
// Usage: URL=http://127.0.0.1:5176/ node scripts/kart-audio-check.mjs
import puppeteer from 'puppeteer-core';
import assert from 'node:assert/strict';

const browser = await puppeteer.launch({
  executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', headless: true,
  args: ['--no-sandbox', '--enable-unsafe-swiftshader', '--use-gl=angle', '--use-angle=swiftshader', '--autoplay-policy=no-user-gesture-required'],
});
try {
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.goto((process.env.URL || 'http://127.0.0.1:5174/') + '?level=playground&experiment=kart', { waitUntil: 'networkidle0', timeout: 90000 });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.experiment?.ready, { timeout: 60000 });
  const info = () => page.evaluate(() => {
    const e = window.__JUMPMAN__.activeExperiment, a = e.audio;
    const [freq, gain, skid] = a.state().split('|').map(Number);
    return { freq, gain, skid, graph: !!a.ctx && !!a.engine, running: a.ctx?.state };
  });
  const step = n => page.evaluate(n => { for (let i = 0; i < n; i++) window.__JUMPMAN__.stepWorld(); }, n);
  const paint = () => page.evaluate(() => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r))));
  await step(10); await paint(); await paint();
  const idle = await info();
  assert(idle.graph, 'the synth graph should be built once frames run');
  await page.keyboard.down('w');
  await step(150); await paint(); await paint();
  const fast = await info();
  await page.keyboard.up('w');
  console.log('idle', idle, 'fast', fast);
  assert(fast.freq > idle.freq + 40, 'engine pitch should rise with speed');
  assert(fast.gain > idle.gain, 'engine should get louder with speed');
  // Cues: smash a box and use the item without throwing.
  await page.evaluate(() => {
    const e = window.__JUMPMAN__.activeExperiment, b = e.race.race.boxes[1];
    e.kart.pos = [b.pos[0], e.kart.pos[1], b.pos[1]];
    e.audio.play('box'); e.audio.play('finish'); e.audio.play('shellHit');
  });
  await step(5); await paint();
  assert.deepEqual(errors, [], errors.join('\n'));
  console.log('PASS: kart audio graph builds, pitch follows speed, cues play without errors.');
} finally {
  await browser.close();
}
