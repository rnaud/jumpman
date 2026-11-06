import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser=await puppeteer.launch({
  executablePath:process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args:['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  for (const character of ['sm64','link','young-link']) {
    const context=await browser.createBrowserContext();
    const page=await context.newPage();
    const errors=[];
    page.on('pageerror',e=>errors.push(e.message));
    await page.goto(new URL('?level=midos_house',process.env.URL || 'http://localhost:5173'));
    await page.waitForSelector('.character-picker:not(:disabled)');
    await page.click('.character-picker input[value="'+character+'"]');
    await page.waitForFunction(c=>window.__JUMPMAN__?.debugState.character===c &&
      !document.querySelector('#start-button').disabled,{},character);
    await page.click('#start-button');
    const count=await page.evaluate(async()=>{
      const g=window.__JUMPMAN__, v=g.villageTalk;
      const A=0x8000,B=0x4000;
      const tick=(pressed=0,y=0)=>{
        const original=g.input.consumeStep;
        g.input.consumeStep=()=>({stickX:0,stickY:y,stickMag:Math.abs(y),buttonDown:pressed,buttonPressed:pressed});
        try {g.stepSimulation();} finally {g.input.consumeStep=original;}
      };
      let total=0;
      for(const id of ['midos_house','deku_tree','zoras_domain','gerudos_fortress','haunted_wasteland',
        'desert_colossus','death_mountain_trail','goron_city']){
        if(g.debugState.world!==id) await g.loadCourse(id);
        for(const c of g.playground.kokiri.treasureChests){
          g.teleport3(c.pos[0]+145,c.pos[1],c.pos[2]);
          const root=g.courseScene.getObjectByName(id);
          const model=root.children.find(o=>o.position.distanceToSquared({x:c.pos[0],y:c.pos[1],z:c.pos[2]})<1 &&
            o.getObjectByName('joint_2'));
          if(!model) throw Error('Missing chest mesh '+c.key);
          const lid=model.getObjectByName('joint_2'), closed=lid.quaternion.clone();
          g.mario.health=0x580;
          const before=g.save.rupees;
          tick(A);
          // Advance the same render-time mixer that drives the lid in play.
          for(let i=1;i<=180;i++) g.playground.update(g.time+i/30);
          g.time+=6;
          if(closed.angleTo(lid.quaternion)<.5) throw Error('Lid did not open '+c.key);
          if(!g.save.chestOpened(c.key) || !v.pages.length) throw Error('Missing chest '+c.key);
          const value={76:1,77:5,85:50,86:200}[c.item] ?? 0;
          if(g.save.rupees!==Math.min(99,before+value)) throw Error('Chest wallet '+c.key);
          if(c.item===72 && g.mario.health!==0x680) throw Error('Chest heart '+c.key);
          v.textbox.finishReveal();tick(A);
          if(v.prompt(g.playground.kokiri,g.mario.pos,g.save)?.includes('CHEST')) throw Error('Chest replay '+c.key);
          total++;
        }
      }
      const pads=['graveyard','desert_colossus','lake_hylia','death_mountain_crater','sacred_forest_meadow'];
      await g.loadCourse(pads[0],'warp-pad');
      for(const to of pads.slice(1).concat(pads[0])){
        const pad=g.playground.kokiri.warpPad;
        if(Math.hypot(g.mario.pos[0]-pad.pos[0],g.mario.pos[2]-pad.pos[2])>4) throw Error('Wrong landing');
        tick(A);tick(B);
        if(v.warpMenu || g.loadingCourse) throw Error('Warp cancellation');
        tick(A);
        if(!v.warpMenu) throw Error('Missing warp prompt at '+g.debugState.world+' '+g.mario.pos);
        const index=v.warpMenu.destinations.findIndex(d=>d.id===to);
        const down=()=>{tick(0,-64);tick();};
        if(index>=2){down();down();tick(A);}
        if(index%2)down();
        tick(A);
        for(let i=0;g.loadingCourse && i<400;i++) await new Promise(r=>setTimeout(r,20));
        if(g.debugState.world!==to) throw Error('Failed warp to '+to);
      }
      return total;
    });
    assert.equal(count,19);
    await page.reload({waitUntil:'domcontentloaded'});
    await page.waitForFunction(()=>window.__JUMPMAN__?.playground?.kokiri);
    assert(await page.evaluate(()=>window.__JUMPMAN__.save.chestOpened('midos_house:0') &&
      window.__JUMPMAN__.save.treasureInventory.includes('Fairy Slingshot')));
    await page.evaluate(async()=>{
      const g=window.__JUMPMAN__;
      await g.loadCourse('midos_house');
      const c=g.playground.kokiri.treasureChests[0];
      const model=g.courseScene.getObjectByName('midos_house').children.find(o=>o.getObjectByName('joint_2') &&
        Math.hypot(o.position.x-c.pos[0],o.position.z-c.pos[2])<1);
      if(Math.abs(model.getObjectByName('joint_2').quaternion.w)<.75) throw Error('Restored lid is closed');
    });
    await page.click('#settings-button');
    await page.click('#settings [data-character="young-link"]');
    await page.waitForFunction(()=>window.__JUMPMAN__.debugState.character==='young-link');
    assert(await page.$eval('#zelda-treasure',el=>!el.hidden && el.textContent.includes('Compass')));
    assert.deepEqual(errors,[]);
    console.log(character+' opened 19 chests, visited all five warp pads, and restored the saved rewards.');
    await context.close();
  }
} finally {await browser.close();}
