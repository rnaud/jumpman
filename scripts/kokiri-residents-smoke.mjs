import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath:process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args:['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  for (const character of ['sm64','link']) {
    const page=await browser.newPage();
    const errors=[];
    page.on('pageerror',e=>errors.push(e.message));
    await page.goto(new URL('?level=kokiri_forest&character='+character,process.env.URL || 'http://localhost:5173'));
    await page.waitForFunction(c=>window.__JUMPMAN__?.debugState.character===c,{},character);
    await page.click('#start-button');
    const result=await page.evaluate(async()=>{
      const g=window.__JUMPMAN__, visit=g.villageTalk;
      const A=0x8000, found=[], destinations=new Set();
      const tick=(buttons=0,y=0)=>{
        const old=g.input.consumeStep;
        g.input.consumeStep=()=>({stickX:0,stickY:y,stickMag:Math.abs(y),buttonDown:buttons,buttonPressed:buttons});
        try {g.stepSimulation();} finally {g.input.consumeStep=old;}
      };
      for (const id of ['kokiri_forest','kokiri_shop','twins_house','know_it_all_house','midos_house','sarias_house']) {
        if (g.debugState.world!==id) await g.loadCourse(id);
        const scene=g.playground.kokiri;
        if (!scene || g.debugState.world!==id) throw Error('Failed to load '+id);
        for (const npc of scene.talks.filter(t=>t.kind!=='sign')) {
          const [x,y,z]=npc.pos;
          g.teleport3(x,y,z+65);
          tick(A);
          if (visit.speaking!==npc) throw Error('Wrong conversation for '+npc.id+' in '+id);
          found.push(npc.id ?? npc.kind);
          const choices=visit.pages.find(p=>p.branches)?.branches.length ?? 0;
          for (let selected=0;selected<choices;selected++) {
            if (selected) {
              visit.clear();
              tick(A);
            }
            let attempts=0;
            while (!visit.pages[visit.index].branches) {
              visit.textbox.finishReveal();
              tick(A);
              if (++attempts>20) throw Error('Missing choice page');
            }
            const answers=visit.pages[visit.index].branches;
            for (let i=0;i<selected;i++) {tick(0,-64);tick();}
            if (visit.textbox.choice!==selected) throw Error('Wrong selected answer');
            visit.textbox.finishReveal();
            tick(A);
            if (visit.pages[visit.index].text!==answers[selected][0].text) throw Error('Wrong answer branch');
          }
          visit.clear();
        }
        for (const exit of Object.values(scene.exits)) destinations.add(exit.course);
        if (['twins_house','know_it_all_house','midos_house','sarias_house'].includes(id)) {
          // Stand on the original exit floor and let the game perform the transition.
          const surface=g.playground.collision.surfaces.find(s=>scene.exits[s.room] && s.normal[1]>0.9);
          const center=[0,1,2].map(i=>(surface.v1[i]+surface.v2[i]+surface.v3[i])/3);
          g.teleport3(...center);
          tick();
          for (let i=0;i<300 && g.loadingCourse;i++) await new Promise(r=>setTimeout(r,20));
          if (g.debugState.world!=='kokiri_forest') throw Error('Exit failed from '+id);
        }
      }
      return {found,destinations:[...destinations]};
    });
    assert.deepEqual(result.found.sort(),['saria','keeper','mido',...Array.from({length:13},(_,i)=>'kokiri_'+i)].sort());
    assert(result.destinations.includes('know_it_all_house') && result.destinations.includes('twins_house'));
    assert.deepEqual(errors,[]);
    if (process.env.SHOT_DIR) {
      await page.evaluate(async()=>{
        const g=window.__JUMPMAN__;await g.loadCourse('know_it_all_house');
        const npc=g.playground.kokiri.talks.find(n=>n.id==='kokiri_7');
        g.teleport3(npc.pos[0],npc.pos[1],npc.pos[2]+65);
      });
      await page.keyboard.press('Space');
      await page.waitForSelector('#kokiri-dialogue:not([hidden])');
      await new Promise(r=>setTimeout(r,2400));
      await page.screenshot({path:`${process.env.SHOT_DIR}/kokiri-choice-${character}.png`});
    }
    console.log(character+' spoke to all 16 opening-day residents and followed the brothers’ choice branches.');
    await page.close();
  }
} finally {await browser.close();}
