import puppeteer from 'puppeteer-core';
import assert from 'node:assert/strict';

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args: ['--no-sandbox', '--enable-unsafe-swiftshader'],
});
try {
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(new URL('?level=bob&act=3', process.env.URL || 'http://127.0.0.1:5173/').href, {waitUntil:'networkidle0'});
  await page.click('#start-button');
  const result = await page.evaluate(() => {
    const g = window.__JUMPMAN__, m = g.mario, original = g.input.consumeStep;
    const tick = (x=0,y=0,button=0) => {
      g.input.consumeStep = () => ({stickX:x,stickY:y,stickMag:Math.hypot(x,y),buttonDown:button,buttonPressed:button});
      g.stepSimulation();
    };
    try {
      const buddy = g.playground.objects.manager.objects.find(o => o.behavior === 'bobombBuddy' && o.param === 1);
      g.teleport3(buddy.pos[0],buddy.pos[1],buddy.pos[2]-90,0);
      tick(0,0,0x4000);
      if (!g.save.cannonOpen('bob')) throw Error('Talking to the operator did not unlock the cannons');
      for (let i=0;i<150;i++) tick();
      g.teleport3(5376,1590,4610);
      m.setAction(0x0100088c);
      for (let i=0;i<200 && !(m.cannon && m.actionState===2);i++) tick();
      if (!m.cannon || m.actionState!==2) throw Error('Failed to enter cannon: '+[m.pos,m.action,m.floorHeight]);
      for (let i=0;i<9;i++) tick(0,-64);
      tick(0,0,0x8000);
      const shot = m.action;
      let flight=0;
      while (flight++ < 260 && m.action===0x00880898) tick();
      const result = {shot,landing:m.action,pos:[...m.pos],wing:m.wingCapTimer,flight};

      // Aim a second shot at the floating island tree. Follow Mario through the
      // fast grab, handstand and jump with controller samples after cannon entry.
      g.teleport3(5376,1590,4610);
      for (let i=0;i<200 && !(m.cannon && m.actionState===2);i++) tick();
      if (!m.cannon || m.actionState!==2) throw Error('Failed to reenter hillside cannon');
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
      const launch = {tree,launch:m.action,height:m.pos[1],speed:m.forwardVel};
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
      return {...result,...launch,revealTicks,saved:true};
    } finally {g.input.consumeStep=original}
  });
  console.log(result);
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
