import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath:process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args:['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(new URL('?level=bob',process.env.URL || 'http://localhost:5173'),{waitUntil:'domcontentloaded'});
  await page.waitForFunction(()=>window.__JUMPMAN__ && !document.querySelector('#start-button').disabled);
  await page.click('#start-button');
  const die = async (fall, duration) => {
    const result = await page.evaluate(({fall,duration})=>{
      const game = window.__JUMPMAN__, m = game.mario;
      // Isolate the death trigger, then let the regular simulation play every frame.
      if (fall) m.pos[1] = m.deathHeight-100;
      else { m.health = 0x100; m.takeDamage(2,[m.pos[0]+40,m.pos[1],m.pos[2]]); }
      for(let i=0;i<240 && !game.deathTransition;i++) game.stepSimulation();
      if(!game.deathTransition) throw Error('Death never triggered in '+game.worldName+' '+m.action.toString(16));
      const lives = m.lives;
      for(let i=0;i<duration;i++) game.stepSimulation();
      if(game.loadingCourse || m.lives!==lives || document.querySelector('#star-warp').hidden) throw Error('Incorrect wipe duration');
      const dest = {...game.deathTransition.target};
      game.stepSimulation();
      return dest;
    },{fall,duration});
    return result;
  };
  assert.deepEqual(await die(false,48),{course:'castle_inside',area:1,entry:'WARP_NODE_64'});
  await page.waitForFunction(()=>window.__JUMPMAN__.worldName==='castle_inside');
  const landing = await page.evaluate(()=>{
    const game=window.__JUMPMAN__, m=game.mario;
    const seen=new Set([m.action]);
    for(let i=0;i<200 && m.action!==0x0c400201;i++){game.stepSimulation();seen.add(m.action);}
    return {seen:[...seen],lives:m.lives,health:m.health,coins:game.coinsCollected,savePrompt:game.courseResult.visible};
  });
  assert(landing.seen.includes(0x1928) && landing.seen.includes(0x20467));
  assert.deepEqual([landing.lives,landing.health,landing.coins,landing.savePrompt],[3,0x880,0,false]);
  await page.evaluate(()=>window.__JUMPMAN__.loadCourse('bowser_1'));
  assert.deepEqual(await die(true,20),{course:'bitdw',area:1,entry:'WARP_NODE_0C'});
  await page.waitForFunction(()=>window.__JUMPMAN__.worldName==='bitdw');
  await page.waitForFunction(()=>window.__JUMPMAN__.mario.lives===2);
  await page.evaluate(()=>window.__JUMPMAN__.loadCourse('pss'));
  assert.deepEqual(await die(true,20),{course:'castle_inside',area:1,entry:'WARP_NODE_20'});
  await page.waitForFunction(()=>window.__JUMPMAN__.worldName==='castle_inside');
  assert.equal(await page.evaluate(()=>window.__JUMPMAN__.mario.lives),2);
  await page.evaluate(async()=>{
    const g=window.__JUMPMAN__;
    await g.loadCourse('bob');
    g.save.collectStar('bob',1);
    g.save.deferCourseSave();
    g.save.collectStar('bob',2);
    g.mario.lives=0;
  });
  await die(false,48);
  await page.waitForFunction(()=>window.__JUMPMAN__?.worldName==='castle_grounds' && document.querySelector('#start-overlay .overlay-kicker').textContent==='GAME OVER');
  assert.deepEqual(await page.evaluate(()=>{
    const g=window.__JUMPMAN__;
    return [g.mario.lives,g.save.hasStar('bob',1),g.save.hasStar('bob',2),g.started];
  }),[4,true,false,false]);
  assert.deepEqual(errors,[]);
  console.log('PASS: painting death, arena fall, safe slide exit, and last-life game over');
} finally { await browser.close(); }
