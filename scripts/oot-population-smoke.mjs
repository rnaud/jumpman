import assert from 'node:assert/strict';
import {readFile,readdir} from 'node:fs/promises';
import puppeteer from 'puppeteer-core';

const courses=[];
for (const file of await readdir('assets/courses')) {
  if (!file.endsWith('.json')) continue;
  const data=JSON.parse(await readFile('assets/courses/'+file,'utf8'));
  const residents=data.objects?.filter(o=>o.model.includes('/oot_residents/') ||
    file==='sacred_forest_meadow.json' && o.model.endsWith('/saria.glb'));
  if (residents?.length) courses.push({id:file.slice(0,-5),residents});
}
const browser=await puppeteer.launch({
  executablePath:process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args:['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  for (const character of ['sm64','link']) {
    const page=await browser.newPage();
    const errors=[];
    page.on('pageerror',error=>errors.push(error.message));
    await page.goto(new URL('?level=kakariko_village&character='+character,process.env.URL || 'http://localhost:5173'));
    await page.waitForFunction(c=>window.__JUMPMAN__?.debugState.character===c,{},character);
    const menu='.start-course-links';
    await page.click(menu+' button.mario');
    assert(await page.$eval(menu+' .course-series.zelda',el=>el.hidden));
    await page.click(menu+' button.zelda');
    assert(await page.$eval(menu+' .course-series.zelda a[aria-current]',el=>el.textContent==='Kakariko Village'));
    await page.click('#start-button');
    await page.click('#settings-button');
    const pause = '#settings .series-selector';
    await page.click(pause+' button.mario');
    await page.click(pause+' a[href*="level=bob"]');
    await page.waitForFunction(()=>window.__JUMPMAN__.debugState.world==='bob');
    await page.click('#settings-button');
    await page.click(pause+' button.zelda');
    await page.click(pause+' a[href*="level=lon_lon_ranch"]');
    await page.waitForFunction(c=>window.__JUMPMAN__.debugState.world==='lon_lon_ranch' &&
      window.__JUMPMAN__.debugState.character===c,{},character);
    let models=0, talks=0;
    for (const course of courses) {
      talks+=await page.evaluate(async({id,residents})=>{
        const g=window.__JUMPMAN__;
        if (g.debugState.world!==id) await g.loadCourse(id);
        if(g.debugState.world!==id) throw Error('Failed to load '+id);
        const root=g.courseScene.getObjectByName(id), visit=g.villageTalk;
        for(const r of residents) {
          const actor=root.children.find(o=>Math.hypot(...o.position.toArray().map((v,i)=>v-r.pos[i]))<1 && o.getObjectByName('joint_1'));
          if (!actor) throw Error('Missing '+id+' '+r.model);
        }
        const people=g.playground.kokiri.talks.filter(t=>t.kind==='resident');
        for(const npc of people) {
          const [x,y,z]=npc.pos;
          g.teleport3(x,y,z);
          visit.step(g.playground.kokiri,g.mario,0x8000,'Visitor',g.save);
          if (visit.speaking!==npc || !visit.pages[0]?.text.trim()) throw Error('Cannot speak to '+npc.id+' in '+id);
          visit.clear();
        }
        return people.length;
      },course);
      models+=course.residents.length;
    }
    assert.deepEqual(errors,[]);
    console.log(character,models+' resident placements, '+talks+' greetings, '+courses.length+' courses');
    await page.close();
  }
} finally {await browser.close();}
