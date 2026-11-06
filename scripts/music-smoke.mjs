import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args: ['--no-sandbox', '--enable-unsafe-swiftshader'],
});
try {
  const page = await browser.newPage();
  await page.setViewport({width: 420,height: 320});
  await page.goto(new URL('?level=bob', process.env.URL || 'http://127.0.0.1:5173/').href, {waitUntil:'networkidle0'});
  await page.click('#start-button');
  await page.waitForFunction(() => window.__JUMPMAN__.sfx.node);
  const output = await page.evaluate(async () => {
    const g = window.__JUMPMAN__, s = g.sfx;
    const analyser = s.context.createAnalyser();
    s.node.connect(analyser);
    const samples = new Float32Array(analyser.fftSize);
    const results = [];
    for (const id of ['bob','castle_inside','jrb','bowser_3','castle_grounds']) {
      if (id !== 'bob') await g.loadCourse(id);
      let peak = 0;
      for (let i = 0; i < 30; i++) {
        await new Promise(r => setTimeout(r,100));
        analyser.getFloatTimeDomainData(samples);
        for (const v of samples) peak = Math.max(peak, Math.abs(v));
      }
      results.push({id, name:s.musicTrack(), peak});
    }
    s.node.disconnect(analyser);
    return results;
  });
  assert.deepEqual(output.map(r => r.name), ['grass','inside','water','boss_koopa_final','ambient']);
  for (const {id,peak} of output) assert(peak > 0.001, id + ' produced silent audio');
  console.log(output);
} finally { await browser.close(); }
