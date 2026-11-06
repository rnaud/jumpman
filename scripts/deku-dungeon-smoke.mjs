import puppeteer from 'puppeteer-core';

const browser=await puppeteer.launch({
  executablePath:process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args:['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  for(const character of ['sm64','link','young-link']) {
    const context=await browser.createBrowserContext();
    const page=await context.newPage();
    page.on('pageerror',e=>console.error(e.message));
    await page.goto(new URL('?level=deku_tree&character='+character,process.env.URL || 'http://localhost:5173'));
    await page.waitForFunction(c=>window.__JUMPMAN__?.debugState.character===c && !document.querySelector('#start-button').disabled,{},character);
    await page.click('#start-button');
    await page.evaluate(()=>{
      const g=window.__JUMPMAN__, m=g.mario, d=g.playground.dekuDungeon, save=g.save;
      const A=0x8000,Z=0x2000, base=d.parts.get('web').home;
      const location=()=>[m.pos[0]-base.x,m.pos[1],m.pos[2]-base.z].map(x=>Math.round(x/4.4)).join(',');
      const fail=s=>{throw Error(s+' at '+location()+' ('+d.prompt+')');};
      const tick=(pressed=0,x=0,y=0,down=pressed)=>{
        g.input.consumeStep=()=>({stickX:x,stickY:y,stickMag:Math.hypot(x,y),buttonDown:down,buttonPressed:pressed});
        g.stepSimulation();
      };
      const idle=n=>{for(let i=0;i<n;i++)tick();};
      const walk=(x,z,r=45)=>{
        for(let i=0;i<600;i++){
          const dx=base.x+x*4.4-m.pos[0], dz=base.z+z*4.4-m.pos[2], distance=Math.hypot(dx,dz);
          if(distance<r)return;
          const angle=Math.atan2(dx,dz)-g.rig.yawS16*Math.PI/32768;
          const mag=Math.min(64,Math.max(24,distance/2));
          const swim=!!(m.action&0x2000);
          tick(swim && i%15===0?A:0,mag*Math.sin(angle),-mag*Math.cos(angle),swim?A:0);
        }
        fail('Walk blocked toward '+[x,z]);
      };
      const action=(label,ticks)=>{
        tick();
        if(!d.prompt?.includes(label))fail('Missing '+label);
        tick(A);idle(ticks);
      };
      const shoot=role=>{
        const target=d.targets(save,()=>{}).find(t=>role==='ladder'?t.radius===185:t.radius===80);
        const dx=target.center.x-m.pos[0],dy=target.center.y-m.pos[1]-130,dz=target.center.z-m.pos[2];
        g.input.keys.add('KeyH');tick();
        const sling=g.playground.slingshot;
        if(!sling.aiming)fail('Cannot aim at '+role);
        const yaw=Math.atan2(dx,dz),pitch=Math.atan2(dy,Math.hypot(dx,dz));
        for(let i=0;i<120;i++){
          const delta=Math.atan2(Math.sin(yaw-sling.yaw),Math.cos(yaw-sling.yaw));
          tick(0,Math.max(-64,Math.min(64,-delta/.0007)),Math.max(-64,Math.min(64,(pitch-sling.pitch)/.00055)));
        }
        tick(A);g.input.keys.delete('KeyH');idle(90);
        if(!save.dekuCleared(role))fail('Seed missed '+role);
      };
      idle(8);walk(0,400);walk(-120,280);walk(-228,197);
      action('LOBBY LADDER',55);walk(-284,259);action('UPPER WALKWAY',65);
      walk(-432,432);action('DOOR',18);walk(-695,692);walk(-910,910);action('DOOR',18);walk(-1020,1020);
      action('JUMP TO THE PLATFORM',27);action('CHEST LEDGE',30);
      walk(-1391,1391,155);tick(A);
      if(!save.hasSlingshot)fail('Chest failed');
      g.villageTalk.textbox.finishReveal();tick(A);
      g.syncCharacterButtons();document.querySelector('#equip-slingshot').click();
      idle(6);shoot('ladder');
      walk(-1250,1250);idle(60);walk(-1068,1068);action('FALLEN LADDER',45);
      walk(-962,962);action('DOOR',18);walk(-695,692);walk(-480,480);action('DOOR',18);walk(-340,311);
      action('LEAP TOWARD THE WEB',240);
      if(!save.dekuCleared('web'))fail('Web survived');
      action('SWIM TO THE BANK',70);walk(-75,545);action('DOOR',18);walk(-75,770);idle(4);
      for(let i=0;i<190 && !save.dekuCleared('scrub');i++)tick(0,0,0,Z);
      if(!save.dekuCleared('scrub'))fail('Scrub failed to yield');
      walk(-95,930);idle(12);shoot('eye');walk(-300,960);action('WATER ROOM',18);walk(-560,990);
      action('SWIM TO THE SWITCH',80);action('WATER SWITCH',30);
      if(!d.waterTimer)fail('Water switch failed');
      action('SWIM TO THE PLATFORM',50);
      for(let i=0;i<400 && !d.prompt?.includes('JUMP TO THE FAR BANK');i++)tick();
      if(!d.ridden)fail('Ferry crossing failed');
      action('JUMP TO THE FAR BANK',40);
      if(!save.dekuCleared('crossed'))fail('Far bank failed');
      walk(-1310,1160);action('PUSH THE BLOCK',42);
      walk(-1380,1110,235);action('CLIMB THE PUSH BLOCK',30);
      walk(-1500,1070);action('DOOR',18);walk(-1855,810);action('DOOR',18);
      walk(-1855,300);walk(-1500,100,100);walk(-1370,3);action('CRAWL',60);
      walk(-800,80);walk(-750,245);walk(-490,410);action('LIGHT A DEKU STICK',425);
      if(d.flame.visible || save.dekuCleared('burned'))fail('Expired stick still burns');
      action('LIGHT A DEKU STICK',3);
      walk(-650,270);walk(-635,0);idle(210);
      if(!save.dekuCleared('burned'))fail('Web did not burn');
      action('SWIM TO THE SCRUBS',80);
      walk(-330,-810);
      for(let i=0;i<155;i++)tick(0,0,0,Z);
      if(d.brothersSolved || save.dekuCleared('brothers'))fail('Wrong scrub order advanced');
      for(const [x,z,id] of [[-662,-870,'brother1'],[-330,-810,'brother2'],[-890,-730,'brother3']]){
        walk(x,z);
        for(let i=0;i<185 && d.parts.get(id).model.visible;i++)tick(0,0,0,Z);
        if(d.parts.get(id).model.visible)fail('Did not reflect '+id);
      }
      if(!save.dekuCleared('brothers'))fail('Boss door stayed locked');
      walk(-865,-910);action('ENTER GOHMA',0);
    });
    await page.waitForFunction(()=>window.__JUMPMAN__.playground.gohma);
    await page.evaluate(()=>{
      const g=window.__JUMPMAN__, b=g.playground.gohma, m=g.mario, save=g.save;
      const tick=(pressed=0,x=0,y=0)=>{
        g.input.consumeStep=()=>({stickX:x,stickY:y,stickMag:Math.hypot(x,y),buttonDown:pressed,buttonPressed:pressed});
        g.stepSimulation();
      };
      const approach=(pos,r)=>{
        for(let i=0;i<260;i++){
          const dx=pos.x-m.pos[0],dz=pos.z-m.pos[2];
          if(Math.hypot(dx,dz)<r)return;
          const yaw=Math.atan2(dx,dz)-g.rig.yawS16*Math.PI/32768;
          tick(0,64*Math.sin(yaw),-64*Math.cos(yaw));
        }
        throw Error('Arena movement blocked '+m.pos+' toward '+pos.toArray());
      };
      approach(b.boss.home,1100);
      for(let attempt=0;attempt<6 && !save.dekuCleared('gohma');attempt++){
        for(let i=0;i<500 && !b.targets().length;i++)tick();
        g.input.keys.add('KeyH');tick();
        const sling=g.playground.slingshot;
        for(let i=0;i<120;i++){
          const target=b.targets()[0]?.center;
          if(!target)break;
          const dx=target.x-m.pos[0],dz=target.z-m.pos[2],dy=target.y-m.pos[1]-130;
          const yaw=Math.atan2(dx,dz)-sling.yaw,pitch=Math.atan2(dy,Math.hypot(dx,dz))-sling.pitch;
          if(Math.abs(Math.sin(yaw/2))<.012 && Math.abs(pitch)<.023){tick(0x8000);break;}
          tick(0,Math.max(-64,Math.min(64,-Math.atan2(Math.sin(yaw),Math.cos(yaw))/.0007)),Math.max(-64,Math.min(64,pitch/.00055)));
        }
        g.input.keys.delete('KeyH');
        for(let i=0;i<75 && b.phase!=='stunned';i++)tick();
        if(b.phase!=='stunned')continue;
        approach(b.boss.model.position,290);
        for(let i=0;i<160 && b.phase==='stunned';i++)tick(i%18===0?0x4000:0);
        for(let i=0;i<75;i++)tick();
      }
      if(!save.dekuCleared('gohma'))throw Error('Gohma survived '+JSON.stringify([b.phase,b.health,save.dekuSeeds,m.pos]));
      approach(b.heart.home,120);tick();
      if(!save.dekuCleared('heart') || m.maxHealth!==0x980)throw Error('Missing Heart Container');
      approach(b.portal.position,155);tick(0x8000);
    });
    await page.waitForFunction(()=>window.__JUMPMAN__.debugState.world==='kokiri_forest');
    await page.evaluate(()=>{
      const g=window.__JUMPMAN__;
      for(let i=0;i<50 && !g.save.dekuCleared('emerald');i++){
        g.villageTalk.textbox.finishReveal();
        g.input.consumeStep=()=>({stickX:0,stickY:0,stickMag:0,buttonDown:0x8000,buttonPressed:0x8000});
        g.stepSimulation();
      }
      if(!g.save.dekuCleared('emerald'))throw Error('Missing farewell or Kokiri Emerald');
    });
    await page.reload({waitUntil:'domcontentloaded'});
    await page.waitForFunction(()=>window.__JUMPMAN__?.playground?.kokiri);
    await page.evaluate(()=>window.__JUMPMAN__.loadCourse('deku_tree'));
    await page.evaluate(()=>{
      const g=window.__JUMPMAN__, dungeon=g.playground.dekuDungeon;
      if(!['ladder','web','scrub','eye','crossed','block','burned','brothers','gohma','heart','emerald'].every(flag=>g.save.dekuCleared(flag)) ||
         !g.save.slingshotEquipped || g.save.dekuSeeds>=28 ||
         ['web','scrub','gate','lowerWeb','bossDoor'].some(role=>dungeon.parts.get(role).model.visible) ||
         dungeon.ladderDrop!==280) throw Error('Dungeon progress did not survive reload');
      const base=dungeon.parts.get('web').home;
      g.teleport3(base.x-55*4.4,-880*4.4,base.z+240*4.4);
      const tick=pressed=>{
        g.input.consumeStep=()=>({stickX:0,stickY:0,stickMag:0,buttonDown:pressed,buttonPressed:pressed});
        g.stepSimulation();
      };
      g.start();tick(0);
      if(!dungeon.prompt?.includes('SHORTCUT'))throw Error('Restored shortcut unavailable');
      tick(0x8000);for(let i=0;i<55;i++)tick(0);
      if(Math.abs(g.mario.pos[0]-(base.x-470*4.4))>100)throw Error('Restored shortcut failed');
    });
    console.log(character+' completed the Deku Tree route');
    await context.close();
  }
} finally {await browser.close();}
