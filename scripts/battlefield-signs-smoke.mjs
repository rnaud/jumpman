import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args: ['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  const page = await browser.newPage();
  await page.goto(new URL('?level=bob&act=1',process.env.URL || 'http://localhost:5173'), {waitUntil:'networkidle0'});
  await page.click('#start-button');
  const result = await page.evaluate(() => {
    const g=window.__JUMPMAN__, m=g.mario, original=g.input.consumeStep;
    const sign=g.playground.signs.find(s=>s.pos[0]===-4000 && s.pos[2]===6050);
    const tick=(x=0,y=0,button=0)=>{
      g.input.consumeStep=()=>({stickX:x,stickY:y,stickMag:Math.hypot(x,y),buttonDown:button,buttonPressed:button});
      g.stepSimulation();
      g.renderPos.fromArray(m.pos); g.renderYaw=m.faceAngle[1]; g.updateViewCamera(1/30);
    };
    try {
      for(const [x,z] of [[-4930,6000],[-4140,6050]]) {
        for (let i=0;i<300;i++) {
          const dx=x-m.pos[0], dz=z-m.pos[2], dist=Math.hypot(dx,dz);
          if (dist<12) break;
          const yaw=Math.atan2(dx,dz)-g.rig.yawS16*Math.PI/32768, mag=Math.min(64,dist*0.5);
          tick(mag*Math.sin(yaw),-mag*Math.cos(yaw));
        }
      }
      for(let i=0;i<25;i++)tick();
      if(g.nearbySign()!==sign)throw Error('Cannot read entrance sign '+JSON.stringify([m.pos,m.faceAngle,m.action]));
      tick(0,0,0x4000);
      const pages=new Set();
      for(let i=0;i<220 && g.sign;i++) {
        const box=document.querySelector('.sm64-sign-dialog');
        if(box && !box.hidden && sign.pages.includes(box.textContent))pages.add(box.textContent);
        tick(0,0,i%15===0?0x8000:0);
      }
      if(g.sign || document.querySelector('.sm64-sign-dialog'))throw Error('Sign failed to close');
      const aligned=Math.hypot(m.pos[0]-(sign.pos[0]-105),m.pos[2]-sign.pos[2]);
      const start=[...m.pos];
      for(let i=0;i<25;i++)tick(0,-64);
      return {signs:g.playground.signs.length, pages:[...pages], expected:sign.pages, aligned, moved:Math.hypot(m.pos[0]-start[0],m.pos[2]-start[2])};
    } finally {g.input.consumeStep=original;}
  });
  assert.equal(result.signs,13);
  assert.deepEqual(result.pages,result.expected);
  assert(result.aligned < 0.01 && result.moved>20);
  console.log({signs:result.signs,pages:result.pages.length,aligned:result.aligned,moved:result.moved});
} finally {await browser.close();}
