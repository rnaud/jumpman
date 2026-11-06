import assert from 'node:assert/strict';
import {installSlideDriver} from './slide-driver.mjs';
import puppeteer from 'puppeteer-core';

const browser=await puppeteer.launch({executablePath:process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',args:['--no-sandbox','--enable-unsafe-swiftshader']});
try{
  const page=await browser.newPage();
  await page.goto(new URL('?level=castle_inside',process.env.URL || 'http://localhost:5173/').href,{waitUntil:'networkidle0'});
  await page.click('#start-button');
  await page.evaluate(()=>{
    const g=window.__JUMPMAN__,s=g.playground.collision.surfaces.find(t=>t.type===0xd6);
    const p=s.v1.map((v,i)=>(v+s.v2[i]+s.v3[i])/3);
    g.teleport3(p[0],p[1]+20,p[2]);
  });
  await page.waitForFunction(()=>window.__JUMPMAN__.worldName==='ccm');
  await installSlideDriver(page);
  const result=await page.evaluate(()=>{
    const g=window.__JUMPMAN__,m=g.mario,original=g.input.consumeStep;
    const tick=(target,mag=64,jump=false)=>{
      const angle=target ? Math.atan2(target[0]-m.pos[0],target[2]-m.pos[2])-g.rig.yawS16*Math.PI/32768 : 0;
      g.input.consumeStep=()=>({stickX:target?mag*Math.sin(angle):0,stickY:target?-mag*Math.cos(angle):0,stickMag:target?mag:0,buttonDown:jump || (m.action & 0x800)?0x8000:0,buttonPressed:jump?0x8000:0});
      g.stepSimulation();
    };
    try{
      const reward=g.playground.sideStars.find(r=>r.secret);
      if(reward.group.visible)throw Error('Slide reward visible before entering cabin');
      const chimney=[-181,2918,-1486];
      const runUp=[-250,2560,-2300];
      for(let t=0;t<250 && Math.hypot(m.pos[0]-runUp[0],m.pos[2]-runUp[2])>100;t++)tick(runUp);
      for(let t=0;t<500 && g.playground.area===1;t++){
        const distance=Math.hypot(m.pos[0]-chimney[0],m.pos[2]-chimney[2]);
        tick(distance>30?chimney:null,64,distance<650 && !(m.action & 0x800));
      }
      if(g.playground.area!==2)throw Error('Chimney did not lead to slide '+[m.pos,m.action,m.floorHeight]);
      const start=[-5192,6656,-6075];
      for(let t=0;t<240 && Math.hypot(m.pos[0]-start[0],m.pos[2]-start[2])>16;t++)tick(start);
      for(let t=0;t<40;t++)tick();
      const points=window.driveMountainSlide(g,tick);
      const door=g.playground.doors[0];
      for(let t=0;t<400 && g.playground.area===2;t++)tick(door.pos);
      if(g.playground.area!==1)throw Error('Exit door failed '+[m.pos,m.action]);
      for(let t=0;t<120;t++)tick();
      if(!reward.group.visible)throw Error('Cabin exit did not trigger reward');
      const star=reward.group.position.toArray();
      for(let t=0;t<500 && !g.save.hasStar('ccm',1);t++) {
        const dist=Math.hypot(star[0]-m.pos[0],star[2]-m.pos[2]);
        tick(star,64,dist<150 && !(m.action & 0x800));
      }
      if(!g.save.hasStar('ccm',1))throw Error('Failed to reach star '+[...m.pos,...star]);
      return {points,coins:g.coinsCollected,saved:g.save.hasStar('ccm',1)};
    }finally{g.input.consumeStep=original}
  });
  assert(result.saved && result.coins>0);
  console.log(result);
  await page.waitForFunction(()=>window.__JUMPMAN__.worldName==='castle_inside');
  await page.evaluate(async()=>{
    const g=window.__JUMPMAN__;
    await g.loadCourse('ccm','WARP_NODE_0A');
    const reward=g.playground.sideStars.find(r=>r.secret);
    if(reward.group.visible || g.coinsCollected || !g.save.hasStar('ccm',1)) throw Error('Incorrect return-visit state');
  });
}finally{await browser.close()}
