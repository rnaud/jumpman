import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args: ['--no-sandbox', '--enable-unsafe-swiftshader', '--use-gl=angle', '--use-angle=swiftshader'],
});
try {
  const page = await browser.newPage();
  page.on('pageerror', error => console.error(error.message));
  await page.evaluateOnNewDocument(() => {
    let seed = 64;
    Math.random = () => ((seed = (Math.imul(seed,1664525)+1013904223) >>> 0) / 0x100000000);
  });
  await page.goto(new URL('?level=bob&act=6', process.env.URL || 'http://localhost:5173'), {waitUntil: 'domcontentloaded', timeout: 90000});
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'bob' && !document.querySelector('#start-button').disabled, {timeout: 90000});
  await page.click('#start-button');
  const result = await page.evaluate(() => {
    const g = window.__JUMPMAN__, m = g.mario, manager = g.playground.objects.manager;
    const chomp = manager.findByBehavior('chainChomp');
    const original = g.input.consumeStep;
    let target = null;
    const tick = (buttons = 0, stick = 0, held = buttons) => {
      let x = stick, y = 0;
      if (target) {
        const dx = target[0]-m.pos[0], dz = target[1]-m.pos[2];
        const yaw = Math.atan2(dx,dz) - g.rig.yawS16*Math.PI/32768;
        const magnitude = Math.min(64, Math.hypot(dx,dz)*0.75);
        x = magnitude*Math.sin(yaw);
        y = -magnitude*Math.cos(yaw);
      }
      g.input.consumeStep = () => ({stickX: x, stickY: y, stickMag: Math.hypot(x,y), buttonPressed: buttons, buttonDown: held | buttons});
      g.stepSimulation();
      g.renderPos.fromArray(m.pos);
      g.renderYaw = m.faceAngle[1];
      if (!g.chompShot) g.updateViewCamera(1/30);
    };
    try {
      const approach = (waypoint, radius = 60) => {
        target = waypoint;
        for (let i = 0; i < 400; i++) {
          if (Math.hypot(target[0]-m.pos[0],target[1]-m.pos[2]) < radius) return;
          const jump = !(m.action & 0x800) && (m.forwardVel < 5 && i%6 === 0 || waypoint[0] === 930 && i===0);
          tick(jump ? 0x8000 : 0, 0, 0x8000);
        }
        throw Error('Missed waypoint '+JSON.stringify([waypoint,m.pos]));
      };
      for (const point of [[-4300,5380],[-2800,5480],[-1800,5630],[-900,5630],[930,5630],
        [2000,4970],[2000,4320],[1240,3600],[-280,2720],[130,2220]]) approach(point);
      const post = chomp.links[0];
      const poundDepths = new Set();
      target = [post.pos[0],post.pos[2]];
      for (let i = 0; i < 650 && !chomp.vars.release; i++) {
        const distance = Math.hypot(target[0]-m.pos[0],target[1]-m.pos[2]);
        const air = (m.action & 0x800) !== 0;
        const pound = air && distance < 43 && m.pos[1] > post.pos[1]+220;
        tick(pound ? 0x2000 : !air && distance < 320 && i%3 === 0 ? 0x8000 : 0, 0, 0x8000);
        if ([-65,-130,-190].includes(post.vars.offsetY)) poundDepths.add(post.vars.offsetY);
      }
      target = null;
      if (!chomp.vars.release) throw Error('Post depth after three ground pounds: '+post.vars.offsetY+' action '+m.action.toString(16)+' pos '+JSON.stringify(m.pos));
      let lockedAt, tracked = 0, maxHeight = 0, returnTicks = 0, regained = false;
      const lunges = new Set();
      for (let i = 0; i < 600; i++) {
        tick(0, lockedAt ? 64 : 0);
        if (chomp.vars.release === 2 && chomp.vars.lunges >= 2) lunges.add(chomp.vars.lunges);
        if (chomp.vars.release === 4) maxHeight = Math.max(maxHeight, chomp.pos[1] - chomp.home[1]);
        if (chomp.vars.release === 5 && g.chompShot) returnTicks++;
        if (g.chompShot) {
          lockedAt ??= [...m.pos];
          if (m.pos.some((v,j) => Math.abs(v-lockedAt[j]) > 0.01)) throw Error('Mario moved during the shot '+JSON.stringify([lockedAt,m.pos,m.action.toString(16),m.actionState,chomp.vars.release,tracked]));
          const direction = g.chompShot.focus.clone().sub(g.camera.position).normalize();
          if (g.camera.getWorldDirection(direction.clone()).dot(direction) < 0.9999) throw Error('Camera lost shot focus');
          tracked++;
        } else if (lockedAt && !chomp.active && Math.hypot(m.pos[0]-lockedAt[0], m.pos[2]-lockedAt[2]) > 10) {
          regained = true;
          break;
        }
      }
      if (regained) {
        const {x,z} = g.playground.star.position;
        approach([1200,820]);
        target = [x,z];
        for (let i = 0; i < 200 && !g.save.hasStar('bob',6); i++) {
          tick(i%5 === 0 && !(m.action & 0x800) ? 0x8000 : 0, 0, 0x8000);
        }
      }
      const grabAction = m.action;
      target = null;
      for (let i=0; i<150 && m.action === 0x1904; i++) tick();
      if (m.action !== 0x1302 || g.won) throw Error('Expected a grounded star dance before the result');
      for (let i = m.actionTimer; i < 79; i++) tick();
      if (g.won || !g.celebration?.model.visible) throw Error('Celebration ended early');
      tick();
      const wipe = document.querySelector('#star-warp');
      if (g.won || wipe.hidden || m.actionTimer !== 80) throw Error('Missing automatic exit on dance frame 80');
      for (let i=0; i<24; i++) tick();
      const pixels = wipe.getContext('2d');
      if (pixels.getImageData(0,0,1,1).data[3] !== 255
          || pixels.getImageData(Math.floor(wipe.width/2),120,1,1).data[3] !== 0) throw Error('Missing Mario silhouette');
      for (let i=0; i<7; i++) tick();
      if (g.loadingCourse) throw Error('Warp started early');
      tick();
      if (!g.loadingCourse) throw Error('Missing warp after 32 frames');
      return {grabAction, poundDepths: [...poundDepths], saved:g.save.hasStar('bob',6), tracked, returnTicks, lunges: [...lunges], maxHeight, regained, gateBroken: !manager.findByBehavior('chainChompGate'),
        phase: chomp.vars.release, marioAction:m.action};
    } finally {g.input.consumeStep = original;}
  });
  await page.waitForFunction(() => window.__JUMPMAN__.worldName === 'castle_inside');
  assert.equal(new URL(page.url()).searchParams.get('entry'), 'WARP_NODE_32');
  await page.waitForSelector('#course-result button:not(:disabled)', {visible:true});
  // Select Continue, Don't Save with two stick taps, then A.
  await page.keyboard.press('s', {delay:100});
  await page.waitForSelector('#course-result button:nth-child(2)[aria-current="true"]');
  await page.waitForFunction(() => window.__JUMPMAN__.courseResult.lastDirection === 0);
  await page.keyboard.press('s', {delay:100});
  await page.waitForSelector('#course-result button:nth-child(3)[aria-current="true"]');
  await page.keyboard.press('Space');
  await page.waitForFunction(() => window.__JUMPMAN__.mario.action === 0x0c400201 && document.querySelector('#star-warp').hidden);
  assert(await page.evaluate(() => window.__JUMPMAN__.save.hasStar('bob',6)));
  await page.reload({waitUntil:'networkidle0'});
  assert.equal(await page.evaluate(() => window.__JUMPMAN__.save.hasStar('bob',6)), false);
  console.log(result);
  assert.deepEqual(result.poundDepths, [-65,-130,-190]);
  assert(result.tracked > 150 && result.returnTicks === 15 && result.lunges.length === 4);
  assert(result.maxHeight > 1200 && result.regained && result.gateBroken && result.saved);
} finally {await browser.close();}
