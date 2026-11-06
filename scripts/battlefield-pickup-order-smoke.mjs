import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath:process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args:['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  const page = await browser.newPage();
  await page.goto(new URL('?level=bob&act=4',process.env.URL || 'http://localhost:5173'),{waitUntil:'networkidle0'});
  await page.click('#start-button');
  const trace = await page.evaluate(() => {
    const g = window.__JUMPMAN__, m = g.mario, world = g.playground;
    // Put different real pickup types at the same position to isolate the interaction order.
    g.teleport3(-6000,768,2400);
    const yellow = world.coins.find(c=>!c.collected);
    const red = world.redCoins.find(c=>!c.collected);
    for (const coin of [yellow,red]) {
      coin.position.fromArray(m.pos);
      coin.group.position.copy(coin.position);
    }
    const drops = Array.from({length:2},()=>{
      const o = world.objects.manager.spawn({behavior:'movingYellowCoin',pos:m.pos,yaw:0,model:'actors/coin/yellow_coin_geo.glb'});
      o.timer=12; o.forwardVel=o.vel[1]=0;
      return o;
    });
    const trace = [];
    const tick = () => {
      g.stepSimulation();
      trace.push([g.coinsCollected,g.redCoinsCollected,m.healCounter,m.health,drops.filter(o=>o.active).length]);
    };
    m.health=0x400;
    m.setAction(0x20001306); // intangible NPC dialogue
    m.actionState=8;
    tick();
    m.setAction(0x0c400201); // idle, but still invincible after damage
    m.invincTimer=20;
    for (let i=0;i<4;i++) tick();
    return trace;
  });
  console.log(trace);
  assert.deepEqual(trace,[[0,0,0,1024,2],[1,0,3,1088,2],[3,1,10,1152,2],[4,1,13,1216,1],[5,1,16,1280,0]]);
} finally {await browser.close();}
