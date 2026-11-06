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
  const result=await page.evaluate(()=>{
    const g=window.__JUMPMAN__, m=g.mario;
    const manager=g.playground.objects.manager, koopa=manager.findByBehavior('koopaTheQuick'), flag=manager.findByBehavior('koopaFlag');
    const original=g.input.consumeStep;
    let peakCameraShake = 0;
    const tick=(button=0,x=0,y=0,held=button)=>{
      g.input.consumeStep=()=>({stickX:x,stickY:y,stickMag:Math.hypot(x,y),buttonDown:held,buttonPressed:button});
      g.stepSimulation();
      peakCameraShake = Math.max(peakCameraShake, Math.abs(g.ballShake.radians));
      g.renderPos.fromArray(m.pos); g.renderYaw=m.faceAngle[1]; g.updateViewCamera(1/30);
    };
    let dodgeCount = 0;
    const drive=(target,radius,limit)=>{
      for(let t=0;t<limit;t++){
        const dx=target[0]-m.pos[0], dz=target[2]-m.pos[2], distance=Math.hypot(dx,dz);
        if(distance<radius) return;
        const yaw=Math.atan2(dx,dz)-g.rig.yawS16*Math.PI/32768;
        const hazard = manager.objects.some(ball => {
          if (!ball.active || !['mountainBall','pitBowlingBall'].includes(ball.behavior)) return false;
          if (Math.abs(ball.pos[1]-m.pos[1]) > 210) return false;
          const ballYaw = ball.moveYaw*Math.PI/32768;
          const vx = ball.forwardVel*Math.sin(ballYaw)-m.vel[0], vz = ball.forwardVel*Math.cos(ballYaw)-m.vel[2];
          const bx = ball.pos[0]-m.pos[0], bz = ball.pos[2]-m.pos[2];
          const time = -(bx*vx+bz*vz)/Math.max(1,vx*vx+vz*vz);
          return time > 0 && time <= 11 && Math.hypot(bx+time*vx,bz+time*vz)<170;
        });
        // Keep both feet on the trail through the tight turn, then jump once facing uphill.
        const upperBend = m.pos[0]>2750 && m.pos[2]>-4200 && m.pos[1]>2850;
        const dodge = hazard && !upperBend && !(m.action&0x800);
        if (dodge) dodgeCount++;
        const jump = (target[0] === 930 && t === 0) || dodge || (m.forwardVel<7 && !(m.action&0x800) && t%4===0);
        tick(jump?0x8000:0,64*Math.sin(yaw),-64*Math.cos(yaw),0x8000);
        if(koopa.action===1 || koopa.action===6)return;
      }
      throw Error('Missed waypoint: '+JSON.stringify([target,m.pos,m.action.toString(16),koopa.action]));
    };
    try {
      drive(koopa.pos,370,600);
      tick();
      tick(0x4000);
      if (koopa.action!==0) throw Error('Decline failed');
      for(let i=0;i<65;i++)tick();
      if (koopa.action!==1) throw Error('No repeat invitation');
      tick(0x8000);
      for(let i=0;i<50;i++)tick();
      if (koopa.action!==2) throw Error('Early start');
      tick();
      if (koopa.action!==3) throw Error('Missing starting gun');
      const route=[[-2800,150,5480],[-1800,255,5630],[-900,256,5630],[930,768,5630],
        [2000,768,4970],[2000,768,4320],[1240,768,3600],[-280,768,2720],[-1560,768,1770],
        [-2303,768,1024],[-3050,768,280],[-3760,768,350],[-4750,768,100],[-4900,1024,-600],
        [-4350,1024,-2100],[-3900,935,-3800],[-3350,800,-4470],[-2900,870,-4500],[-2550,990,-4900],[-2150,980,-5100],[-1550,800,-4800],
        [-900,730,-4200],[-550,717,-3700],[0,850,-3330],[160,1040,-2780],[700,1093,-2700],[1400,1206,-2400],[1900,1314,-1900],
        [2700,1516,-1800],[3700,1720,-1700],[4600,1809,-2500],[6240,1943,-3500],
        [6380,2066,-6180],[4000,2390,-7720],[2975,2466,-7425],[1549,2525,-6600],[700,2556,-6150],[-350,2608,-5515],[-385,2620,-5200],
        [-180,2641,-4860],[580,2769,-3380],[1670,2867,-2580],[2216,2900,-2126],[2450,2917,-2005],[2590,2948,-1970],[2800,2955,-1900],[3040,2961,-1860],[3167,2965,-1870],
        [3760,3010,-2140],[4040,3020,-2670],[4380,2990,-2940],[4680,3010,-3200],[4880,3037,-3416],
        [5000,3149,-3400],[5060,3185,-3712],[5040,3195,-4650],[5020,3209,-5481],[5020,3181,-5760],[4660,3354,-6300],[3940,3514,-6800],[3200,3619,-6850],[1290,3768,-5793],
        [1150,3900,-3670],[2980,4046,-2930],[4240,4180,-3600],[4390,4190,-3810],[4390,4220,-4030],[4180,4242,-4220],[3660,4242,-4380]];
      for(const point of route) drive(point,65,350);
      drive(flag.pos,100,200);
      for(let i=0;i<4500 && koopa.action<5;i++)tick();
      drive(koopa.pos,240,240);
      for(let i=0;i<100 && koopa.action!==6;i++)tick();
      if (koopa.action!==6) throw Error('No finish dialogue: '+JSON.stringify([koopa.pos,koopa.vars,koopa.action,flag.vars,m.pos]));
      if (g.missionComplete) throw Error('Early reward');
      tick(0x8000);
      for(let i=0;i<120;i++)tick();
      const star=g.playground.star.position.toArray();
      drive(star,65,240);
      for(let i=0;i<90 && !g.save.hasStar('bob',2);i++)tick(i%30===0?0x8000:0,0,0,0x8000);
      return {star,dodgeCount,peakCameraShake,saved:g.save.hasStar('bob',2),finish:koopa.pos};
    } finally {g.input.consumeStep=original;}
  });
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
