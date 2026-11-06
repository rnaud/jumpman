import puppeteer from 'puppeteer-core';
import assert from 'node:assert/strict';

const browser = await puppeteer.launch({
  executablePath:process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args:['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  const page=await browser.newPage();
  await page.evaluateOnNewDocument(()=>{
    let seed=64;
    Math.random=()=>((seed=(Math.imul(seed,1664525)+1013904223)>>>0)/0x100000000);
  });
  await page.goto(new URL('?level=bob&act=3',process.env.URL || 'http://localhost:5173/').href,{waitUntil:'networkidle0'});
  await page.click('#start-button');
  const ride=await page.evaluate(()=>{
    const g=window.__JUMPMAN__, m=g.mario, koopa=g.playground.objects.manager.findByBehavior('koopa');
    const original=g.input.consumeStep;
    function drive(target,pressed=0,held=pressed) {
      const dx=target[0]-m.pos[0], dz=target[2]-m.pos[2];
      const yaw=Math.atan2(dx,dz)-g.rig.yawS16*Math.PI/32768;
      g.input.consumeStep=()=>({stickX:64*Math.sin(yaw),stickY:-64*Math.cos(yaw),stickMag:64,
        buttonDown:held,buttonPressed:pressed});
      g.stepSimulation();
      g.renderPos.fromArray(m.pos); g.renderYaw=m.faceAngle[1]; g.updateViewCamera(1/30);
      return Math.hypot(dx,dz);
    }
    try {
      for(const point of [[-4300,0,5380],[-2800,150,5480],[-1800,255,5630],[-900,256,5630],[930,768,5630],[1780,768,5000],[2000,768,4150],[2550,780,4150],[3450,850,4900],[3400,780,6200]]) {
        for(let i=0;i<400;i++){
          const bridgeJump=point[0]===930 && m.pos[0]>-650 && m.pos[0]<-570 && !(m.action&0x800);
          if(drive(point,(bridgeJump || (i%6===0 && m.forwardVel<5 && !(m.action&0x800)))?0x8000:0,0x8000)<65)break;
          if(i===399)throw Error('Approach stalled '+JSON.stringify([point,m.pos]));
        }
      }
      for(let i=0;i<350 && !koopa.vars.unshelled;i++) {
        const distance=Math.hypot(koopa.pos[0]-m.pos[0],koopa.pos[2]-m.pos[2]);
        drive(koopa.pos,distance<150 && i%6===0 ? 0x4000 : 0);
      }
      if(!koopa.link?.active)throw Error('Mario did not knock off the shell');
      const shell=koopa.link;
      for(let i=0;i<150 && !m.ridingShell;i++)drive(shell.pos);
      if(m.ridingShell!==shell)throw Error('Mario did not mount the dropped shell');
      const start=[...m.pos], target=[2000,768,4800];
      for(let i=0;i<22;i++)drive(target);
      const heading=m.faceAngle[1];
      drive(target,0x8000);
      if(m.action!==0x0281089a || m.ridingShell!==shell)throw Error('Jump detached rider');
      let peak=m.pos[1];
      for(let i=0;i<90 && m.action!==0x20810446;i++){
        drive(target,0,0x8000);
        peak=Math.max(peak,m.pos[1]);
      }
      if(!m.ridingShell || m.actionArg!==1)throw Error('Failed to land riding');
      const turn=[m.pos[0]+600,0,m.pos[2]-100];
      for(let i=0;i<10;i++)drive(turn);
      const steered=m.faceAngle[1]!==heading;
      const distance=Math.hypot(start[0]-m.pos[0],start[2]-m.pos[2]);
      drive(turn,0x2000);
      return {distance,steered,rise:peak-start[1],dismounted:!m.ridingShell && !shell.active};
    } finally {g.input.consumeStep=original;}
  });
  assert(ride.distance>500 && ride.steered && ride.rise>150 && ride.dismounted,JSON.stringify(ride));
  console.log(ride);
} finally {await browser.close();}
