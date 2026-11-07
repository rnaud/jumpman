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
  await page.goto(new URL('?level=bob&act=3', process.env.URL || 'http://127.0.0.1:5173/').href, {waitUntil:'networkidle0'});
  await page.click('#start-button');
  const result = await page.evaluate(() => {
    const g = window.__JUMPMAN__, m = g.mario, original = g.input.consumeStep;
    let raiseFov = 0, maxFlightDistance = 0, flightFrames = 0;
    const tick = (x=0,y=0,button=0,held=button) => {
      g.input.consumeStep = () => ({stickX:x,stickY:y,stickMag:Math.hypot(x,y),buttonDown:held,buttonPressed:button});
      g.stepSimulation();
      if (m.action === 0x00880898) {
        const eye = g.camera.position;
        const distance = Math.hypot(eye.x-m.pos[0],eye.y-m.pos[1]-125,eye.z-m.pos[2]);
        maxFlightDistance = Math.max(maxFlightDistance,distance);
        flightFrames++;
      }
      if (g.cannonCamera) raiseFov = Math.max(raiseFov,g.cannonCamera.fovOffset);
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
          const mound=x===5376;
          const runningJump=mound && m.pos[0]>4900 && m.forwardVel>14;
          const jump=!(m.action&0x800) && !(mound && m.pos[1]>=1510) &&
            (runningJump || i%6===0 && m.forwardVel<5 || x===930 && i===0);
          tick(mag*Math.sin(yaw),-mag*Math.cos(yaw),jump?0x8000:0,0x8000);
        }
        throw Error('Missed waypoint '+JSON.stringify([[x,z],m.pos]));
      };
      walk(buddy.pos[0],buddy.pos[2],125);
      tick(0,0,0x4000);
      if (g.save.cannonOpen('bob')) throw Error('Cannon unlocked before Mario finished talking');
      const pages = new Set();
      let openingTicks = 0, nearestDistance = Infinity;
      for (let i=0;i<320 && g.operator;i++) {
        if (g.operator.text) pages.add(g.operator.text);
        if (g.operator.opening) {
          nearestDistance = Math.min(nearestDistance, Math.hypot(g.camera.position.x+5694,g.camera.position.y-128,g.camera.position.z-5600));
          openingTicks++;
        }
        tick(0,0,g.operator.text ? 0x8000 : 0);
      }
      if (pages.size !== 7 || openingTicks !== 171 || nearestDistance > 305) throw Error('Incomplete cannon preparation '+[pages.size,openingTicks,nearestDistance]);
      for (let i=0;i<16;i++) tick();
      if (!g.save.cannonOpen('bob')) throw Error('Talking to the operator did not unlock the cannons');
      const enterHillside = () => {
        for (const point of [[-4300,5380],[-2800,5480],[-1800,5630],[-900,5630],[930,5630],
          [1780,5000],[2000,4150],[2550,4150],[3450,4900],[4450,5100],[5376,4610]]) walk(...point);
        for (let i=0;i<150 && !m.cannon;i++) {
          const dx=5376-m.pos[0], dz=4610-m.pos[2];
          const yaw=Math.atan2(dx,dz)-g.rig.yawS16*Math.PI/32768;
          const mag=Math.min(48,Math.hypot(dx,dz)*0.3);
          tick(mag*Math.sin(yaw),-mag*Math.cos(yaw));
        }
      };
      enterHillside();
      for (let i=0;i<200 && !(m.cannon && m.actionState===2);i++) tick();
      if (!m.cannon || m.actionState!==2) throw Error('Failed to enter cannon: '+[m.pos,m.action,m.floorHeight]);
      const aimEye = g.camera.position.y-m.pos[1];
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
      const tree=m.usedObj.pos;
      for (let i=0;i<30 && m.action!==0x08100340;i++) tick();
      for (let i=0;i<60;i++) tick(0,64);
      tick();
      for (let i=0;i<25 && m.action!==0x00100345;i++) tick(0,64);
      if (m.action!==0x00100345) throw Error('Failed to stand on treetop');
      tick(0,0,0x8000);
      const islandCoin = g.playground.redCoins.find(coin => coin.position.y === 3672).collected;
      const launch = {tree,islandCoin,launch:m.action,height:m.pos[1],speed:m.forwardVel};
      const box=g.playground.objects.manager.findByBehavior('starBox');
      let revealTicks=0;
      for (let i=0;i<480 && !g.save.hasStar('bob',3);i++) {
        const reward=g.playground.sideStars.find(s=>s.box).group;
        const [x,,z]=box.active ? box.home : reward.position.toArray();
        const distance=Math.hypot(x-m.pos[0],z-m.pos[2]);
        const yaw=Math.atan2(x-m.pos[0],z-m.pos[2])-g.rig.yawS16*Math.PI/32768;
        const mag=distance>90 ? 64 : Math.min(40,distance);
        const a=distance<120 ? 0x8000 : 0;
        g.input.consumeStep=()=>({stickX:mag*Math.sin(yaw),stickY:-mag*Math.cos(yaw),stickMag:mag,
          buttonDown:a,buttonPressed:(i%24===0 ? a : 0)});
        const flight=g.boxFlights.get(3);
        const frozen=flight && !flight.tangible;
        const before=[...m.pos];
        g.stepSimulation();
        if (frozen) {
          revealTicks++;
          if (before.some((v,j)=>v!==m.pos[j])) throw Error('Mario moved during box-star reveal');
          if (flight.pos[0]!==box.home[0] || flight.pos[2]!==box.home[2]) throw Error('Star drifted away from box');
        }
      }
      if (!g.save.hasStar('bob',3)) throw Error('Island star not collected: '+[m.pos,box.active,m.action.toString(16),g.playground.sideStars.find(s=>s.box).group.position.toArray()]);
      // Isolate a second firing angle after the complete spawn-to-star run.
      g.restart();
      walk(buddy.pos[0],buddy.pos[2],125);
      tick(0,0,0x4000);
      const repeatPages = new Set();
      for (let i=0;i<80 && g.operator;i++) {
        if (g.operator.opening) throw Error('Unlocked cannon repeated its opening cutscene');
        if (g.operator.text) repeatPages.add(g.operator.text);
        tick(0,0,g.operator.text ? 0x8000 : 0);
      }
      if (repeatPages.size !== 4) throw Error('Missing return-visit instructions');
      for (let i=0;i<16;i++) tick();
      g.teleport3(5376,1590,4610);
      for (let i=0;i<200 && !(m.cannon && m.actionState===2);i++) tick();
      for (let i=0;i<9;i++) tick(0,-64);
      tick(0,0,0x8000);
      const shot = m.action;
      let flight=0;
      while (flight++ < 260 && m.action===0x00880898) tick();
      const result = {shot,landing:m.action,pos:[...m.pos],wing:m.wingCapTimer,flight};

      return {...result,...launch,openingTicks,pages:pages.size,revealTicks,aimEye,raiseFov,maxFlightDistance,flightFrames,saved:true};
    } finally {g.input.consumeStep=original}
  });
  console.log(result);
  assert(result.flightFrames > 180 && Math.abs(result.maxFlightDistance-800) < 0.01);
  assert.equal(result.raiseFov,1);
  assert(result.islandCoin);
  assert.equal(result.aimEye,125);
  assert.equal(result.shot,0x00880898);
  assert.equal(result.landing,0x00880456);
  assert.equal(result.pos[1],3072);
  assert(Math.hypot(result.pos[0]-5540,result.pos[2]-1200) < 300, 'did not land near the island star box');
  assert(result.revealTicks > 60);
  assert.equal(result.wing,0);
  assert.deepEqual(result.tree,[4096,3072,1638]);
  assert.equal(result.launch,0x0300088d);
  assert(result.height > 3640 && result.speed >= 23);
  assert.deepEqual(errors,[]);
} finally {await browser.close()}
