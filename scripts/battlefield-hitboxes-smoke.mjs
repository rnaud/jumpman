import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath:process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args:['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  const page=await browser.newPage();
  await page.goto(new URL('?level=bob',process.env.URL || 'http://localhost:5173/').href,{waitUntil:'networkidle0'});
  await page.click('#start-button');
  const result=await page.evaluate(()=>{
    const g=window.__JUMPMAN__, m=g.mario, world=g.playground, original=g.input.consumeStep;
    const box=world.objects.manager.objects.find(o=>o.behavior==='breakableBoxSmall');
    const enemy=world.goombas[0];
    const tick=(button=0,x=0,y=0,held=button)=>{
      g.input.consumeStep=()=>({stickX:x,stickY:y,stickMag:Math.hypot(x,y),buttonDown:held,buttonPressed:button});
      g.stepSimulation();
    };
    try {
      const [x,y,z]=box.pos;
      g.teleport3(x,y,z-120,0);
      tick();
      if(m.nearGrabbable)throw Error('Box prompt beyond grab range');
      g.teleport3(x,y,z-190,0);
      tick(0x4000);
      for(let i=0;i<20;i++)tick();
      if(m.heldObj || m.nearGrabbable)throw Error('Grab reached past the box');
      g.teleport3(x,y,z-85,0);
      tick(0x4000);
      for(let i=0;i<22;i++)tick();
      if(m.heldObj!==box)throw Error('Could not grab box up close');
      m.dropHeldObject();
      const p=enemy.model.position;
      g.teleport3(p.x+105,p.y,p.z);
      const health=m.health;
      tick();
      if(m.hurtCounter || m.health!==health)throw Error('Goomba damaged Mario outside its hurt radius');
      // Approach the enemy from its side and use the ordinary jump button to stomp.
      g.teleport3(p.x,p.y,p.z-240,0);
      for(let i=0;i<55 && enemy.tangible;i++) {
        const dx=p.x-m.pos[0], dz=p.z-m.pos[2], mag=Math.min(64,Math.hypot(dx,dz)*0.5);
        const yaw=Math.atan2(dx,dz)-g.rig.yawS16*Math.PI/32768;
        tick(i===0?0x8000:0,mag*Math.sin(yaw),-mag*Math.cos(yaw),0x8000);
      }
      if(enemy.tangible || !enemy.model.visible)throw Error('Jump failed to squash Goomba');
      const bounce=m.vel[1], scale=enemy.model.scale.clone(), before=world.coins.filter(c=>c.loot).length;
      for(let i=0;i<8;i++)tick();
      const height=enemy.model.scale.y/scale.y, width=enemy.model.scale.x/scale.x;
      if(!enemy.model.visible || world.coins.filter(c=>c.loot).length!==before)throw Error('Stomp ended early');
      for(let i=0;i<12;i++)tick();
      return {bounce,height,width,visible:enemy.model.visible,coins:world.coins.filter(c=>c.loot).length-before,boxRadius:box.hitbox.radius};
    } finally {g.input.consumeStep=original;}
  });
  console.log(result);
  assert(result.bounce===30 && Math.abs(result.height-0.3)<0.001 && Math.abs(result.width-1.7)<0.001);
  assert(!result.visible && result.coins===1 && result.boxRadius===60);
} finally {await browser.close();}
