import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath:process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args:['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  const page=await browser.newPage();
  const errors=[];
  page.on('pageerror',e=>errors.push(e.message));
  await page.goto(new URL('?level=castle_inside',process.env.URL || 'http://localhost:5173/').href,{waitUntil:'networkidle0'});
  await page.click('#start-button');
  await page.evaluate(()=>{
    const g=window.__JUMPMAN__, tile=g.playground.collision.surfaces.find(s=>s.type===0x2f);
    const pos=tile.v1.map((_,i)=>(tile.v1[i]+tile.v2[i]+tile.v3[i])/3);
    g.teleport3(...pos,-32768);
    for(let i=1;i<=6;i++)g.save.collectStar('bob',i);
    for(let i=1;i<=3;i++)g.save.collectStar('wf',i);
  });
  await page.keyboard.down('ArrowUp');
  await page.evaluate(()=>{const g=window.__JUMPMAN__;for(let i=0;i<65;i++)g.stepSimulation();});
  assert.equal(await page.evaluate(()=>window.__JUMPMAN__.worldName),'castle_inside');
  await page.evaluate(()=>{const g=window.__JUMPMAN__;g.save.collectStar('wf',4);for(let i=0;i<30;i++)g.stepSimulation();});
  await page.waitForFunction(()=>window.__JUMPMAN__.worldName==='totwc');
  await page.keyboard.up('ArrowUp');
  const tower=await page.evaluate(()=>{
    const g=window.__JUMPMAN__, m=g.mario;
    const entry={flying:m.action===0x10880899,cap:m.wingCapTimer,redCoins:g.playground.redCoins.length};
    const pos=g.playground.wingSwitch.pos;
    const floor=g.playground.collision.findFloor(pos[0],pos[1]+500,pos[2]);
    g.teleport3(pos[0],floor.height+60,pos[2]); m.setAction(0x0100088c);
    for(let i=0;i<40;i++)g.stepSimulation();
    if(!g.save.wingSwitchOn)throw Error('Landing on switch failed: '+[m.pos,m.floor?.type,floor.height]);
    const barrier=g.playground.collision.surfaces.find(s=>s.type===10);
    const spot=barrier.v1.map((_,i)=>(barrier.v1[i]+barrier.v2[i]+barrier.v3[i])/3);
    g.teleport3(spot[0],spot[1]+1200,spot[2]);
    for(let i=0;i<22;i++)g.stepSimulation();
    return entry;
  });
  assert(tower.flying && tower.cap>1150 && tower.redCoins===8);
  await page.waitForFunction(()=>window.__JUMPMAN__.worldName==='castle_inside');
  await page.goto(new URL('?level=bob&act=5',page.url()).href,{waitUntil:'networkidle0'});
  await page.click('#start-button');
  const outcome=await page.evaluate(()=>{
    const g=window.__JUMPMAN__, m=g.mario;
    const box=g.playground.objects.manager.objects.find(o=>o.behavior==='wingBox');
    if(!box.vars.unlocked) throw Error('Wing box did not retain switch unlock');
    const [x,y,z]=box.pos;
    g.teleport3(x,y+300,z); m.setAction(0x008008a9);
    for(let i=0;i<90 && !box.link;i++)g.stepSimulation();
    if(!box.link)throw Error('Pound did not break wing box: '+[m.action,m.pos,box.action]);
    const cap=box.link;
    for(let i=0;i<22;i++)g.stepSimulation();
    g.teleport3(...cap.pos);g.stepSimulation();
    return {wing:m.wingCapTimer,switchOn:g.save.wingSwitchOn};
  });
  assert(outcome.wing>=1798 && outcome.switchOn);
  assert.deepEqual(errors,[]);
  console.log({tower,outcome});
}finally{await browser.close()}
