import { climbBattlefieldSummit } from './battlefield-climb.mjs';
import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath:process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args:['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  const page=await browser.newPage();
  // Repeat the same enemy and water-bomb timing on every controller replay.
  await page.evaluateOnNewDocument(()=>{
    let seed=64;
    Math.random=()=>((seed=(Math.imul(seed,1664525)+1013904223)>>>0)/0x100000000);
  });
  await page.goto(new URL('?level=bob&act=1',process.env.URL || 'http://localhost:5173/').href,{waitUntil:'networkidle0'});
  await page.click('#start-button');
  await page.evaluate(climbBattlefieldSummit);
  const result=await page.evaluate(()=>{
    const g=window.__JUMPMAN__, m=g.mario;
    const manager=g.playground.objects.manager, king=manager.findByBehavior('kingBobomb');
    const original=g.input.consumeStep;
    const tick=(button=0,x=0,y=0,held=button)=>{
      g.input.consumeStep=()=>({stickX:x,stickY:y,stickMag:Math.hypot(x,y),buttonDown:held,buttonPressed:button});
      g.stepSimulation();
      g.renderPos.fromArray(m.pos); g.renderYaw=m.faceAngle[1]; g.updateViewCamera(1/30);
    };
    try {
      const pages=new Set();
      const health=[];
      let carried=0;
      const aim=(x,z,button=0,max=64)=>{
        const dx=x-m.pos[0], dz=z-m.pos[2], yaw=Math.atan2(dx,dz)-g.rig.yawS16*Math.PI/32768;
        const mag=Math.min(max,Math.hypot(dx,dz)*0.8);
        tick(button,mag*Math.sin(yaw),-mag*Math.cos(yaw));
      };
      for(let i=0;i<3600 && !g.missionComplete;i++) {
        if(g.kingDialog || m.action===0x20001306) {
          const reading=g.kingDialog?.page>=0;
          if(reading) pages.add(g.kingDialog.id+':'+g.kingDialog.page);
          tick(reading && i%4===0 ? 0x4000 : 0);
        } else if(m.heldObj===king){
          carried++;
          // Carry him back toward the center so the throw lands on the summit.
          const close=Math.hypot(m.pos[0]-3000,m.pos[2]+4600)<340;
          aim(3000,-4600,close && i%4===0 ? 0x4000 : 0);
        } else if(king.active && !king.vars.dialog){
          if(king.pos[0]<2150 || king.pos[2]<-5300) {aim(2700,-4550);continue;}
          const angle=Math.atan2(m.pos[0]-king.pos[0],m.pos[2]-king.pos[2]);
          const facing=king.moveYaw*Math.PI/32768;
          const behind=Math.cos(angle-facing)<-0.4;
          if(behind) aim(king.pos[0],king.pos[2],king.distanceToMario<136 && m.forwardVel<27 && i%3===0 ? 0x4000 : 0,28);
          else {
            const orbit=angle+(Math.sin(angle-facing)>=0?1:-1)*0.55;
            aim(king.pos[0]+270*Math.sin(orbit),king.pos[2]+270*Math.cos(orbit));
          }
        } else tick();
        if(health.at(-1)!==king.vars.health) health.push(king.vars.health);
      }
      if(!g.missionComplete) throw Error('Summit fight stalled '+JSON.stringify([m.pos,m.action.toString(16),king.pos,king.vars,health,carried,[...pages]]));
      const star=g.playground.star.position;
      for(let i=0;i<300 && !g.save.hasStar('bob',1);i++) aim(star.x,star.z,!(m.action&0x800) && i%4===0 ? 0x8000:0);
      return {pages:[...pages],health,carried,target:star.toArray(),saved:g.save.hasStar('bob',1),respawns:m.respawns};
    } finally {g.input.consumeStep=original;}
  });
  console.log(result);
  assert.deepEqual(result.health,[3,2,1,0]);
  assert.equal(result.pages.length,10);
  assert.deepEqual(result.target,[2000,4500,-4500]);
  assert(result.saved && result.respawns===0);
  await page.waitForSelector('#course-result button:not(:disabled)',{visible:true,timeout:90000});
  await page.keyboard.press('Space');
  await page.waitForFunction(()=>document.querySelector('#course-result').hidden);
  await page.reload({waitUntil:'domcontentloaded',timeout:90000});
  await page.waitForFunction(()=>window.__JUMPMAN__?.save.hasStar('bob',1),{timeout:90000});
}finally{await browser.close();}
