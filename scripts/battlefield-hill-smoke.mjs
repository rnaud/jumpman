import { climbBattlefieldSummit } from './battlefield-climb.mjs';
import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args: ['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  const page = await browser.newPage();
  await page.evaluateOnNewDocument(()=>{ let seed=64; Math.random=()=>((seed=(Math.imul(seed,1664525)+1013904223)>>>0)/0x100000000); });
  await page.setViewport({width:520,height:360});
  await page.goto(new URL('?level=bob&act=1',process.env.URL || 'http://localhost:5173/').href,{waitUntil:'networkidle0'});
  await page.click('#start-button');
  const results = [];
  for (const pos of [[6380,2066,-6180],[4000,2390,-7720],[580,2769,-3380],[4069,3050,-2487],[3940,3514,-6800],[2980,4046,-2930]]) {
    await page.evaluate(([x,y,z])=>{
      const g=window.__JUMPMAN__, floor=g.playground.collision.findFloor(x,y+75,z);
      g.teleport3(x,floor.height,z);
      for(let i=0;i<250;i++)g.stepSimulation();
    },pos);
    await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
    const result = await page.evaluate(()=>{
      const g=window.__JUMPMAN__, king=g.playground.objects.manager.findByBehavior('kingBobomb');
      const [x,,z]=g.mario.pos, dx=g.camera.position.x-x, dz=g.camera.position.z-z;
      return {context:g.rig.context, eyeDistance:Math.round(Math.hypot(dx,dz)),
        outward:(dx*(x-3072)+dz*(z+4608))/(Math.hypot(dx,dz)*Math.hypot(x-3072,z+4608)),
        kingHome:king.pos.every((v,i)=>v===king.home[i]), intro:king.vars.intro};
    });
    assert.equal(result.context,'radial');
    assert(result.kingHome && !result.intro);
    assert(result.outward>0.8 && result.eyeDistance>(pos[2]<-7500 ? 275 : 350),JSON.stringify({pos,result}));
    results.push(result.eyeDistance);
  }
  // Restart from spawn so the scan above cannot advance the climb's hazards.
  await page.reload({waitUntil:'networkidle0'});
  await page.click('#start-button');
  const climb = await page.evaluate(climbBattlefieldSummit);
  assert(climb.height>4200);
  const fight=await page.evaluate(()=>{
    const g=window.__JUMPMAN__, king=g.playground.objects.manager.findByBehavior('kingBobomb');
    // The climb ends when the King triggers the introduction.
    for(let i=0;i<65 && !g.kingCamera;i++) g.stepSimulation();
    if(!g.kingCamera || g.kingCamera.frame>10) throw Error('Summit shoulder shot never started');
    while(g.kingCamera.frame<10) g.stepSimulation();
    if(g.kingDialog.page!==-1) throw Error('Dialogue opened before camera frame 10');
    g.stepSimulation();
    if(g.kingDialog.page!==0) throw Error('Dialogue did not open on camera frame 10');
    for(let i=0;i<120;i++) g.stepSimulation();
    const eye=g.camera.position, [mx,,mz]=g.mario.pos;
    const distance=Math.hypot(eye.x-mx,eye.z-mz);
    if(Math.abs(distance-180)>3) throw Error('Shoulder distance: '+distance);
    const before=eye.clone();
    g.updateViewCamera(1/30);
    if(before.distanceTo(eye)>0.01) throw Error('Orbit camera overwrote the shoulder shot');
    const initial = document.querySelector('#toast').textContent;
    if (king.vars.intro!==1 || !initial.startsWith("I'm the Big Bob-omb")) throw Error('Intro advanced without input');
    const original = g.input.consumeStep;
    const pages = new Set();
    try {
      for(let i=0;i<120 && king.vars.intro!==2;i++) {
        const pressed = g.kingDialog?.page >= 0 && i%2===0 ? 0x8000 : 0;
        if(pressed) pages.add(document.querySelector('#toast').textContent);
        g.input.consumeStep=()=>({stickX:0,stickY:0,stickMag:0,buttonDown:pressed,buttonPressed:pressed});
        g.stepSimulation();
      }
    } finally {g.input.consumeStep=original;}
    if(pages.size!==6) throw Error('Missing opening pages: '+[...pages]);
    if (king.vars.intro!==2) throw Error('Summit encounter failed to start');
    // Place the King on a lower path mid-throw to exercise the off-hill landing.
    king.pos=[2980,4046,-2930];
    king.heldState=2;
    let returned=false;
    for(let i=0;i<220;i++){
      g.stepSimulation();
      if(king.action===5) returned=true;
      if(returned && king.action!==5) break;
    }
    const landed = [...king.pos];
    for(let i=0;i<65;i++) g.stepSimulation();
    if (g.kingDialog?.id!==128 || !document.querySelector('#toast').textContent.includes('out of the ring')) {
      throw Error('Missing rematch warning after an off-hill throw');
    }
    return {returned,health:king.vars.health,pos:landed,home:king.home};
  });
  assert(fight.returned && fight.health===3);
  assert.deepEqual(fight.pos,fight.home);
  console.log('Controller climb',climb,'Mountain camera distances',results,'Off-hill return',fight);
} finally {await browser.close();}
