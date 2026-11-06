import puppeteer from 'puppeteer-core';
import assert from 'node:assert/strict';

const browser = await puppeteer.launch({
  executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  headless: true,
  args: ['--no-sandbox', '--enable-unsafe-swiftshader', '--use-gl=angle', '--use-angle=swiftshader'],
  defaultViewport: { width: 1440, height: 900 },
});
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
try {
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });

  for (const [type, destination] of [[0x00d3, 'bob'], [0x00d6, 'ccm'], [0x00d9, 'wf'], [0x00dc, 'jrb']]) {
    await page.goto(new URL('?level=castle_inside', process.env.URL || 'http://127.0.0.1:5173/').href, { waitUntil: 'networkidle0', timeout: 60000 });
    await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'castle_inside', { timeout: 30000 });
    await page.click('#start-button');
    await sleep(300);
    await page.evaluate(t => {
      const g = window.__JUMPMAN__;
      const tri = g.playground.collision.surfaces.find(s => s.type === t && s.normal[1] > 0);
      const center = tri.v1.map((v, i) => (v + tri.v2[i] + tri.v3[i]) / 3);
      g.teleport3(center[0], center[1] + 10, center[2]);
    }, type - 0x2d);
    await page.waitForFunction(id => window.__JUMPMAN__.courseScene.getObjectByName(id + '_painting').visible, {}, destination);
    const art = await page.evaluate(id => {
      const g = window.__JUMPMAN__;
      const frame = g.courseScene.getObjectByName(id + '_painting');
      if (!frame) return null;
      return frame.children.map(mesh => ({
        vertices: mesh.geometry.getAttribute('position').count,
        width: mesh.material.map.image.width,
        height: mesh.material.map.image.height,
      }));
    }, destination);
    assert(art && art.length === 2 && art.every(part => part.vertices > 4 && part.width === 64 && part.height === 32),
      'painting must load its two original texture halves and ripple mesh');
    if (destination === 'bob' && process.env.PAINTING_SHOT) {
      const pixels = await page.evaluate(() => {
        const g = window.__JUMPMAN__;
        const frame = g.courseScene.getObjectByName('bob_painting');
        frame.updateWorldMatrix(true, true);
        const point = (x,y,z) => frame.localToWorld(frame.position.clone().set(x,y,z));
        const camera = g.camera.clone();
        camera.position.copy(point(307,307,1000));
        camera.lookAt(point(307,307,0));
        g.renderer.render(g.scene, camera);
        return g.renderer.domElement.toDataURL('image/png').split(',')[1];
      });
      await (await import('node:fs/promises')).writeFile(process.env.PAINTING_SHOT, Buffer.from(pixels, 'base64'));
    }
    const painting = await page.evaluate(t => {
      const surfaces = window.__JUMPMAN__.playground.collision.surfaces;
      const surface = surfaces.find(candidate => candidate.type === t);
      if (!surface) return null;
      return {
        type: surface.type,
        x: (surface.v1[0] + surface.v2[0] + surface.v3[0]) / 3,
        y: (surface.v1[1] + surface.v2[1] + surface.v3[1]) / 3,
        z: (surface.v1[2] + surface.v2[2] + surface.v3[2]) / 3,
      };
    }, type);
    assert(painting, `painting surface ${type.toString(16)} missing`);
    await page.evaluate(p => window.__JUMPMAN__.teleport3(p.x, p.y + 20, p.z), painting);
    await page.waitForFunction(()=>window.__JUMPMAN__.actSelector?.timer>10);
    assert.equal(await page.$eval('#act-selector .act-stars', el=>el.children.length),1);
    assert.equal(await page.evaluate(()=>window.__JUMPMAN__.worldName),'castle_inside');
    await page.keyboard.press('Space');
    await page.waitForFunction(world => window.__JUMPMAN__?.debugState.world === world, { timeout: 15000 }, destination);
    if (destination === 'jrb') {
      assert(await page.evaluate(() => window.__JUMPMAN__.debugState.waterSurfaces > 0),
        'water should render when entering Jolly Roger Bay through its painting');
    }
  }
  for (const [select,expected] of [[1,1],[2,2],[5,2]]) {
    // Replay an out-of-order file. Star 5 is collected, while 2 is the next unfinished mission.
    await page.goto(new URL('?level=castle_inside',page.url()).href,{waitUntil:'domcontentloaded'});
    await page.waitForFunction(()=>window.__JUMPMAN__ && !document.querySelector('#start-button').disabled);
    await page.click('#start-button');
    await page.evaluate(()=>{
      const g=window.__JUMPMAN__;
      g.save.collectStar('bob',1,18);
      g.save.collectStar('bob',5);
      g.save.collectStar('bob',7,100);
      const tri=g.playground.collision.surfaces.find(s=>s.type===0xd3 && s.normal[1]>0);
      const [x,y,z]=tri.v1.map((v,i)=>(v+tri.v2[i]+tri.v3[i])/3);
      g.teleport3(x,y+15,z);
    });
    await page.waitForFunction(()=>window.__JUMPMAN__.actSelector?.timer>10);
    assert.deepEqual(await page.$$eval('.act-stars button', buttons=>buttons.map(b=>!b.disabled)),[true,true,false,false,true]);
    assert.equal(await page.$eval('.act-name',el=>el.textContent),'Footrace with Koopa the Quick');
    assert.equal(await page.$eval('.act-score',el=>el.getAttribute('aria-label')==='My Score: 100 coins'),true);
    if(select!==2) {
      const key=select===1?'ArrowLeft':'ArrowRight';
      await page.keyboard.down(key);
      await page.waitForSelector('.act-stars button:nth-child('+select+')[aria-current=true]');
      await page.keyboard.up(key);
    }
    await page.waitForFunction(()=>Array.from(document.querySelectorAll('#act-selector img')).every(img=>img.complete && img.naturalWidth>0));
    if(process.env.ACT_SHOT && select===5) await page.screenshot({path:process.env.ACT_SHOT});
    await page.keyboard.press('j'); // N64 B also confirms.
    await page.waitForFunction(()=>window.__JUMPMAN__.worldName==='bob');
    const loaded=await page.evaluate(()=>{
      const g=window.__JUMPMAN__;
      return [g.act,!!g.playground.objects.manager.findByBehavior('kingBobomb'),!!g.playground.objects.manager.findByBehavior('koopaTheQuick')];
    });
    assert.deepEqual(loaded,[expected,expected===1,expected===2]);
  }
  assert.deepEqual(errors, []);
  console.log('PASS: castle painting warps to all four lobby painting courses');
} finally {
  await browser.close();
}
