import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';
import { climbBattlefieldSummit } from './battlefield-climb.mjs';
import { fightSummitKing, winSummitRace } from './battlefield-missions.mjs';

// Walk on native collision. Door animations and paintings trigger the actual level loads.
function walkCastle(points) {
  const g=window.__JUMPMAN__, m=g.mario, original=g.input.consumeStep;
  try {
    for(const [x,z] of points) {
      for(let i=0;i<400;i++) {
        if(g.enteringDoor || g.enteringPainting || g.loadingCourse) return;
        const dx=x-m.pos[0], dz=z-m.pos[2], distance=Math.hypot(dx,dz);
        if(distance<35) break;
        if(i===399) throw Error('Castle path stalled '+JSON.stringify({target:[x,z],pos:m.pos,action:m.action.toString(16)}));
        const yaw=Math.atan2(dx,dz)-g.rig.yawS16*Math.PI/32768;
        const mag=Math.min(64,distance);
        const jump=x===-5422 && m.pos[0]<-4760 && !(m.action&0x800) && i%4===0 ? 0x8000 : 0;
        g.input.consumeStep=()=>({stickX:mag*Math.sin(yaw),stickY:-mag*Math.cos(yaw),stickMag:mag,buttonDown:0x8000,buttonPressed:jump});
        g.stepSimulation();
        g.renderPos.fromArray(m.pos);
        g.renderYaw=m.faceAngle[1];
        g.updateViewCamera(1/30);
      }
    }
  } finally {g.input.consumeStep=original;}
}

const browser=await puppeteer.launch({
  executablePath:process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args:['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  const page=await browser.newPage();
  await page.evaluateOnNewDocument(()=>{
    let seed=64;
    Math.random=()=>((seed=(Math.imul(seed,1664525)+1013904223)>>>0)/0x100000000);
  });
  await page.goto(process.env.URL || 'http://localhost:5173/',{waitUntil:'networkidle0'});
  await page.click('#start-button');
  assert.equal(await page.evaluate(()=>window.__JUMPMAN__.save.stars),0);
  await page.evaluate(walkCastle,[[0,3700],[0,1600],[0,0],[0,-1600],[-76,-3155]]);
  await page.waitForFunction(()=>window.__JUMPMAN__.worldName==='castle_inside');
  await page.evaluate(walkCastle,[[-1100,1000],[-2300,500],[-2850,0],[-2850,-793],[-3350,-793],[-4150,-465],[-5422,-461]]);
  async function selectAct(act) {
    await page.waitForFunction(()=>window.__JUMPMAN__.actSelector?.timer>10);
    assert.equal(await page.$eval('.act-stars button[aria-current=true]',b=>Array.from(b.parentNode.children).indexOf(b)+1),act);
    await page.keyboard.press('Space');
    await page.waitForFunction(n=>window.__JUMPMAN__.worldName==='bob' && window.__JUMPMAN__.act===n,{},act);
  }
  async function saveAndReturn(act) {
    await page.waitForSelector('#course-result button:not(:disabled)',{visible:true,timeout:90000});
    await page.keyboard.press('Space');
    await page.waitForFunction(()=>document.querySelector('#course-result').hidden);
    assert.equal(await page.evaluate(()=>window.__JUMPMAN__.worldName),'castle_inside');
    assert(await page.evaluate(n=>window.__JUMPMAN__.save.hasStar('bob',n),act));
  }
  await selectAct(1);
  await page.evaluate(climbBattlefieldSummit);
  const fight=await page.evaluate(fightSummitKing);
  assert.deepEqual(fight.health,[3,2,1,0]);
  assert(fight.saved && fight.respawns===0);
  await saveAndReturn(1);
  await page.reload({waitUntil:'networkidle0'});
  await page.waitForFunction(()=>window.__JUMPMAN__ && !document.querySelector('#start-button').disabled);
  if (!await page.evaluate(()=>window.__JUMPMAN__.started)) await page.click('#start-button');
  await page.evaluate(walkCastle,[[-4750,-461],[-5422,-461]]);
  await selectAct(2);
  const race=await page.evaluate(winSummitRace);
  assert(race.saved);
  await saveAndReturn(2);
  console.log('Fresh file: castle grounds, front door, painting, King, save/reload, painting, Koopa race.');
} finally {await browser.close();}
