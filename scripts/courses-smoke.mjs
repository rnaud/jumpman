import puppeteer from 'puppeteer-core';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
const base = process.env.URL || 'http://127.0.0.1:5174/';
const out = '/tmp/jumpman-courses'; await fs.mkdir(out, {recursive:true});
const browser = await puppeteer.launch({executablePath:'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',headless:true,args:['--no-sandbox','--enable-unsafe-swiftshader','--use-gl=angle','--use-angle=swiftshader'],defaultViewport:{width:1440,height:900}});
const wait = ms => new Promise(r=>setTimeout(r,ms));
try {
 const page=await browser.newPage();
 if(process.env.SITE_PASSWORD) {
  await page.goto(base,{waitUntil:'networkidle0'});
  await page.type('#password',process.env.SITE_PASSWORD);
  await Promise.all([page.waitForNavigation({waitUntil:'networkidle0'}),page.click('form button')]);
 }
 const errors=[]; page.on('pageerror', e=>errors.push(e.message));
 page.on('console',async m=>{if(m.type()==='warn'&&m.text().startsWith('[jumpman]')) {errors.push(m.text()); console.log('WARNING',m.text(),await m.args().at(-1).evaluate(e=>e.message));}});
 for(const level of ['bob','castle_grounds','castle_inside','wf','bbh','ccm','jrb','lll','pss','playground']) {
  await page.goto(new URL(`?level=${level}`,base).href,{waitUntil:'networkidle0',timeout:60000});
  await page.waitForFunction(()=>window.__JUMPMAN__?.debugState.characterLoaded,{timeout:30000});
  let s=await page.evaluate(()=>window.__JUMPMAN__.debugState);
  console.log('COURSE',level,JSON.stringify({world:s.world,pos:s.pos,triangles:s.collisionTriangles,star:s.starPos}));
  assert.equal(s.world,level);assert(s.floorNormal);
  if (level === 'lll') {
    assert(s.objects.some(object => object.behavior === 'bully' && object.active), 'LLL bullies missing');
    assert(s.objects.some(object => object.behavior === 'bigBully' && object.active), 'LLL Big Bully missing');
  }
  if (level === 'ccm') {
    assert(s.objects.some(object => object.behavior === 'penguin' && object.active), 'CCM penguins missing');
    assert(s.objects.some(object => object.behavior === 'mrBlizzard' && object.active), 'CCM Mr. Blizzards missing');
  }
  if (level === 'wf') assert(s.objects.some(object => object.behavior === 'whompKing' && object.active), 'WF Whomp King missing');
  if (level === 'bbh') {
    assert(s.platforms >= 4, 'BBH moving platforms missing');
    assert(s.objects.some(object => object.behavior === 'boo' && object.active), 'BBH Boos missing');
    assert(s.objects.some(object => object.behavior === 'bigBoo' && object.active), 'BBH Big Boo missing');
    assert(s.objects.some(object => object.behavior === 'flame' && object.active), 'BBH flames missing');
  }
  await page.click('#start-button'); await wait(300);
  await page.screenshot({path:`${out}/${level}.png`});
  await page.keyboard.press('Escape'); await page.select('#display-resolution','240');
  s=await page.evaluate(()=>window.__JUMPMAN__.debugState); assert.equal(s.display.width,320);assert.equal(s.display.height,240);assert(s.paused);
  const before=s.pos;await page.keyboard.press('r');await wait(150);assert.deepEqual((await page.evaluate(()=>window.__JUMPMAN__.debugState)).pos,before);
  await page.screenshot({path:`${out}/settings.png`});
  assert((await page.$$eval('#settings .course-links a', links => links.length)) >= 7);
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => !window.__JUMPMAN__.debugState.paused);
  await wait(100); await page.keyboard.down('w');await wait(500); await page.keyboard.up('w');
  s=await page.evaluate(()=>window.__JUMPMAN__.debugState);assert(s.forwardVel>0);
  if (level === 'bob') {
    await page.evaluate(() => {
      const game = window.__JUMPMAN__;
      const { x, top, z } = game.debugState.goombas[0];
      game.teleport3(x, top + 80, z);
    });
    await page.waitForFunction(() => {
      const state = window.__JUMPMAN__.debugState;
      return !state.goombas[0].alive && state.vel[1] > 0;
    }, { polling: 'raf', timeout: 4000 });
    await page.keyboard.press('r');
    assert((await page.evaluate(() => window.__JUMPMAN__.debugState)).goombas.slice(0, 2).every(enemy => enemy.alive));
  }
  if(['castle_grounds','wf','bbh','ccm','jrb','lll','pss'].includes(level)) {
    await page.evaluate(levelName=>{const g=window.__JUMPMAN__;if(levelName==='bbh'||levelName==='wf'||levelName==='lll'){g.missionComplete=true;g.starReady=true;}const [x,y,z]=g.debugState.starPos;g.teleport3(x,y-90,z);}, level);
    await wait(250);s=await page.evaluate(()=>window.__JUMPMAN__.debugState);assert(s.won,`${level} finish star`);assert.equal(s.respawns,0);
    await page.keyboard.press('r');await wait(150);assert(!(await page.evaluate(()=>window.__JUMPMAN__.debugState)).won);
  }
 }
 await page.reload({waitUntil:'networkidle0'});await page.waitForFunction(()=>window.__JUMPMAN__);
 assert.equal((await page.evaluate(()=>window.__JUMPMAN__.debugState)).display.width,320);
 await page.click('#settings-button');await page.click('#modern-preset');
 assert.equal((await page.evaluate(()=>window.__JUMPMAN__.debugState)).display.width,1440);
 await page.setViewport({width:390,height:844});await page.click('#classic-preset');
 assert.equal((await page.evaluate(()=>window.__JUMPMAN__.debugState)).display.width,640);
 await page.screenshot({path:`${out}/mobile-settings.png`});
 assert.deepEqual(errors,[]);console.log('PASS: all courses, native spawn/collision, movement, stars/reset, display presets, persistence, responsive sizing.');
} finally {await browser.close();}
