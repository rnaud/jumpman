import {installSlideDriver} from './slide-driver.mjs';
import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser=await puppeteer.launch({executablePath:process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',args:['--no-sandbox','--enable-unsafe-swiftshader']});
try{
  const page=await browser.newPage();
  await page.goto(new URL('?level=ccm&act=3&area=2',process.env.URL || 'http://localhost:5173/').href,{waitUntil:'networkidle0'});
  await page.click('#start-button');
  await installSlideDriver(page);
  const run=await page.evaluate(()=>{
    const g=window.__JUMPMAN__,m=g.mario,manager=g.playground.objects.manager;
    const o=manager.findByBehavior('racingPenguin'),race=manager.penguinRace;
    const original=g.input.consumeStep;
    const tick=(target,pressed=0,down=pressed)=>{
      const yaw=target?Math.atan2(target[0]-m.pos[0],target[2]-m.pos[2])-g.rig.yawS16*Math.PI/32768:0;
      g.input.consumeStep=()=>({stickX:target?64*Math.sin(yaw):0,stickY:target?-64*Math.cos(yaw):0,stickMag:target?64:0,buttonDown:down,buttonPressed:pressed});
      g.stepSimulation();
    };
    try{
      g.teleport3(o.pos[0]-240,6656,o.pos[2]);
      for(let t=0;t<50 && o.action!==1;t++)tick();
      if(o.action!==1)throw Error('No proposition '+[o.pos,m.pos,o.action]);
      tick(null,0x8000);
      for(let t=0;t<60 && o.action!==3;t++)tick();
      if(o.action!==3)throw Error('No starting gun');
      const points=window.driveMountainSlide(g,tick,true);
      for(let t=0;t<120;t++)tick([race.finish[0],0,race.finish[2]-400]);
      for(let t=0;t<4000 && o.action<5;t++)tick();
      if(o.action<5)throw Error('Racer stalled '+[o.action,o.vars.waypoint,o.pos,o.forwardVel]);
      for(let t=0;t<600 && o.action<6;t++)tick(o.pos);
      for(let t=0;t<120;t++)tick();
      const reward=g.playground.sideStars.find(r=>r.penguin);
      if(!reward.group.visible)throw Error('Clean victory did not reveal star');
      const star=reward.group.position.toArray();
      for(let t=0;t<500 && !g.save.hasStar('ccm',3);t++){
        const jump=Math.hypot(m.pos[0]-star[0],m.pos[2]-star[2])<150 && !(m.action&0x800);
        tick(star,jump?0x8000:0,jump || m.action&0x800 ? 0x8000 : 0);
      }
      return {state:o.action,won:o.vars.won,points,saved:g.save.hasStar('ccm',3)};
    } finally{g.input.consumeStep=original}
  });
  console.log(run);
  assert.equal(run.state,6);
  assert(run.saved);
  await page.waitForFunction(()=>window.__JUMPMAN__.worldName==='castle_inside');
  for (const scenario of ['loss','shortcut','airtime']) {
    const verdict=await page.evaluate(async scenario=>{
      const g=window.__JUMPMAN__;
      await g.loadCourse('ccm',null,false,'none',3,2);
      const m=g.mario, manager=g.playground.objects.manager, racer=manager.findByBehavior('racingPenguin');
      const race=manager.penguinRace,original=g.input.consumeStep;
      const tick=(button=0)=>{
        g.input.consumeStep=()=>({stickX:0,stickY:0,stickMag:0,buttonPressed:button,buttonDown:button});
        g.stepSimulation();
      };
      try{
        g.teleport3(racer.pos[0]-240,6656,racer.pos[2]);
        for(let t=0;t<20 && racer.action!==1;t++)tick();
        tick(0x4000);
        if(racer.action!==0)throw Error('Declining started the race');
        for(let t=0;t<80 && racer.action!==1;t++)tick();
        tick(0x8000);
        for(let t=0;t<60 && racer.action!==3;t++)tick();
        if(scenario==='shortcut'){g.teleport3(...race.shortcut);tick();}
        if(scenario==='airtime'){
          g.teleport3(race.finish[0],2500,race.finish[2]);
          m.setAction(0x0100088c);
          for(let t=0;t<70;t++)tick();
        }
        if(scenario!=='loss')g.teleport3(race.finish[0],race.finish[1],race.finish[2]-200);
        for(let t=0;t<3500 && racer.action<5;t++)tick();
        g.teleport3(racer.pos[0]-220,racer.pos[1],racer.pos[2]);
        for(let t=0;t<400 && racer.action<6;t++)tick();
        for(let t=0;t<120;t++)tick();
        return {ended:racer.action===6,first:!!racer.vars.won,cheated:!!racer.vars.cheated,
          reward:g.playground.sideStars.some(s=>s.penguin && s.group.visible)};
      }finally{g.input.consumeStep=original}
    },scenario);
    assert.deepEqual(verdict,{ended:true,first:scenario!=='loss',cheated:scenario!=='loss',reward:false});
    console.log(scenario,verdict);
  }
}finally{await browser.close()}
