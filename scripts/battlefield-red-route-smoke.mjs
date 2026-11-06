import puppeteer from 'puppeteer-core';
import assert from 'node:assert/strict';

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args: ['--no-sandbox', '--enable-unsafe-swiftshader'],
});
try {
  const page = await browser.newPage();
  await page.evaluateOnNewDocument(() => {
    let seed = 64;
    Math.random = () => ((seed = (Math.imul(seed,1664525)+1013904223) >>> 0) / 0x100000000);
  });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(new URL('?level=bob&act=4', process.env.URL || 'http://127.0.0.1:5173/').href, {waitUntil:'networkidle0'});
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
          const jump=!(m.action&0x800) && (i%6===0 && m.forwardVel<5 || x===930 && i===0);
          tick(mag*Math.sin(yaw),-mag*Math.cos(yaw),jump?0x8000:0,0x8000);
        }
        throw Error('Missed waypoint '+JSON.stringify([[x,z],m.pos,m.health,m.respawns]));
      };
      walk(buddy.pos[0],buddy.pos[2],125);
      tick(0,0,0x4000);
      if (g.save.cannonOpen('bob')) throw Error('Cannon unlocked before Mario finished talking');
      for (let i=0;i<320 && g.operator;i++) tick(0,0,g.operator.text ? 0x8000 : 0);
      for (let i=0;i<16;i++) tick();
      if (!g.save.cannonOpen('bob')) throw Error('Talking to the operator did not unlock the cannons');
      const follow = points => { for (const point of points) walk(...point); };
      follow([[-4300,5380],[-2800,5480],[-1800,5630],[-900,5630],[930,5630],
        [1780,5000],[2000,4150],[2550,4150],[3450,4900],[4450,5100],[5376,4610]]);
      for (let i=0;i<150 && !m.cannon;i++) {
        const dx=5376-m.pos[0], dz=4610-m.pos[2];
        const yaw=Math.atan2(dx,dz)-g.rig.yawS16*Math.PI/32768;
        const mag=Math.min(48,Math.hypot(dx,dz)*0.3);
        tick(mag*Math.sin(yaw),-mag*Math.cos(yaw));
      }
      for (let i=0;i<200 && !(m.cannon && m.actionState===2);i++) tick();
      if (!m.cannon || m.actionState!==2) throw Error('Failed to enter cannon: '+[m.pos,m.action,m.floorHeight]);
      const treeYaw = Math.round(Math.atan2(4096-5376,1638-4610)*32768/Math.PI);
      const pitch = Math.round(77*32768/180);
      for (let i=0;i<40;i++) {
        const dx=((m.faceAngle[1]-treeYaw+32768)&65535)-32768, dy=m.faceAngle[0]-pitch;
        if (Math.abs(dx)<2 && Math.abs(dy)<2) break;
        tick(Math.max(-64,Math.min(64,dx/10)),Math.max(-64,Math.min(64,dy/10)));
      }
      tick(0,0,0x8000);
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
      const pickup=(index, jump=true)=>{
        const coin=g.playground.redCoins[index];
        for(let i=0;i<450 && !coin.collected;i++) {
          const {x,z}=coin.position, distance=Math.hypot(x-m.pos[0],z-m.pos[2]);
          steer(x,z,jump && !(m.action&0x800) && (distance<500 || m.forwardVel<4) && i%5===0);
        }
        if(!coin.collected) throw Error('Missed coin '+JSON.stringify([index,coin.position.toArray(),m.pos,m.action.toString(16),g.redCoinsCollected]));
      };
      // Leave the island at the east edge, then walk up the stone beneath it.
      pickup(7);
      for(let i=0;i<180 && m.pos[1]>1600;i++) steer(5900,700);
      pickup(6,false);
      walk(3200,3600);
      walk(2000,4150);
      pickup(4);
      // Cross the bridge again to approach the Chomp post from the upper field.
      follow([
        [1000,4510],[-1000,4300],[-2900,3500],[-4300,4400],[-4300,5380],[-2800,5480],
        [-1800,5630],[-900,5630],[930,5630],[1780,5000],[2000,4150]
      ]);
      walk(1240,3600);
      walk(-280,2720);
      pickup(3);
      walk(-1560,1770);
      walk(-2303,1024);
      walk(-3050,280);
      walk(-3760,350);
      pickup(0); pickup(1);
      walk(-4750,100);
      walk(-4900,-600);
      walk(-4350,-2100);
      // Drop through the opening north of the bridge to reach the coin underneath.
      walk(-3800,-4300); walk(-3160,-5060);
      for (let i=0;i<65;i++) steer(-3160,-5060);
      pickup(5);
      // Climb out of the pit and take the mountain switchbacks above the slope coin.
      follow([
        [-3160,-5060],[-3450,-5400],[-3700,-5800],[-4100,-6050],[-4500,-5600],[-4350,-4800],
        [-3900,-3800],[-3350,-4470],[-2900,-4500],[-2550,-4900],[-2150,-5100],[-1550,-4800],
        [-900,-4200],[-550,-3700],[0,-3330],[160,-2780],[700,-2700],[1400,-2400],
        [1900,-1900],[2700,-1800],[3700,-1700],[4600,-2500],[6240,-3500],[6380,-6180],
        [4000,-7720],[3000,-7460],[2100,-7250]
      ]);
      pickup(2);
      for(let i=0;i<180 && g.redStarTimer!==120;i++)tick();
      if(g.redStarTimer!==120 || !g.starReady) throw Error('Missing red-coin reward');
      follow([
        [-1800,-6240],[-2150,-5100],[-2550,-4900],[-2900,-4500],[-3350,-4470],[-3900,-3800],
        [-4350,-2100],[-4900,-600],[-4750,100],[-5500,2200]
      ]);
      const reward=g.playground.star.position;
      for(let i=0;i<250 && !g.save.hasStar('bob',4);i++){
        steer(reward.x,reward.z,!(m.action&0x800) && i%5===0);
      }
      if (!g.save.hasStar('bob',4)) throw Error('Reward unreachable '+JSON.stringify([m.pos,reward.toArray()]));
      if(m.respawns!==startRespawns)throw Error('Mario respawned during the route');
      return {redCoins:g.redCoinsCollected, totalCoins:g.coinsCollected, frames, respawns:m.respawns-startRespawns};
    }finally{g.input.consumeStep=original;}
  });
  await page.waitForFunction(()=>window.__JUMPMAN__.worldName==='castle_inside');
  assert.equal(new URL(page.url()).searchParams.get('entry'),'WARP_NODE_32');
  await page.waitForSelector('#course-result button:not(:disabled)',{visible:true});
  await page.keyboard.press('Space');
  await page.waitForFunction(()=>document.querySelector('#course-result').hidden);
  await page.reload({waitUntil:'networkidle0'});
  assert(await page.evaluate(()=>window.__JUMPMAN__.save.hasStar('bob',4)));
  console.log(result);
  assert.equal(result.redCoins,8);
  assert.deepEqual(errors,[]);
}finally{await browser.close();}
