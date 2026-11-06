import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import puppeteer from 'puppeteer-core';

const regions = ['hyrule_field','kakariko_village','graveyard','lost_woods','sacred_forest_meadow',
  'zoras_river','zoras_domain','zoras_fountain','lake_hylia','gerudo_valley','gerudos_fortress',
  'haunted_wasteland','desert_colossus','hyrule_castle','death_mountain_trail','death_mountain_crater','goron_city'];

// Every linked floor must resolve to a real arrival marker in the destination.
for (const id of regions) {
  const data = JSON.parse(await readFile('assets/courses/'+id+'.json','utf8'));
  for (const {course,entry} of Object.values(data.kokiri.exits)) {
    const target = JSON.parse(await readFile('assets/courses/'+course+'.json','utf8'));
    assert(target.kokiri.arrivals[entry], id+' to '+course+' at '+entry);
  }
}
const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args:['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  for (const character of ['sm64','link']) {
    const page = await browser.newPage();
    const errors = [];
    page.on('pageerror',e=>errors.push(e.message));
    await page.goto(new URL('?level=hyrule_field&character='+character,process.env.URL || 'http://localhost:5173'));
    await page.waitForFunction(c=>window.__JUMPMAN__?.debugState.character===c,{},character);
    await page.click('#start-button');
    for (const id of regions) {
      const result = await page.evaluate(async id=>{
        const g=window.__JUMPMAN__;
        if (g.debugState.world!==id) await g.loadCourse(id);
        if (g.debugState.world!==id) throw Error('Failed to load '+id);
        if (id==='hyrule_field') {
          const {keyDir,fillDir} = g.playground.kokiri.light;
          if (keyDir[1]<=0 || fillDir[1]>=0 || g.sunOffset.length()<1)
            throw Error('Hyrule Field daytime lights must illuminate the player');
        }
        const m=g.mario, start=[...m.pos], original=g.input.consumeStep;
        const heading=m.faceAngle[1]*Math.PI/32768 - g.rig.yawS16*Math.PI/32768;
        g.input.consumeStep=()=>({stickX:48*Math.sin(heading),stickY:-48*Math.cos(heading),stickMag:48,buttonPressed:0,buttonDown:0});
        try {
          for (let i=0;i<24;i++) g.stepSimulation();
        } finally { g.input.consumeStep=original; }
        return {distance:Math.hypot(m.pos[0]-start[0],m.pos[2]-start[2]),floor:!!m.floor,
          respawns:m.respawns,world:g.debugState.world,loading:g.loadingCourse};
      },id);
      assert(result.distance>45 && result.floor && !result.respawns && !result.loading && result.world===id,id+' '+JSON.stringify(result));
    }
    await page.evaluate(async()=>{
      const g=window.__JUMPMAN__;
      await g.loadCourse('kakariko_village');
      for (const to of ['graveyard','kakariko_village']) {
        const w=g.playground;
        const floor=w.collision.surfaces.find(s=>s.normal[1]>.8 && w.kokiri.exits[s.room]?.course===to);
        const entry=w.kokiri.exits[floor.room].entry;
        g.teleport3(...[0,1,2].map(i=>(floor.v1[i]+floor.v2[i]+floor.v3[i])/3));
        g.stepSimulation();
        for(let i=0;g.loadingCourse && i<400;i++) await new Promise(r=>setTimeout(r,20));
        const arrival=g.playground.kokiri.arrivals[entry];
        if(g.debugState.world!==to || Math.hypot(g.mario.pos[0]-arrival.pos[0],g.mario.pos[2]-arrival.pos[2])>80)
          throw Error('Bad transition to '+to);
      }
    });
    assert.deepEqual(errors,[]);
    console.log(character+' explored all '+regions.length+' areas and crossed both Graveyard entrances.');
    await page.close();
  }
} finally { await browser.close(); }
