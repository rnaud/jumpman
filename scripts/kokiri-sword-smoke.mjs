import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args: ['--no-sandbox', '--enable-unsafe-swiftshader'],
});
try {
  for (const character of ['link','sm64']) {
    const context = await browser.createBrowserContext();
    const page = await context.newPage();
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(new URL('?level=kokiri_forest&character='+character, process.env.URL || 'http://localhost:5173'));
    await page.waitForFunction(c => window.__JUMPMAN__?.debugState.character === c, {}, character);
    await page.click('#start-button');
    await page.evaluate(async character => {
      const g = window.__JUMPMAN__, visit = g.villageTalk;
      const A = 0x8000;
      const check = (value, detail) => { if (!value) throw Error(detail); };
      const tick = (button=0, dx=0, heading=Math.PI/2) => {
        const original = g.input.consumeStep;
        const angle = heading - g.rig.yawS16*Math.PI/32768;
        g.input.consumeStep = () => ({stickX:dx*Math.sin(angle),stickY:-dx*Math.cos(angle),stickMag:dx,buttonDown:button,buttonPressed:button});
        try { g.stepSimulation(); } finally { g.input.consumeStep = original; }
      };
      const finish = () => {
        for (let i=0; visit.pages.length && i<20; i++) { visit.textbox.finishReveal(); tick(A); }
        check(!visit.pages.length, 'Dialogue did not close');
      };
      const gate = (open) => {
        const [x,y,z] = g.playground.kokiri.quest.path[0];
        g.teleport3(x-580,y,z);
        for (let i=0;i<70;i++) tick(0,64);
        check(open ? g.mario.pos[0]>x+100 : g.mario.pos[0]<x, 'Gate crossing '+open+' at '+g.mario.pos);
      };
      const speak = (approval) => {
        const [x,y,z] = g.playground.kokiri.quest.path[0];
        g.teleport3(x-405,y,z);
        tick(A);
        check(visit.speakerId==='mido' && visit.approvingMido===approval, 'Mido equipment check');
        finish();
        tick();
        check(g.save.dekuPathOpen===approval,'Permission after dialogue');
      };
      const shield = async () => {
        await g.loadCourse('kokiri_shop');
        g.save.collectVillageRupees('test-wallet',40,false);
        g.teleport3(...g.playground.kokiri.counter.shield);
        tick(A); tick(A); finish();
        check(g.save.dekuShield,'Shop purchase');
        await g.loadCourse('kokiri_forest');
      };
      gate(false);
      speak(false);
      if (character==='link') { await shield(); speak(false); }
      const {crawl,chest} = g.playground.kokiri.quest;
      g.teleport3(...crawl[0]); tick(A);
      check(visit.climb?.crawling,'Crawl did not start');
      for (let i=1;i<100;i++) tick();
      check(Math.hypot(...g.mario.pos.map((v,i)=>v-crawl[1][i]))<2,'Crawl destination');
      // Walk the bends in the original training-ground maze with the active controller.
      for (const [x,z] of [[-11600,7900],[-11300,8200],[-10900,8200],[-10600,8500],
        [-10600,9400],[-10400,9500],[-10000,9500],[-9800,9700],[-9400,9700],
        [-9300,9900],[-9300,10800],[chest[0],chest[2]-145]]) {
        for (let i=0;i<150;i++) {
          const dx=x-g.mario.pos[0], dz=z-g.mario.pos[2];
          if (Math.hypot(dx,dz)<60) break;
          tick(0,48,Math.atan2(dx,dz));
        }
        check(Math.hypot(g.mario.pos[0]-x,g.mario.pos[2]-z)<60,'Maze turn '+[x,z]+' from '+g.mario.pos);
      }
      tick(A);
      check(g.save.kokiriSword && visit.pages.length===2,'Sword pickup');
      finish();
      for(let i=0;i<32;i++) tick();
      check(g.playground.forestQuest.sword.visible,'Sword reward model');
      if (character==='sm64') { speak(false); await shield(); }
      gate(false);
      speak(true);
      for (let i=0;i<130;i++) tick();
      const q = g.playground.forestQuest;
      check(q.mido.position.distanceTo(q.destination)<1,'Mido did not step aside');
      gate(true);
      const walk = (x,z,stop) => {
        for (let i=0; i<350; i++) {
          if (stop?.()) return;
          const dx=x-g.mario.pos[0], dz=z-g.mario.pos[2];
          if (Math.hypot(dx,dz)<50) return;
          tick(0,52,Math.atan2(dx,dz));
        }
        throw Error('Walk blocked at '+g.mario.pos+' toward '+[x,z]);
      };
      const arrive = async id => {
        for (let i=0;i<400 && g.loadingCourse;i++) await new Promise(r=>setTimeout(r,20));
        check(g.debugState.world===id,'Expected '+id+', got '+g.debugState.world);
      };
      for (const [x,z] of [[900,1800],[1000,400],[1900,-600],[2500,-600],[4000,-1800],
        [5500,-2500],[7700,-3000]]) {
        walk(x,z,()=>!!visit.treeMeeting);
        if (visit.treeMeeting) break;
      }
      check(visit.treeMeeting && !q.mouthOpen, 'Meeting the Great Deku Tree');
      while (!visit.pages[visit.index].branches) { visit.textbox.finishReveal(); tick(A); }
      visit.textbox.choose(-64);
      finish();
      check(!g.save.hasSpokenTo('dekuTreeAccepted') && !q.mouthOpen,'Refusing should keep the mouth closed');
      tick(A);
      check(visit.pages.length===2,'Retry the Tree’s offer');
      finish();
      check(g.save.hasSpokenTo('dekuTreeAccepted'),'Accepted the curse quest');
      tick();
      check(q.mouthPhase>0 && q.mouthPhase<1,'Mouth moves gradually');
      for (let i=0;i<155;i++) tick();
      check(q.mouthOpen,'Tree mouth finished opening');
      walk(8668,-3632);
      walk(10200,-4550,()=>g.loadingCourse);
      await arrive('deku_tree');
      const spawn=[...g.mario.pos];
      walk(spawn[0],spawn[2]-1500);
      check(g.mario.floor && Math.abs(g.mario.pos[1]-spawn[1])<80,'Dungeon lobby floor');
      walk(spawn[0],spawn[2]+570,()=>g.loadingCourse);
      await arrive('kokiri_forest');
      check(g.playground.forestQuest.mouthOpen,'The Tree stays open on return');
      const back=g.playground.kokiri.arrivals.tree.pos;
      check(Math.hypot(g.mario.pos[0]-back[0],g.mario.pos[2]-back[2])<80,'Walked out at the Tree');
    }, character);
    await page.reload();
    await page.waitForFunction(()=>window.__JUMPMAN__?.playground?.forestQuest);
    assert(await page.evaluate(()=>{
      const g=window.__JUMPMAN__, q=g.playground.forestQuest;
      return g.save.kokiriSword && g.save.dekuPathOpen && q.mouthOpen && !q.walls.size &&
        q.opening.time===q.opening.getClip().duration && q.mido.position.distanceTo(q.destination)<1;
    }));
    assert.deepEqual(errors,[]);
    console.log(character+' opened the chest, bought the shield, passed Mido, and walked into and back out of the Great Deku Tree.');
    await context.close();
  }
} finally { await browser.close(); }
