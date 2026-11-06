import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath:process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args:['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  const page = await browser.newPage();
  await page.evaluateOnNewDocument(()=>{ let seed=64; Math.random=()=>((seed=(Math.imul(seed,1664525)+1013904223)>>>0)/0x100000000); });
  await page.goto(new URL('?level=bob&act=4',process.env.URL || 'http://localhost:5173/').href,{waitUntil:'networkidle0'});
  await page.click('#start-button');
  const run = await page.evaluate(()=>{
    const g=window.__JUMPMAN__, m=g.mario, world=g.playground, original=g.input.consumeStep;
    const tick=(x=0,y=0,button=0)=>{
      g.input.consumeStep=()=>({stickX:x,stickY:y,stickMag:Math.hypot(x,y),buttonDown:button,buttonPressed:button});
      g.stepSimulation();
    };
    const posts=world.objects.manager.objects.filter(o=>o.behavior==='woodenPost' && !o.link);
    const initial=world.coins.slice();
    try {
      for(const post of posts) {
        const [x,y,z]=post.pos;
        g.teleport3(x,y,z+180,0x4000);
        const count=world.coins.length;
        for(let i=0;i<800 && !post.vars.rewarded;i++){
          if(i%200===199){
            g.teleport3(x,y,z+410);
            tick();
            g.teleport3(x,y,z+150,0x4000);
          }
          const angle=Math.atan2(m.pos[0]-x,m.pos[2]-z)+0.35;
          const dx=x+150*Math.sin(angle)-m.pos[0], dz=z+150*Math.cos(angle)-m.pos[2];
          const stick=Math.atan2(dx,dz)-g.rig.yawS16*Math.PI/32768;
          tick(64*Math.sin(stick),-64*Math.cos(stick));
        }
        const loot=world.coins.slice(count).filter(c=>c.loot);
        if(loot.length!==5)throw Error('Missing post payout: '+JSON.stringify([x,z,post.vars,m.pos,loot.length]));
        const start=loot[0].position.y;
        tick();
        if(loot[0].position.y<=start || loot[0].loot.tangible)throw Error('Post coins should rise before pickup');
        for(let i=0;i<75;i++)tick();
        for(const c of loot){
          if(c.collected)continue;
          g.teleport3(...c.position.toArray());
          tick();
          if(!c.collected)throw Error('Coin pickup failed');
        }
      }
      const postCoins=g.coinsCollected;
      const crates=world.objects.manager.objects.filter(o=>o.behavior==='breakableBox');
      const payouts=[];
      for(const crate of crates){
        const before=world.coins.filter(c=>c.loot).length;
        g.teleport3(crate.pos[0],crate.pos[1],crate.pos[2]-80,0);
        tick(0,0,0x4000);
        for(let i=0;i<30;i++)tick();
        const payout=world.coins.filter(c=>c.loot).length-before;
        if(crate.active || payout!==(crate.param===1?3:0)) throw Error('Crate payout '+JSON.stringify([crate.param,payout,crate.active]));
        payouts.push(payout);
      }
      const goomba=world.goombas.find(o=>!o.spawner && !o.dead);
      const before=world.coins.filter(c=>c.loot).length;
      const foot=goomba.model.position;
      g.teleport3(foot.x,goomba.top+120,foot.z);
      m.setAction(0x0100088c);
      for(let i=0;i<35 && goomba.model.visible;i++)tick();
      if(goomba.model.visible || world.coins.filter(c=>c.loot).length!==before+1)throw Error('Missing Goomba stomp coin');
      // Use the exported placements to isolate coin accounting from route navigation.
      for(const coin of [...initial,...world.redCoins]){
        if(g.coinsCollected>=100)break;
        if(coin.collected)continue;
        for(let i=0;i<180 && !coin.collected;i++){
          g.teleport3(coin.position.x,coin.position.y-30,coin.position.z);
          m.setAction(0x0100088c);
          tick();
        }
        if(!coin.collected) throw Error('Missed placed coin '+JSON.stringify([coin.position,m.pos,g.coinsCollected,g.won,g.secretTimer,m.action,g.worldName]));
      }
      for(let i=0;i<160 && !g.coinStarFlight?.tangible;i++)tick();
      if(!g.coinStarFlight?.tangible)throw Error('No 100-coin star at '+g.coinsCollected);
      const star=world.hundredCoinStar.group.position;
      g.teleport3(star.x,star.y-30,star.z);
      tick();
      return {posts:posts.length,postCoins,payouts,goombaCoin:true,count:g.coinsCollected,saved:g.save.hasStar('bob',7),exited:g.won};
    } finally {g.input.consumeStep=original;}
  });
  console.log(run);
  assert.equal(run.posts,4);
  assert(run.postCoins>=20 && run.count>=100 && run.saved && !run.exited);
  await page.reload({waitUntil:'networkidle0'});
  assert(await page.evaluate(()=>window.__JUMPMAN__.save.hasStar('bob',7)));
} finally {await browser.close();}
