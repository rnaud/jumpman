import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args: ['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  for (const character of ['sm64','link']) {
    const context = await browser.createBrowserContext();
    const page = await context.newPage();
    const errors=[];
    page.on('pageerror',e=>errors.push(e.message));
    await page.goto(new URL('?level=kokiri_forest&character='+character,process.env.URL || 'http://localhost:5173'));
    await page.waitForFunction(c=>window.__JUMPMAN__?.debugState.character===c,{},character);
    await page.click('#start-button');
    const result = await page.evaluate(async () => {
      const g=window.__JUMPMAN__;
      // Visit the actual ROM pickups; include a second visit for respawning greens.
      for (let visit=0;g.save.rupees<50 && visit<6;visit++) {
        if (visit) await g.loadCourse('kokiri_forest');
        for (const p of g.playground.kokiri.rupees) {
          g.teleport3(...p.pos);
          g.stepSimulation();
        }
      }
      const funds=g.save.rupees;
      await g.loadCourse('kokiri_shop','door');
      const tick=buttons=>{
        const original=g.input.consumeStep;
        g.input.consumeStep=()=>({stickX:0,stickY:0,stickMag:0,buttonDown:buttons,buttonPressed:buttons});
        try { g.stepSimulation(); } finally {g.input.consumeStep=original;}
      };
      const A=0x8000,B=0x4000;
      const text=()=>document.querySelector('#kokiri-dialogue p').textContent;
      g.teleport3(...g.playground.kokiri.counter.shield);
      tick(A); const offered=text(); tick(B);
      const cancelled=g.save.rupees===funds && !g.save.dekuShield;
      tick(A);tick(A);const shield=text();tick(A);
      tick(A);tick(A);const duplicate=text();tick(A);
      g.teleport3(...g.playground.kokiri.counter.heart);
      tick(A);tick(A);const full=text();tick(A);
      g.mario.health=0x680;
      tick(A);tick(A);const heart=text();tick(A);
      return {funds,cancelled,offered,shield,duplicate,full,heart,balance:g.save.rupees,health:g.mario.health};
    });
    assert(result.funds >= 50,JSON.stringify(result));
    assert(result.cancelled);
    assert.match(result.offered,/40 rupees/);
    assert.match(result.shield,/got a Deku Shield/);
    assert.match(result.duplicate,/already own/);
    assert.match(result.full,/full/);
    assert.match(result.heart,/Recovered/);
    assert.equal(result.health,0x780);
    assert.equal(result.balance,result.funds-50);
    await page.reload();
    await page.waitForFunction(()=>window.__JUMPMAN__?.save.dekuShield);
    assert.equal(await page.evaluate(()=>window.__JUMPMAN__.save.rupees),result.balance);
    assert.deepEqual(errors,[]);
    console.log(character+' collected rupees, cancelled and completed purchases, and reloaded the saved wallet.');
    await context.close();
  }
} finally {await browser.close();}
