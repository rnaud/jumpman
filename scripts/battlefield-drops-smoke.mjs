import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath:process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args:['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  const page = await browser.newPage();
  await page.goto(new URL('?level=bob&act=3',process.env.URL || 'http://localhost:5173/').href,{waitUntil:'networkidle0'});
  await page.click('#start-button');
  const result = await page.evaluate(()=>{
    const g=window.__JUMPMAN__, m=g.mario, world=g.playground, manager=world.objects.manager;
    const original=g.input.consumeStep;
    const tick=(button=0)=>{
      g.input.consumeStep=()=>({stickX:0,stickY:0,stickMag:0,buttonPressed:button,buttonDown:button});
      g.stepSimulation();
    };
    const wait=(n)=>{ for(let i=0;i<n;i++) tick(); };
    const drops=()=>manager.objects.filter(o=>o.behavior==='movingYellowCoin');
    function grab(object) {
      g.teleport3(object.pos[0],object.pos[1],object.pos[2]-80,0);
      tick(0x4000); wait(25);
      if(m.heldObj!==object)throw Error('Grab failed for '+object.behavior);
    }
    try {
      const bomb=manager.objects.find(o=>o.behavior==='bobomb' && o.param===1);
      grab(bomb);
      tick(0x4000);
      for(let i=0;i<160 && bomb.active;i++)tick();
      if(bomb.active || drops().length!==1)throw Error('Missing Bob-omb coin');
      const coin=drops()[0], launch=[...coin.pos];
      wait(3);
      const rise=coin.pos[1]-launch[1];
      if(rise<=0 || !coin.active)throw Error('Coin did not arc');
      wait(40);
      const before=g.coinsCollected;
      g.teleport3(...coin.pos);
      tick();
      if(coin.active || g.coinsCollected!==before+1)throw Error('Coin did not count');
      g.teleport3(...bomb.home);
      wait(150);
      if(bomb.active)throw Error('Bob-omb respawned nearby '+JSON.stringify([m.pos,bomb.pos,bomb.home,launch,coin.pos,g.worldName]));
      g.teleport3(...world.spawn.pos);
      tick();
      if(!bomb.active)throw Error('Bob-omb failed to respawn');
      grab(bomb);
      tick(0x4000);
      for(let i=0;i<160 && bomb.active;i++)tick();
      if(bomb.active || drops().length!==1)throw Error('Duplicate Bob-omb reward');

      const box=manager.objects.find(o=>o.behavior==='breakableBoxSmall' && o.active);
      grab(box);
      // Face a vertical terrain wall with enough space to release the held box.
      const wall=world.collision.surfaces.find(s=>{
        if(Math.abs(s.normal[1])>0.001 || s.upperY-s.lowerY<500)return false;
        const x=(s.v1[0]+s.v2[0]+s.v3[0])/3+350*s.normal[0];
        const z=(s.v1[2]+s.v2[2]+s.v3[2])/3+350*s.normal[2];
        const floor=world.collision.findFloor(x,s.upperY-200,z);
        return floor.surface && floor.height>0 && floor.height>=s.lowerY && floor.height+300<s.upperY;
      });
      const x=(wall.v1[0]+wall.v2[0]+wall.v3[0])/3+350*wall.normal[0];
      const z=(wall.v1[2]+wall.v2[2]+wall.v3[2])/3+350*wall.normal[2];
      m.pos=[x,world.collision.findFloor(x,wall.upperY-200,z).height,z];
      m.faceAngle[1]=Math.atan2(-wall.normal[0],-wall.normal[2])*32768/Math.PI;
      tick();
      tick(0x4000);
      for(let i=0;i<80 && box.active;i++)tick();
      if(box.active || drops().length!==4)throw Error('Wall throw failed '+JSON.stringify([box.pos,m.pos,box.heldState]));
      wait(150);
      if(box.active)throw Error('Broken box respawned');
      return {rise,bobombDrops:1,boxDrops:drops().length-1};
    } finally {g.input.consumeStep=original;}
  });
  assert(result.rise>0 && result.boxDrops===3);
  console.log(result);
} finally {await browser.close();}
