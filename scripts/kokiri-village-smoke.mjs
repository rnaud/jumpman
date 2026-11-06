import assert from 'node:assert/strict';
import {writeFile} from 'node:fs/promises';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args: ['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  for (const character of ['link','sm64']) {
    const context = await browser.createBrowserContext();
    const page = await context.newPage();
    const errors=[];
    page.on('pageerror', e => errors.push(e.message));
    page.on('console', m => { if (m.type()==='error') console.error(m.text()); });
    await page.goto(new URL('?level=links_house&character='+character,process.env.URL || 'http://localhost:5173'), {waitUntil:'domcontentloaded'});
    const loaded = id => page.waitForFunction((id,c) => {
      const g = window.__JUMPMAN__;
      return g && !g.loadingCourse && g.debugState.world === id && g.debugState.character === c;
    }, {timeout:90000},id,character);
    await loaded('links_house');
    await page.click('#start-button');
    const shot = async label => {
      if (!process.env.SHOT_DIR) return;
      const image = await page.evaluate(() => {
        const g=window.__JUMPMAN__;
        g.renderPos.fromArray(g.mario.pos);
        g.updateViewCamera(1/30);
        g.character.update(g.mario,1/30,g.renderPos,g.mario.faceAngle[1]);
        g.renderer.render(g.scene,g.camera);
        return g.renderer.domElement.toDataURL();
      });
      await writeFile(`${process.env.SHOT_DIR}/kokiri-${character}-${label}.png`, Buffer.from(image.split(',')[1],'base64'));
    };
    const walk = async (x,z,radius=65) => {
      await page.evaluate(([x,z,radius])=>{
        const g=window.__JUMPMAN__, m=g.mario;
        const original=g.input.consumeStep;
        try {
          for (let i=0;i<480;i++) {
            const dx=x-m.pos[0], dz=z-m.pos[2], distance=Math.hypot(dx,dz);
            if (g.loadingCourse || distance<radius) return;
            const yaw=Math.atan2(dx,dz)-g.rig.yawS16*Math.PI/32768;
            const mag=Math.min(64,Math.max(32,distance/2));
            g.input.consumeStep=()=>({stickX:mag*Math.sin(yaw),stickY:-mag*Math.cos(yaw),stickMag:mag,buttonPressed:0,buttonDown:0});
            g.stepSimulation();
          }
          throw Error('Walk blocked at '+JSON.stringify(m.pos)+' toward '+[x,z]+' on '+g.debugState.world);
        } finally {g.input.consumeStep=original;}
      },[x,z,radius]);
    };
    const exit = async (to) => {
      const target=await page.evaluate(to=>{
        const g=window.__JUMPMAN__, w=g.playground;
        const tag=Number(Object.entries(w.kokiri.exits).find(([,e])=>e.course===to)[0]);
        const f=w.collision.surfaces.find(s=>s.room===tag && s.normal[1]>.8);
        return [(f.v1[0]+f.v2[0]+f.v3[0])/3,(f.v1[2]+f.v2[2]+f.v3[2])/3];
      },to);
      await walk(...target,6);
      await loaded(to);
    };
    const talk = async (kind) => {
      const spot=await page.evaluate(kind=>{
        const g=window.__JUMPMAN__;
        return g.playground.kokiri.talks.filter(s=>s.kind===kind).sort((a,b)=>Math.hypot(a.pos[0]-g.mario.pos[0],a.pos[2]-g.mario.pos[2])-Math.hypot(b.pos[0]-g.mario.pos[0],b.pos[2]-g.mario.pos[2]))[0];
      },kind);
      await walk(spot.pos[0],spot.pos[2],115);
      for (let i=0;i<25;i++) await page.evaluate(()=>window.__JUMPMAN__.stepSimulation());
      await page.keyboard.press('Space');
      await page.waitForSelector('#kokiri-dialogue:not([hidden])',{timeout:4000});
      assert.equal(await page.$eval('#kokiri-dialogue strong',e=>e.textContent),kind==='saria'?'Saria':'Sign');
      if (kind==='saria') {
        assert.match(await page.$eval('#kokiri-dialogue p',e=>e.textContent),new RegExp(character==='sm64'?'Mario':'Link'));
        await shot('saria');
      } else {
        assert.match(await page.$eval('#kokiri-dialogue p',e=>e.textContent),/Link.s House/);
      }
      let lines=0;
      while (await page.$eval('#kokiri-dialogue',e=>!e.hidden)) {
        await page.keyboard.press('Space');
        await page.evaluate(()=>window.__JUMPMAN__.stepSimulation());
        assert(++lines<=24);
      }
    };
    await shot('home');
    await walk(275,110);
    await exit('kokiri_forest');
    const ladder = await page.evaluate(()=>window.__JUMPMAN__.playground.kokiri.ladder);
    await walk(ladder[1][0],ladder[1][2],75);
    await page.keyboard.press('Space');
    await page.waitForFunction(y=>Math.abs(window.__JUMPMAN__.mario.pos[1]-y)<40,{},ladder[0][1]);
    await talk('saria');
    await shot('forest');
    await talk('sign');
    // Follow the ramp from Link's clearing, cross the village and approach the shop.
    for (const [x,z] of [[-7900,5500],[-7600,5100],[-7600,3700],[-6100,2200],
                          [-5900,2200],[-5400,1700],[-5200,1700],[-4900,1400],[-4800,1150],[-4400,200]]) {
      await walk(x,z,60);
    }
    await exit('kokiri_shop');
    await shot('shop');
    await exit('kokiri_forest');
    const state=await page.evaluate(()=>window.__JUMPMAN__.debugState);
    assert.equal(state.character,character);
    assert.equal(state.respawns,0);
    assert.equal(new URL(page.url()).searchParams.get('entry'),'shop');
    assert.deepEqual(errors,[]);
    console.log(character+' completed the Kokiri round trip');
    await context.close();
  }
} finally { await browser.close(); }
