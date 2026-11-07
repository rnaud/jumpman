import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser=await puppeteer.launch({
  executablePath:process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args:['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  const page=await browser.newPage();
  await page.goto(new URL('?level=bob&act=3',process.env.URL || 'http://localhost:5173/').href,{waitUntil:'networkidle0'});
  await page.click('#start-button');
  const result=await page.evaluate(()=>{
    const g=window.__JUMPMAN__, world=g.playground, k=world.objects.manager.findByBehavior('koopa');
    const original=g.input.consumeStep;
    const tick=(button=0)=>{
      g.input.consumeStep=()=>({stickX:0,stickY:0,stickMag:0,buttonDown:button,buttonPressed:button});
      g.stepSimulation();
    };
    function punch() {
      const yaw=k.moveYaw*Math.PI/32768;
      g.teleport3(k.pos[0]+70*Math.sin(yaw),k.pos[1],k.pos[2]+70*Math.cos(yaw),k.moveYaw+0x8000);
      tick(0x4000);
      for(let i=0;i<8;i++)tick();
    }
    function stomp() {
      const yaw=k.moveYaw*Math.PI/32768;
      g.teleport3(k.pos[0]+k.forwardVel*2*Math.sin(yaw),k.pos[1]+30,k.pos[2]+k.forwardVel*2*Math.cos(yaw));
      g.mario.setAction(0x0100088c);
      g.mario.vel[1]=-12;
      for(let i=0;i<6;i++)tick();
    }
    try {
      punch();
      if (!k.active || !k.vars.unshelled || !k.link?.active)throw Error('Shell loss failed');
      const bareView=world.objects.views.get(k.id).bareKoopa;
      if(!bareView.visible)throw Error('Missing unshelled model');
      g.teleport3(...world.spawn.pos);
      for(let i=0;i<600 && k.vars.unshelled;i++)tick();
      if(k.vars.unshelled || k.link.active || bareView.visible)throw Error('Shell recovery failed '+JSON.stringify([k.pos,k.link.pos,k.action,k.vars]));
      for(let i=0;i<60;i++)tick();
      stomp();
      if (!k.vars.unshelled)throw Error('Second shell loss failed');
      for(let i=0;i<15;i++)tick();
      stomp();
      for(let i=0;i<80 && k.active;i++)tick();
      const blues=world.coins.filter(c=>c.value===5);
      if(k.active || blues.length!==1)throw Error('Missing single blue coin '+JSON.stringify([k.active,k.action,k.vars,blues.length,g.mario.pos,k.pos]));
      const blue=blues[0];
      for(let i=0;i<75 && !blue.loot.tangible;i++)tick();
      const before=g.coinsCollected;
      g.teleport3(...blue.position.toArray());
      tick();
      return {difference:g.coinsCollected-before,paid:blue.collected,shellStillPresent:k.link.active};
    } finally {g.input.consumeStep=original;}
  });
  assert.deepEqual(result,{difference:5,paid:true,shellStillPresent:true});
  console.log(result);
} finally {await browser.close();}
