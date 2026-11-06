import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';
import {writeFile} from 'node:fs/promises';

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args: ['--no-sandbox', '--enable-unsafe-swiftshader'],
});
try {
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  for (const character of ['link', 'sm64']) {
    await page.goto(new URL('?level=kokiri_forest', process.env.URL || 'http://localhost:5173'), {waitUntil:'domcontentloaded', timeout:90000});
    await page.waitForSelector('.character-picker:not(:disabled)', {timeout:90000});
    await page.click(`.character-picker input[value="${character}"]`);
    await page.waitForFunction(c => window.__JUMPMAN__?.debugState.character === c && !document.querySelector('#start-button').disabled, {timeout:90000}, character);
    await page.click('#start-button');
    for (const course of ['kokiri_forest','lon_lon_ranch']) {
      if (course === 'lon_lon_ranch') {
        await page.click('#settings-button');
        await page.click('#settings .course-links a[href*="level=lon_lon_ranch"]');
      }
      await page.waitForFunction(id => window.__JUMPMAN__?.debugState.world === id, {timeout:90000}, course);
      const before = await page.evaluate(() => window.__JUMPMAN__.debugState);
      // Both native entrances are beyond the original SM64 horizontal boundary.
      assert(Math.max(Math.abs(before.pos[0]),Math.abs(before.pos[2])) > 8192);
      const terrain = await page.evaluate(() => {
        const g = window.__JUMPMAN__;
        const root = g.courseScene.getObjectByName(g.debugState.world);
        let filtered = 0, decals = 0;
        root.traverse(node => {
          const material = node.material;
          if (material?.map?.generateMipmaps && material.map.anisotropy > 1) filtered++;
          if (material?.userData.ootDecal && material.polygonOffset && !material.depthWrite) decals++;
        });
        return {filtered, decals};
      });
      assert(terrain.filtered > 0 && terrain.decals > 0, JSON.stringify(terrain));
      await page.keyboard.down('KeyW');
      await page.waitForFunction(p => Math.hypot(window.__JUMPMAN__.mario.pos[0]-p[0],window.__JUMPMAN__.mario.pos[2]-p[2]) > 100, {timeout:30000}, before.pos);
      await page.keyboard.up('KeyW');
      const after = await page.evaluate(() => {
        const g = window.__JUMPMAN__;
        return {floor: !!g.mario.floor, respawns:g.mario.respawns, character:g.debugState.character, won:g.won};
      });
      assert.equal(after.character,character);
      assert(after.floor && !after.won && after.respawns === before.respawns);
      if (process.env.SHOT_DIR) {
        const url = await page.evaluate(() => {
          const g = window.__JUMPMAN__;
          g.renderer.render(g.scene,g.camera);
          return g.renderer.domElement.toDataURL();
        });
        await writeFile(`${process.env.SHOT_DIR}/${course}-${character}.png`,Buffer.from(url.split(',')[1],'base64'));
      }
      console.log(`${character} walked on ${course}`);
    }
  }
  assert.deepEqual(errors,[]);
} finally {
  await browser.close();
}
