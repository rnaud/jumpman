import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args:['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  const page = await browser.newPage();
  await page.goto(new URL('?level=bob&act=3',process.env.URL || 'http://localhost:5173/').href,{waitUntil:'networkidle0'});
  await page.click('#start-button');
  const visits=await page.evaluate(()=>{
    const g=window.__JUMPMAN__, m=g.mario, world=g.playground;
    const coin=world.coins[0].position;
    g.teleport3(coin.x,coin.y-20,coin.z); m.setAction(0x0100088c); g.stepSimulation();
    const collected=g.coinsCollected;
    if(collected<1)throw Error('Failed to collect setup coin');
    const pads=world.fadingWarps;
    for(const pad of pads) {
      // Leave the trigger before testing each direction independently.
      g.teleport3(...world.spawn.pos); g.stepSimulation();
      const [x,y,z]=pad.pos;
      g.teleport3(x,y+25,z); m.setAction(0x0100088c); g.stepSimulation();
      if(m.action===0x1336)throw Error('Airborne Mario triggered a warp');
      g.teleport3(x,y,z);
      g.stepSimulation();
      if(m.action!==0x1336)throw Error('No fade-out '+pad.pos);
      for(let i=0;i<38;i++)g.stepSimulation();
      if(m.pos[0]!==x || +document.querySelector('#warp-fade').style.opacity<0.8)throw Error('Departure timing');
      for(let i=0;i<3;i++)g.stepSimulation();
      if(m.action!==0x1337 || m.pos[0]!==pad.target[0] || m.pos[2]!==pad.target[2] || m.faceAngle[1]!==pad.faceYaw)
        throw Error('Wrong arrival '+JSON.stringify([m.action,m.pos,pad]));
      for(let i=0;i<80;i++)g.stepSimulation();
      if(m.action!==0x0c400201 || m.pos[0]!==pad.target[0] || +document.querySelector('#warp-fade').style.opacity!==0)
        throw Error('Immediate return warp or stuck fade');
      if(g.playground!==world || g.coinsCollected!==collected || g.act!==3)throw Error('Lost course progress');
    }
    return pads.length;
  });
  assert.equal(visits,4);
  console.log('All four same-area warp directions passed.');
} finally {await browser.close();}
