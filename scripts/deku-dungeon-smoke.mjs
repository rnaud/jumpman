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
      walk(-800,80);walk(-750,245);walk(-490,410);action('LIGHT A DEKU STICK',3);
      walk(-650,270);walk(-635,0);idle(210);
      if(!save.dekuCleared('burned'))fail('Web did not burn');
      action('SWIM TO THE SCRUBS',80);
      for(const [x,z,id] of [[-662,-870,'brother1'],[-330,-810,'brother2'],[-890,-730,'brother3']]){
        walk(x,z);
        for(let i=0;i<185 && d.parts.get(id).model.visible;i++)tick(0,0,0,Z);
        if(d.parts.get(id).model.visible)fail('Did not reflect '+id);
      }
      if(!save.dekuCleared('brothers'))fail('Boss door stayed locked');
    });
    await page.reload({waitUntil:'domcontentloaded'});
    await page.waitForFunction(()=>window.__JUMPMAN__?.playground?.dekuDungeon);
    await page.evaluate(()=>{
      const g=window.__JUMPMAN__, dungeon=g.playground.dekuDungeon;
      if(!['ladder','web','scrub','eye','crossed','block','burned','brothers'].every(flag=>g.save.dekuCleared(flag)) ||
         !g.save.slingshotEquipped || g.save.dekuSeeds!==28 ||
         ['web','scrub','gate'].some(role=>dungeon.parts.get(role).model.visible) ||
         dungeon.ladderDrop!==280) throw Error('Dungeon progress did not survive reload');
    });
    console.log(character+' completed the Deku Tree route');
    await context.close();
  }
} finally {await browser.close();}
