import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath:process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args:['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  const page=await browser.newPage();
  await page.goto(new URL('?level=wf',process.env.URL || 'http://localhost:5173/').href,{waitUntil:'networkidle0'});
  await page.click('#start-button');
  const tower=await page.evaluate(async()=>{
    const g=window.__JUMPMAN__;
    const parts=()=>g.playground.platforms.filter(p=>p.kind.startsWith('tower_'));
    if(parts().length)throw Error('Tower present during Whomp King mission');
    await g.loadCourse('wf',null,false,'push',2);
    const elevator=parts().find(p=>p.kind==='tower_elevator');
    const {x,y,z}=elevator.home;
    g.teleport3(x,y+104,z);
    g.stepSimulation();
    const floor=g.playground.collision.findFloor(x,y+202,z);
    g.teleport3(x,floor.height,z);
    const start=g.mario.pos[1];
    for(let i=0;i<142;i++)g.stepSimulation();
    const rise=g.mario.pos[1]-start;
    const {position:star}=g.playground.star;
    const summit=g.playground.collision.findFloor(star.x,star.y,star.z);
    if(!summit.surface)throw Error('Missing summit below mission star');
    g.teleport3(star.x,summit.height,star.z);
    const original=g.input.consumeStep;
    try {
      for(let t=0;t<45 && !g.won;t++){
        g.input.consumeStep=()=>({stickX:0,stickY:0,stickMag:0,buttonDown:0x8000,buttonPressed:t===0?0x8000:0});
        g.stepSimulation();
      }
    } finally {g.input.consumeStep=original}
    return {parts:parts().length,rise,summit:summit.height,star:star.toArray(),saved:g.save.hasStar('wf',2)};
  });
  console.log(tower);
  assert.equal(tower.parts,8);
  assert.equal(tower.rise,705);
  assert(tower.saved);
  await page.click('#castle-button');
  await page.waitForFunction(()=>window.__JUMPMAN__.worldName==='castle_inside');
  const blast=await page.evaluate(async()=>{
    const g=window.__JUMPMAN__;
    await g.loadCourse('wf',null,false,'push',6);
    const m=g.mario, original=g.input.consumeStep;
    const tick=(x=0,y=0,button=0)=>{
      g.input.consumeStep=()=>({stickX:x,stickY:y,stickMag:Math.hypot(x,y),buttonDown:button,buttonPressed:button});
      g.stepSimulation();
    };
    try {
      const objects=g.playground.objects.manager.objects;
      const right=objects.find(o=>o.behavior==='blastWall' && o.param===1);
      const buddy=objects.find(o=>o.behavior==='bobombBuddy' && o.param===1);
      g.teleport3(buddy.pos[0],buddy.pos[1],buddy.pos[2]-90,0);
      tick(0,0,0x4000);
      for(let i=0;i<260 && g.operator;i++) tick(0,0,g.operator.text ? 0x8000 : 0);
      if(!g.save.cannonOpen('wf'))throw Error('Cannon stayed closed');
      for(let i=0;i<150;i++)tick();
      g.teleport3(-1844,1090,3893);
      m.setAction(0x0100088c);
      for(let i=0;i<200 && !(m.cannon && m.actionState===2);i++)tick();
      if(!m.cannon || m.actionState!==2)throw Error('Cannon entry failed');
      for(let i=0;i<19;i++)tick(64,0);
      for(let i=0;i<3;i++)tick(0,64);
      tick(0,0,0x8000);
      for(let i=0;i<200 && m.action===0x00880898;i++)tick();
      const impact=[...m.pos];
      if(!right.active) {
        const star=g.playground.star.position;
        g.teleport3(star.x,star.y,star.z);
        tick();
      }
      return {broken:!right.active,saved:g.save.hasStar('wf',6),leftIntact:objects.some(o=>o.behavior==='blastWall' && !o.param && o.active),
        impact};
    }finally{g.input.consumeStep=original}
  });
  console.log(blast);
  assert(blast.broken && blast.leftIntact && blast.saved);
} finally {await browser.close()}
