import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args: ['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  const page = await browser.newPage();
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
  const climb = await page.evaluate(()=>{
    const g=window.__JUMPMAN__, m=g.mario, original=g.input.consumeStep;
    const path=[[6240,1943,-3500],[6380,2066,-6180],[4000,2390,-7720],[2975,2466,-7425],[1549,2525,-6600],[700,2556,-6150],[-350,2608,-5515],[-385,2620,-5200],
      [-180,2641,-4860],[580,2769,-3380],[1670,2867,-2580],[2216,2900,-2126],[3167,2965,-1870],
      [3760,3010,-2140],[4040,3020,-2670],[4380,2990,-2940],[4680,3010,-3200],[4880,3037,-3416],[5020,3181,-5760],[4660,3354,-6300],[3940,3514,-6800],
      [3200,3619,-6850],[1290,3768,-5793],[1150,3900,-3670],[2980,4046,-2930],[4334,4186,-3680],[4180,4242,-4220],[3660,4242,-4380]];
    const [x,y,z]=path.shift();
    g.teleport3(x,g.playground.collision.findFloor(x,y+80,z).height,z);
    let frames=0;
    try {
      for(const [tx,,tz] of path) {
        for(let i=0;i<240;i++) {
          const dx=tx-m.pos[0], dz=tz-m.pos[2];
          if(Math.hypot(dx,dz)<65)break;
          if(i===239)throw Error('Climb stalled at '+JSON.stringify([tx,tz,m.pos]));
          const yaw=Math.atan2(dx,dz)-g.rig.yawS16*Math.PI/32768;
          const jump=m.forwardVel<7 && !(m.action&0x800) && i%4===0 ? 0x8000 : 0;
          g.input.consumeStep=()=>({stickX:64*Math.sin(yaw),stickY:-64*Math.cos(yaw),stickMag:64,buttonDown:0x8000,buttonPressed:jump});
          g.stepSimulation();
          g.renderPos.fromArray(m.pos);
          g.renderYaw=m.faceAngle[1];
          g.updateViewCamera(1/30);
          frames++;
        }
      }
      return {frames,height:m.pos[1]};
    } finally {g.input.consumeStep=original;}
  });
  assert(climb.height>4200);
  const fight=await page.evaluate(()=>{
    const g=window.__JUMPMAN__, king=g.playground.objects.manager.findByBehavior('kingBobomb');
    const [x,y,z]=king.home;
    g.teleport3(x,y,z+350);
    for(let i=0;i<65;i++) g.stepSimulation();
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
    return {returned,health:king.vars.health,pos:king.pos,home:king.home};
  });
  assert(fight.returned && fight.health===3);
  assert.deepEqual(fight.pos,fight.home);
  console.log('Controller climb',climb,'Mountain camera distances',results,'Off-hill return',fight);
} finally {await browser.close();}
