import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser=await puppeteer.launch({executablePath:process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',args:['--no-sandbox','--enable-unsafe-swiftshader']});
try{
  const page=await browser.newPage();
  await page.goto(new URL('?level=ccm&act=2',process.env.URL || 'http://localhost:5173/').href,{waitUntil:'networkidle0'});
  await page.click('#start-button');
  const report=await page.evaluate(()=>{
    const g=window.__JUMPMAN__,m=g.mario,orig=g.input.consumeStep;
    const obs=g.playground.objects.manager.objects;
    const mother=obs.find(o=>o.behavior==='tuxiesMother');
    const reward=g.playground.sideStars.find(r=>r.penguin);
    const tick=(target,button=0)=>{
      const angle=target?Math.atan2(target[0]-m.pos[0],target[2]-m.pos[2])-g.rig.yawS16*Math.PI/32768:0;
      g.input.consumeStep=()=>({stickX:target?64*Math.sin(angle):0,stickY:target?-64*Math.cos(angle):0,stickMag:target?64:0,buttonDown:button || (m.action & 0x800 ? 0x8000 : 0),buttonPressed:button});
      g.stepSimulation();
    };
    function grab(id){
      const baby=obs.find(o=>o.behavior==='babyPenguin' && o.param===id);
      for(let attempt=0;attempt<4 && m.heldObj!==baby;attempt++){
        g.teleport3(baby.pos[0],baby.pos[1],baby.pos[2]-53,0);
        tick(null,0x4000);
        for(let t=0;t<12;t++)tick();
      }
      if(m.heldObj!==baby)throw Error('Pickup failed for '+id+' '+[m.action,m.pos,baby.pos]);
    }
    try{
      if(reward.group.visible)throw Error('Reward visible before delivery');
      grab(1);
      for(let t=0;t<350 && m.heldObj;t++)tick(mother.pos);
      for(let t=0;t<120;t++)tick();
      if(m.heldObj || reward.group.visible || !obs.find(o=>o.behavior==='babyPenguin' && o.param===1).vars.rejected)throw Error('Decoy delivery failed');
      grab(0);
      const respawns=m.respawns;
      for(let t=0;t<1600 && m.heldObj;t++)tick(mother.pos);
      for(let t=0;t<130;t++)tick();
      if(!reward.group.visible)throw Error('True delivery failed '+JSON.stringify({m:m.pos,action:m.action,mother:mother.pos,baby:obs.find(o=>o.behavior==='babyPenguin' && o.param===0).pos,held:!!m.heldObj}));
      const star=reward.group.position.toArray();
      for(let t=0;t<450 && !g.save.hasStar('ccm',2);t++){
        const jump=Math.hypot(m.pos[0]-star[0],m.pos[2]-star[2])<140 && !(m.action&0x800);
        tick(star,jump?0x8000:0);
      }
      return {saved:g.save.hasStar('ccm',2),star,respawns:m.respawns-respawns};
    }finally{g.input.consumeStep=orig}
  });
  console.log(report);
  assert(report.saved && report.respawns===0);
  assert.deepEqual(report.star,[3167,-4300,5108]);
  await page.waitForFunction(()=>window.__JUMPMAN__.worldName==='castle_inside');
}finally{await browser.close()}
