import { winSummitRace } from './battlefield-missions.mjs';
import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath:process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args:['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  const page=await browser.newPage();
  // Repeat the same enemy and water-bomb timing on every controller replay.
  await page.evaluateOnNewDocument(()=>{
    let seed=64;
    Math.random=()=>((seed=(Math.imul(seed,1664525)+1013904223)>>>0)/0x100000000);
  });
  await page.goto(new URL('?level=bob&act=2',process.env.URL || 'http://localhost:5173/').href,{waitUntil:'networkidle0'});
  await page.click('#start-button');
  const result=await page.evaluate(winSummitRace);
  console.log(result);
  assert.deepEqual(result.star,[3030,4500,-4600]);
  assert.equal(result.peakCameraShake, 40*Math.PI/32768);
  assert(result.saved);
  for (const cannon of [false,true]) {
    await page.goto(new URL('?level=bob&act=2',process.env.URL || 'http://localhost:5173/').href,{waitUntil:'networkidle0'});
    await page.click('#start-button');
    const status=await page.evaluate(cannon=>{
      const g=window.__JUMPMAN__, manager=g.playground.objects.manager, m=g.mario;
      const koopa=manager.findByBehavior('koopaTheQuick'), flag=manager.findByBehavior('koopaFlag');
      const original=g.input.consumeStep;
      const tick=(button=0)=>{
        g.input.consumeStep=()=>({stickX:0,stickY:0,stickMag:0,buttonDown:button,buttonPressed:button});
        g.stepSimulation();
      };
      try {
        if (cannon) {
          const buddy=manager.objects.find(o=>o.behavior==='bobombBuddy' && o.param===1);
          g.teleport3(buddy.pos[0],buddy.pos[1],buddy.pos[2]-90,0);
          for(let i=0;i<10;i++)tick();
          tick(0x4000);
          for(let i=0;i<300 && g.operator;i++)tick(g.operator.text ? 0x8000 : 0);
          for(let i=0;i<16;i++)tick();
        }
        g.teleport3(-4004,0,5450);
        tick(); tick(0x8000);
        for(let i=0;i<52;i++)tick();
        if (cannon) {
          g.teleport3(5376,1590,4610);
          for(let i=0;i<200 && !(m.cannon && m.actionState===2);i++)tick();
          if(!m.cannon)throw Error('Failed to enter cannon');
          tick(0x8000); tick();
          g.teleport3(...flag.pos); tick();
        }
        for(let i=0;i<5000 && koopa.action<5;i++)tick();
        if (koopa.action<5)throw Error('Koopa failed to finish '+JSON.stringify([koopa.pos,koopa.action,koopa.vars,koopa.active,m.pos,flag.vars]));
        g.teleport3(koopa.pos[0],koopa.pos[1],koopa.pos[2]-220);
        for(let i=0;i<100 && koopa.action!==6;i++)tick();
        tick(0x8000);
        return {result:flag.vars.result,action:koopa.action,reward:g.missionComplete};
      } finally {g.input.consumeStep=original;}
    },cannon);
    assert.deepEqual(status,{result:cannon?-1:-2,action:7,reward:false});
    console.log(cannon?'Cannon disqualification':'Koopa won',status);
  }
} finally {await browser.close();}
