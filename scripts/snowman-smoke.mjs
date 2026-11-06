import puppeteer from 'puppeteer-core';
import assert from 'node:assert/strict';
const browser=await puppeteer.launch({executablePath:process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',args:['--no-sandbox','--enable-unsafe-swiftshader']});
try{
  const page=await browser.newPage();
  await page.goto(new URL('?level=ccm&act=5',process.env.URL || 'http://localhost:5173/').href,{waitUntil:'networkidle0'});
  await page.click('#start-button');
  const result=await page.evaluate(()=>{
    const g=window.__JUMPMAN__,m=g.mario,manager=g.playground.objects.manager;
    const body=manager.findByBehavior('snowmanBody'), head=manager.findByBehavior('snowmanHead');
    const original=g.input.consumeStep;
    const tick=(target,button=0)=>{
      const angle=target?Math.atan2(target[0]-m.pos[0],target[2]-m.pos[2])-g.rig.yawS16*Math.PI/32768:0;
      g.input.consumeStep=()=>({stickX:target?64*Math.sin(angle):0,stickY:target?-64*Math.cos(angle):0,stickMag:target?64:0,buttonPressed:button,buttonDown:button || (target && m.action&0x800 ? 0x8000 : 0)});
      g.stepSimulation();
    };
    g.teleport3(body.home[0]+280,body.home[1],body.home[2]);
    tick();

    const respawns=m.respawns;
    // Cross the upper snowfield, drop from its north edge to the checkpoint road,
    // then cut across the lower bend before the body reaches the junction.
    const route=[[-402,0,-2898],[-402,0,-3400],[-402,0,-3500],[-402,461,-2898],[-2800,-1200,-3500],[-4400,-1330,-2100],[-4800,-1340,-400],[-4130,-1344,2150]];
    let index=0;
    for(let t=0;t<1600 && head.action!==4;t++){
      if(index<2 && Math.hypot(m.pos[0]-route[index][0],m.pos[2]-route[index][2])<140)index++;
      if(index===2 && m.pos[1]<2000)index=3;
      if(body.vars.checked && index<4 && m.pos[1]<-1000)index=4;
      if(index>=4 && index<7 && Math.hypot(m.pos[0]-route[index][0],m.pos[2]-route[index][2])<400)index++;
      const target=body.vars.checked && index<4 ? [-450,0,-3900] : route[index];
      const pound=Boolean(m.action&0x800) && (!body.vars.checked && m.pos[2]<-3120 || body.vars.checked && m.pos[2]<-3500 && m.pos[1]>-1000);
      tick(Math.hypot(m.pos[0]-target[0],m.pos[2]-target[2])>50?target:null,pound?0x2000:0);
    }
    if(head.action!==4)throw Error('Failed to assemble at waypoint '+body.vars.point);
    // Isolate the source facing/height check from the guided downhill run.
    g.teleport3(-4500,-1344,1813,0x4000);
    for(let t=0;t<140;t++)tick();
    const reward=g.playground.sideStars.find(s=>s.snowman);
    if(!reward.group.visible)throw Error('Missing reward after thanking Mario '+[m.pos,m.faceAngle,head.pos,head.action,head.faceYaw,head.moveYaw,head.angleToMario,m.action]);
    const star=reward.group.position.toArray();
    for(let t=0;t<400 && !g.save.hasStar('ccm',5);t++){
      const jump=Math.hypot(star[0]-m.pos[0],star[2]-m.pos[2])<160 && !(m.action&0x800);
      tick(star,jump?0x8000:0);
    }
    g.input.consumeStep=original;
    return {saved:g.save.hasStar('ccm',5),steps:body.vars.point,respawns:m.respawns-respawns,scale:body.scale,headY:head.pos[1]};
  });
  console.log(result);assert(result.saved && result.steps===29 && result.respawns===0 && result.headY===-994);
await page.waitForFunction(()=>window.__JUMPMAN__.worldName==='castle_inside');
  const revisit=await page.evaluate(async()=>{
    const g=window.__JUMPMAN__;
    await g.loadCourse('ccm',null,false,'none',6);
    const manager=g.playground.objects.manager;
    g.stepSimulation();
    const body=manager.findByBehavior('snowmanBody'),head=manager.findByBehavior('snowmanHead');
    return {whole:body.visible && body.param===-1 && head.pos[1]===-994, saved:g.save.hasStar('ccm',5)};
  });
  assert.deepEqual(revisit,{whole:true,saved:true});
  const miss=await page.evaluate(async()=>{
    const g=window.__JUMPMAN__;
    await g.loadCourse('ccm',null,false,'none',5);
    const manager=g.playground.objects.manager,body=manager.findByBehavior('snowmanBody');
    g.teleport3(body.pos[0]+280,body.pos[1],body.pos[2]);g.stepSimulation();
    g.teleport3(-4500,-1344,1813,0x4000);
    let missed=false;
    for(let t=0;t<1200;t++){
      g.stepSimulation();
      if(body.action===5)missed=true;
      if(missed && body.action===0)break;
    }
    return {missed,rearmed:body.action===0 && body.visible,star:g.playground.sideStars.find(s=>s.snowman).group.visible};
  });
  assert.deepEqual(miss,{missed:true,rearmed:true,star:false});
}finally{await browser.close()}
