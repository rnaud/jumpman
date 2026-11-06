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
      return {shot,landing:m.action,pos:m.pos,wing:m.wingCapTimer,flight};
    } finally {g.input.consumeStep=original}
  });
  console.log(result);
  assert.equal(result.shot,0x00880898);
  assert.equal(result.landing,0x00880456);
  assert.equal(result.pos[1],3072);
  assert(Math.hypot(result.pos[0]-5540,result.pos[2]-1200) < 300, 'did not land near the island star box');
  assert.equal(result.wing,0);
  assert.deepEqual(errors,[]);
} finally {await browser.close()}
