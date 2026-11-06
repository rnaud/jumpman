import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser=await puppeteer.launch({
  executablePath:process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args:['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  const page=await browser.newPage();
  await page.goto(new URL('?level=bob&act=4',process.env.URL || 'http://localhost:5173/').href,{waitUntil:'networkidle0'});
  await page.click('#start-button');
  const result=await page.evaluate(()=>{
    const g=window.__JUMPMAN__, m=g.mario, world=g.playground, original=g.input.consumeStep;
    const lifts=world.platforms.filter(p=>p.kind==='checkerboard_lift');
    const coin=world.redCoins.find(c=>c.position.z===4440);
    const tick=(target,pressed=0,held=0)=>{
      const dx=target[0]-m.pos[0], dz=target[1]-m.pos[2];
      const mag=Math.min(64,Math.hypot(dx,dz)*0.7), yaw=Math.atan2(dx,dz)-g.rig.yawS16*Math.PI/32768;
      g.input.consumeStep=()=>({stickX:mag*Math.sin(yaw),stickY:-mag*Math.cos(yaw),stickMag:mag,buttonDown:held,buttonPressed:pressed});
      g.stepSimulation();
      g.renderPos.fromArray(m.pos); g.renderYaw=m.faceAngle[1]; g.updateViewCamera(1/30);
    };
    try {
      for(const target of [[-4100,4400],[-2900,3500],[-1000,4300],[1300,4510]]) {
        for(let i=0;i<350;i++) {
          if(Math.hypot(target[0]-m.pos[0],target[1]-m.pos[2])<65)break;
          tick(target);
          if(i===349)throw Error('Approach failed '+JSON.stringify([target,m.pos]));
        }
      }
      const low=lifts.find(l=>l.param>0);
      for(let i=0;i<240;i++){
        if(low.phase===4 && low.tick===15)break;
        tick([1300,low.blocker.position.z]);
      }
      let aboard=0;
      for(let i=0;i<220 && !coin.collected;i++) {
        const pos=low.blocker.position;
        const riding=low.surfaces.has(m.floor) && !(m.action&0x800);
        if(riding)aboard++;
        const jump=i===0 || (riding && pos.y>650 && Math.abs(pos.z-4440)<100);
        tick([1600,riding && pos.y>650?4440:pos.z],jump?0x8000:0,0x8000);
      }
      if(!coin.collected || aboard<5) throw Error('Lift ride failed '+JSON.stringify([aboard,m.pos,low.blocker.position,coin.collected]));
      return {aboard,redCoins:g.redCoinsCollected,paired:lifts.length};
    } finally {g.input.consumeStep=original;}
  });
  console.log(result);
  assert.equal(result.paired,2);
  assert(result.redCoins>=1);
} finally {await browser.close();}
