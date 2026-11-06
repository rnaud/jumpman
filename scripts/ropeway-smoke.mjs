import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args: ['--no-sandbox', '--enable-unsafe-swiftshader'],
});
try {
  const page = await browser.newPage();
  await page.goto(new URL('?level=ccm&act=6', process.env.URL || 'http://localhost:5173/').href, {waitUntil:'networkidle0'});
  await page.click('#start-button');
  const ride = await page.evaluate(() => {
    const g = window.__JUMPMAN__;
    const lift = g.playground.platforms.find(p => p.kind === 'ropeway_lift');
    const pos = lift.blocker.position;
    const ticks = n => { for (let i = 0; i < n; i++) g.stepSimulation(); };
    ticks(90);
    const start = pos.toArray();
    const floor = g.playground.collision.findFloor(pos.x, pos.y, pos.z);
    if (!lift.surfaces.has(floor.surface)) throw Error('Cannot board lift at valley stop');
    g.teleport3(pos.x, floor.height, pos.z);
    ticks(20);
    const boarding = pos.toArray();
    let minX = pos.x, peak = -Infinity, passedBuddy = false;
    for (let i = 0; i < 1100; i++) {
      ticks(1);
      if (!lift.surfaces.has(g.mario.floor)) throw Error('Rider lost chair at tick ' + i);
      minX = Math.min(minX, g.mario.pos[0]);
      peak = Math.max(peak, g.mario.pos[1]);
      if (Math.abs(pos.x + 1222) < 5 && Math.abs(pos.y + 3569) < 5) passedBuddy = true;
      if (minX < -3000 && pos.x === start[0]) break;
    }
    const returned = pos.toArray();
    g.teleport3(...g.playground.spawn.pos);
    ticks(100);
    const empty = pos.toArray();
    g.teleport3(pos.x, floor.height, pos.z);
    for (let i = 0; i < 500 && pos.x > -1550; i++) ticks(1);
    if (pos.x > -1550) throw Error('Second departure failed');
    const original = g.input.consumeStep;
    const buddy = g.playground.objects.manager.objects.find(o => o.behavior === 'bobombBuddy');
    const drive = (button = 0) => {
      const dx = buddy.pos[0] - g.mario.pos[0], dz = buddy.pos[2] - g.mario.pos[2];
      const yaw = Math.atan2(dx, dz) - g.rig.yawS16 * Math.PI / 32768;
      g.input.consumeStep = () => ({stickX:64*Math.sin(yaw), stickY:-64*Math.cos(yaw), stickMag:64,
        buttonDown:button | 0x8000, buttonPressed:button});
      ticks(1);
    };
    try {
      for (let i = 0; i < 90 && !g.save.cannonOpen('ccm'); i++) {
        const distance = Math.hypot(g.mario.pos[0]-buddy.pos[0], g.mario.pos[2]-buddy.pos[2]);
        let button = 0;
        if (i === 7 || (i > 14 && !(g.mario.action & 0x800))) button = 0x8000; // Jump and hold A to clear the side of the lift.
        if (i === 26 || distance < 160) button = 0x4000; // Dive, then talk on the ledge.
        drive(button);
      }
    } finally { g.input.consumeStep = original; }
    return {opened:g.save.cannonOpen('ccm'), landing:g.mario.pos, start, boarding, minX, peak, passedBuddy, returned, empty};
  });
  console.log(ride);
  assert.deepEqual(ride.start, [571, -4450, 6426]);
  assert.deepEqual(ride.boarding, ride.start);
  assert(ride.passedBuddy && ride.minX < -3000 && ride.peak > -2400);
  assert.deepEqual(ride.returned, ride.start);
  assert.deepEqual(ride.empty, ride.start);
  assert(ride.opened);
} finally {
  await browser.close();
}
