import puppeteer from 'puppeteer-core';
import assert from 'node:assert/strict';

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args: ['--no-sandbox', '--enable-unsafe-swiftshader'],
});
try {
  const page = await browser.newPage();
  await page.evaluateOnNewDocument(seed => {
    Math.random = () => ((seed = (Math.imul(seed,1664525)+1013904223) >>> 0) / 0x100000000);
  }, Number(process.env.ROUTE_SEED ?? 64));
  // This save has already pressed the red switch in the Tower of the Wing Cap.
  await page.evaluateOnNewDocument(() => {
    if(!localStorage.getItem('jumpman.save.v1.slot1')) localStorage.setItem('jumpman.save.v1.slot1',
      JSON.stringify({version:3, flags:2, stars:[], cannons:[], bestCoins:{}}));
  });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(new URL('?level=bob&act=5', process.env.URL || 'http://127.0.0.1:5173/').href, {waitUntil:'networkidle0'});
  await page.click('#start-button');
  const result = await page.evaluate(() => {
    const g = window.__JUMPMAN__, m = g.mario, original = g.input.consumeStep;
    const startRespawns=m.respawns;
    let frames = 0;
    const tick = (x=0,y=0,button=0,held=button) => {
      g.input.consumeStep = () => ({stickX:x,stickY:y,stickMag:Math.hypot(x,y),buttonDown:held,buttonPressed:button});
      g.stepSimulation();
      frames++;
      g.renderPos.fromArray(m.pos); g.renderYaw=m.faceAngle[1]; g.updateViewCamera(1/30);
    };
    try {
      const buddy = g.playground.objects.manager.objects.find(o => o.behavior === 'bobombBuddy' && o.param === 1);
      const walk = (x,z,radius=60) => {
        for (let i=0;i<400;i++) {
          const dx=x-m.pos[0], dz=z-m.pos[2], distance=Math.hypot(dx,dz);
          if (distance<radius || m.cannon) return;
          const yaw=Math.atan2(dx,dz)-g.rig.yawS16*Math.PI/32768;
          const mag=Math.min(64,distance*0.75);
          const cannonMound=x===5376;
          const runningJump=cannonMound && m.pos[0]>4900 && m.forwardVel>14;
          const bridgeJump=x===930 && i===0;
          const stuck=i%6===0 && m.forwardVel<5;
          const jump=!(m.action&0x800) && !(cannonMound && m.pos[1]>=1510) && (runningJump || bridgeJump || stuck);
          tick(mag*Math.sin(yaw),-mag*Math.cos(yaw),jump?0x8000:0,0x8000);
        }
        throw Error('Missed waypoint '+JSON.stringify([[x,z],m.pos,m.health,m.respawns,m.action.toString(16),m.forwardVel]));
      };
      walk(buddy.pos[0],buddy.pos[2],125);
      tick(0,0,0x4000);
      if (g.save.cannonOpen('bob')) throw Error('Cannon unlocked before Mario finished talking');
      for (let i=0;i<320 && g.operator;i++) tick(0,0,g.operator.text ? 0x8000 : 0);
      for (let i=0;i<16;i++) tick();
      if (!g.save.cannonOpen('bob')) throw Error('Talking to the operator did not unlock the cannons');
      const follow = points => { for (const point of points) walk(...point); };
      follow([[-4300,5380],[-2800,5480],[-1800,5630],[-900,5630],[930,5630],[1780,5000],[2000,4150],[2550,4150],[3450,4900],[4450,5100],[5376,4610]]);
      for(let i=0;i<24;i++) tick();
      for (let i=0;i<150 && !m.cannon;i++) {
        const dx=5376-m.pos[0], dz=4610-m.pos[2];
        const yaw=Math.atan2(dx,dz)-g.rig.yawS16*Math.PI/32768;
        const mag=Math.min(32,Math.hypot(dx,dz)*0.4);
        tick(mag*Math.sin(yaw),-mag*Math.cos(yaw),i===2 ? 0x8000:0,0x8000);
      }
      for (let i=0;i<200 && !(m.cannon && m.actionState===2);i++) tick();
      if (!m.cannon || m.actionState!==2) throw Error('Failed to enter cannon: '+JSON.stringify([m.pos,m.action]));
      const fire = (yaw,degrees) => {
        const pitch=Math.round(degrees*32768/180);
        for(let i=0;i<60;i++){
          const dx=((m.faceAngle[1]-yaw+32768)&65535)-32768;
          const dy=m.faceAngle[0]-pitch;
          if(Math.abs(dx)<2 && Math.abs(dy)<2)break;
          tick(Math.max(-64,Math.min(64,dx/10)),Math.max(-64,Math.min(64,dy/10)));
        }
        tick(0,0,0x8000);
      };
      // Land in the island tree and jump from its crown to the Wing Cap box.
      fire(Math.round(Math.atan2(4096-5376,1638-4610)*32768/Math.PI),77);
      for (let i=0;i<250 && m.action===0x00880898;i++) tick();
      if (m.action!==0x00100342) throw Error('Missed tree: '+[m.pos,m.action.toString(16)]);
      for (let i=0;i<30 && m.action!==0x08100340;i++) tick();
      for (let i=0;i<60;i++) tick(0,64);
      tick();
      for (let i=0;i<25 && m.action!==0x00100345;i++) tick(0,64);
      if (m.action!==0x00100345) throw Error('Failed to stand on treetop');
      tick(0,0,0x8000);
      const steer=(x,z,jump=false)=>{
        const dx=x-m.pos[0], dz=z-m.pos[2], yaw=Math.atan2(dx,dz)-g.rig.yawS16*Math.PI/32768;
        const mag=Math.min(64,Math.hypot(dx,dz)*0.8);
        tick(mag*Math.sin(yaw),-mag*Math.cos(yaw),jump?0x8000:0,0x8000);
      };

      const box=g.playground.objects.manager.objects.find(o=>o.behavior==='wingBox' && o.home[1]===3340);
      for(let i=0;i<450 && !m.wingCapTimer;i++){
        const [x,,z]=box.link?.active ? box.link.pos : box.home;
        steer(x,z,i%24===0 && Math.hypot(x-m.pos[0],z-m.pos[2])<150);
      }
      if(!m.wingCapTimer) throw Error('Missed island Wing Cap '+[m.pos,box.pos,box.action]);
      walk(4352,1229);
      for(let i=0;i<220 && !(m.cannon && m.actionState===2);i++) steer(4352,1229);
      if(!m.cannon || m.actionState!==2) throw Error('Missed island cannon '+m.pos);
      if(g.secrets.size) throw Error('A ring was touched before the island cannon shot');
      fire(-0x4000,19.5);
      for(let i=0;i<130 && g.secrets.size<5;i++){
        if(m.action!==0x00880898 && m.action!==0x10880899) throw Error('Mario landed before reaching all five rings');
        tick();
      }
      if(g.secrets.size!==5) throw Error('Missed rings '+JSON.stringify([[...g.secrets],m.pos]));
      for(let i=0;i<150 && g.secretTimer<120;i++) tick();
      const reward=g.playground.sideStars.find(s=>s.secret).group;
      if(!reward.visible) throw Error('Missing flight reward');
      for(let i=0;i<650 && !g.save.hasStar('bob',5);i++){
        const dx=reward.position.x-m.pos[0], dz=reward.position.z-m.pos[2];
        if(m.action===0x10880899){
          if(Math.abs(dx)<100 && Math.abs(dz)<100) tick(0,0,0x2000);
          else {
            const yaw=Math.atan2(dx,dz)*32768/Math.PI;
            const error=((yaw-m.faceAngle[1]+32768)&65535)-32768;
            tick(Math.max(-64,Math.min(64,-error/80)),28);
          }
        } else steer(reward.position.x,reward.position.z,!(m.action&0x800) && i%6===0);
      }
      if(!g.save.hasStar('bob',5))throw Error('Missed meadow reward '+JSON.stringify([m.pos,reward.position.toArray()]));
      return {frames,rings:g.secrets.size,respawns:m.respawns-startRespawns};
    } finally {g.input.consumeStep=original;}
  });
  await page.waitForFunction(()=>window.__JUMPMAN__?.worldName==='castle_inside',{timeout:90000});
  assert.equal(new URL(page.url()).searchParams.get('entry'),'WARP_NODE_32');
  await page.waitForSelector('#course-result button:not(:disabled)',{visible:true,timeout:90000});
  await page.keyboard.press('Space');
  await page.waitForFunction(()=>document.querySelector('#course-result').hidden);
  await page.reload({waitUntil:'domcontentloaded',timeout:90000});
  await page.waitForFunction(()=>window.__JUMPMAN__?.save.hasStar('bob',5),{timeout:90000});
  console.log(result);
  assert.equal(result.respawns,0);
  assert.deepEqual(errors,[]);
}finally{await browser.close();}
