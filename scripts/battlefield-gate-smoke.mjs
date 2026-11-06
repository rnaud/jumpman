import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args: ['--no-sandbox', '--enable-unsafe-swiftshader'],
});
try {
  const page = await browser.newPage();
  await page.goto(new URL('?level=bob&act=6', process.env.URL || 'http://localhost:5173'));
  await page.click('#start-button');
  const result = await page.evaluate(() => {
    const g = window.__JUMPMAN__, m = g.mario, manager = g.playground.objects.manager;
    const chomp = manager.findByBehavior('chainChomp');
    const original = g.input.consumeStep;
    const tick = (buttons = 0, stick = 0) => {
      g.input.consumeStep = () => ({stickX: stick, stickY: 0, stickMag: stick, buttonPressed: buttons, buttonDown: buttons});
      g.stepSimulation();
    };
    try {
      g.teleport3(chomp.home[0]+900, chomp.home[1]+50, chomp.home[2]);
      tick();
      const post = chomp.links[0];
      for (let j = 0; j < 180 && !chomp.vars.restricted; j++) tick();
      for (let i = 0; i < 3; i++) {
        g.teleport3(post.pos[0], post.pos[1]+420, post.pos[2]);
        tick();
        tick(0x2000);
        for (let j = 0; j < 55 && !chomp.vars.release; j++) tick();
      }
      if (!chomp.vars.release) throw Error('Post depth after three ground pounds: '+post.vars.offsetY+' action '+m.action.toString(16));
      let lockedAt, tracked = 0, maxHeight = 0, returnTicks = 0, regained = false;
      const lunges = new Set();
      for (let i = 0; i < 600; i++) {
        tick(0, lockedAt ? 64 : 0);
        if (chomp.vars.release === 2 && chomp.vars.lunges >= 2) lunges.add(chomp.vars.lunges);
        if (chomp.vars.release === 4) maxHeight = Math.max(maxHeight, chomp.pos[1] - chomp.home[1]);
        if (chomp.vars.release === 5 && g.chompShot) returnTicks++;
        if (g.chompShot) {
          lockedAt ??= [...m.pos];
          if (m.pos.some((v,j) => Math.abs(v-lockedAt[j]) > 0.01)) throw Error('Mario moved during the shot');
          const direction = g.chompShot.focus.clone().sub(g.camera.position).normalize();
          if (g.camera.getWorldDirection(direction.clone()).dot(direction) < 0.9999) throw Error('Camera lost shot focus');
          tracked++;
        } else if (lockedAt && !chomp.active && Math.hypot(m.pos[0]-lockedAt[0], m.pos[2]-lockedAt[2]) > 10) {
          regained = true;
          break;
        }
      }
      if (regained) {
        const {x,y,z} = g.playground.star.position;
        g.teleport3(x, y-30, z);
        for (let i = 0; i < 120 && !g.save.hasStar('bob',6); i++) tick();
      }
      return {saved:g.save.hasStar('bob',6), tracked, returnTicks, lunges: [...lunges], maxHeight, regained, gateBroken: !manager.findByBehavior('chainChompGate'),
        phase: chomp.vars.release, marioAction:m.action};
    } finally {g.input.consumeStep = original;}
  });
  console.log(result);
  assert(result.tracked > 150 && result.returnTicks === 15 && result.lunges.length === 4);
  assert(result.maxHeight > 1200 && result.regained && result.gateBroken && result.saved);
} finally {await browser.close();}
