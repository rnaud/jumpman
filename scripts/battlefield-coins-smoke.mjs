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
      if (g.save.bestCoins('bob') !== 0) throw Error('Coin pickups recorded a score before Mario collected the star');
      const star=world.hundredCoinStar.group.position;
      g.teleport3(star.x,star.y-30,star.z);
      tick();
      if (g.save.bestCoins('bob') !== g.coinsCollected) throw Error('100-coin star missed the run score');
      for (let i=0;i<180 && g.bonusSave.hidden;i++) tick();
      if (g.bonusSave.hidden || m.action !== 0x1307 || m.actionTimer !== 80) throw Error('Missing ground dance/save prompt');
      const disk = JSON.parse(localStorage.getItem('jumpman.save.v1.slot1') ?? '{}');
      if (disk.stars?.includes('bob:7')) throw Error('100-coin star saved before the player answered');
      tick(0,0,0x8000);
      for(let i=0;i<17;i++) tick();
      if (m.actionState !== 2 || !g.celebration) throw Error('Missing return dance');
      tick();
      if (g.celebration || m.action !== 0x0c400201 || g.worldName !== 'bob') throw Error('Did not regain control on the Battlefield');
      return {posts:posts.length,postCoins,payouts,goombaCoin:true,count:g.coinsCollected,saved:g.save.hasStar('bob',7),exited:g.won};
    } finally {g.input.consumeStep=original;}
  });
  console.log(run);
  assert.equal(run.posts,4);
  assert(run.postCoins>=20 && run.count>=100 && run.saved && !run.exited);
  await page.reload({waitUntil:'networkidle0'});
  assert(await page.evaluate(()=>window.__JUMPMAN__.save.hasStar('bob',7)));
  await page.click('#start-button');
  await page.click('#settings-button');
  assert.equal(await page.$eval('#course-score', el => el.innerText), 'MY SCORE · '+run.count+' COINS');
  await page.click('#save-slots button:nth-child(2)');
  assert(await page.$eval('#course-score', el => el.hidden));
  await page.goto(new URL('?level=jrb',page.url()).href,{waitUntil:'networkidle0'});
  await page.click('#start-button');
  const underwater = await page.evaluate(() => {
    // Cross the threshold with an actual submerged pickup; seed only the prior total.
    const g=window.__JUMPMAN__, world=g.playground, m=g.mario;
    const coin=world.coins.find(c => world.collision.waterLevel(c.position.x,c.position.z) > c.position.y+500);
    if (!coin) throw Error('No deep water coin for the swim scenario');
    g.coinsCollected=99;
    const {x,y,z}=coin.position;
    g.teleport3(x,y-30,z);
    m.setAction(0x380022c0);
    for(let i=0;i<180 && !g.coinStarFlight?.tangible;i++) g.stepSimulation();
    if(!g.coinStarFlight?.tangible) throw Error('Submerged coin did not spawn the star');
    const star=world.hundredCoinStar.group.position;
    g.teleport3(star.x,star.y-30,star.z);
    m.setAction(0x380022c0);
    for(let i=0;i<120 && g.bonusSave.hidden;i++) g.stepSimulation();
    if(m.action!==0x1303 || g.bonusSave.hidden) throw Error('Missing underwater dance');
    g.bonusSave.querySelector('.bonus-choices button:nth-child(2)').click();
    for(let i=0;i<23;i++) g.stepSimulation();
    if(m.actionState!==2 || !g.celebration) throw Error('Water recovery ended early');
    g.stepSimulation();
    return {action:m.action,retained:g.save.hasStar('jrb',7)};
  });
  assert.deepEqual(underwater,{action:0x380022c0,retained:true});
  await page.reload({waitUntil:'networkidle0'});
  assert.equal(await page.evaluate(()=>window.__JUMPMAN__.save.hasStar('jrb',7)),false);
} finally {await browser.close();}
