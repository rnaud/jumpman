import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args: ['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  const page = await browser.newPage();
  for (const act of [1,2]) {
    await page.goto(new URL('?level=bob&act='+act,process.env.URL || 'http://localhost:5173'),{waitUntil:'networkidle0'});
    await page.click('#start-button');
    const counts = await page.evaluate(() => {
      const g=window.__JUMPMAN__, m=g.mario, original=g.input.consumeStep;
      const buddies=g.playground.objects.manager.objects.filter(o=>o.behavior==='bobombBuddyAdvice').sort((a,b)=>a.param-b.param);
      const pages=[];
      try {
        for (const buddy of buddies) {
          const tick=(button,walk=false)=>{
            const dx=buddy.pos[0]-m.pos[0], dz=buddy.pos[2]-m.pos[2];
            const yaw=Math.atan2(dx,dz)-g.rig.yawS16*Math.PI/32768;
            const mag=walk ? 64 : 0;
            g.input.consumeStep=()=>({stickX:mag*Math.sin(yaw),stickY:-mag*Math.cos(yaw),stickMag:mag,buttonDown:button,buttonPressed:button});
            g.stepSimulation();
            g.renderPos.fromArray(m.pos); g.renderYaw=m.faceAngle[1]; g.updateViewCamera(1/30);
          };
          for(let i=0;i<240 && g.nearbyBuddy()!==buddy;i++)tick(0,true);
          if (g.operator || g.nearbyBuddy()!==buddy) throw Error('Cannot initiate advice '+buddy.param);
          tick(buddy.param===2?0x4000:0x8000);
          const seen=new Set();
          for(let i=0;i<120 && g.operator;i++) {
            if (g.operator.text) seen.add(g.operator.text);
            if (g.operator.opening) throw Error('Advice triggered the cannon cutscene');
            tick(g.operator.text?0x8000:0);
          }
          if (g.save.cannonOpen('bob')) throw Error('Advice unlocked the cannons');
          pages.push(seen.size);
          for(let i=0;i<20;i++)tick(0);
        }
      } finally {g.input.consumeStep=original;}
      return pages;
    });
    assert.deepEqual(counts,act===1?[5,4]:[3]);
    console.log({act,counts});
  }
} finally {await browser.close();}
