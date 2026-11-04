import puppeteer from 'puppeteer-core';
const browser = await puppeteer.launch({executablePath:'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',headless:true,args:['--no-sandbox','--enable-unsafe-swiftshader','--use-gl=angle','--use-angle=swiftshader','--window-size=1440,900'],defaultViewport:{width:1440,height:900}});
const page = await browser.newPage();
const errors=[]; page.on('pageerror',e=>errors.push(e.message));
await page.goto('http://127.0.0.1:5173/?level=playground&character=procedural',{waitUntil:'networkidle0'});
await new Promise(r=>setTimeout(r,800));
await page.click('#start-button');
await new Promise(r=>setTimeout(r,400));
const state=()=>page.evaluate(()=>window.__JUMPMAN__.debugState);
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function waitFor(fn,timeout=3000){const t=Date.now();while(Date.now()-t<timeout){const s=await state();if(fn(s))return s;await sleep(12);}return await state();}

// Dive on the ice rink.
await page.evaluate(()=>window.__JUMPMAN__.teleport(1500,100,0));
await sleep(600);
await page.keyboard.down('w');
let s=await waitFor(s=>s.forwardVel>29,4000);
console.log('run', s.forwardVel.toFixed(1), s.actionName, s.pos[1].toFixed(0));
await page.keyboard.down('Shift'); await sleep(60); await page.keyboard.up('Shift');
s=await waitFor(s=>s.actionName==='DIVE',1000); console.log('dive', s.actionName, s.vel[1].toFixed(1));
await page.screenshot({path:'screenshots/07-dive.png'});
await page.keyboard.up('w'); await sleep(1400);

// Backflip visual.
await page.evaluate(()=>window.__JUMPMAN__.teleport(-700,300,0x8000));
await sleep(400);
await page.keyboard.down('Control'); await sleep(350);
await page.keyboard.down('Space'); await sleep(60); await page.keyboard.up('Space');
s=await waitFor(s=>s.actionName==='BACKFLIP',1000);
await sleep(180); await page.screenshot({path:'screenshots/08-backflip.png'});
console.log('backflip',s.actionName);
await page.keyboard.up('Control'); await sleep(1200);

// Wall kick: build speed, jump just before the end wall.
await page.evaluate(()=>window.__JUMPMAN__.teleport(-1480,-450,0x8000));
await sleep(500);
await page.keyboard.down('w');
s=await waitFor(s=>s.pos[2]<-920 && s.forwardVel>20,5000);
console.log('pre-wall', s.pos[2].toFixed(0), s.forwardVel.toFixed(1));
await page.keyboard.down('Space'); await sleep(45); await page.keyboard.up('Space');
s=await waitFor(s=>s.actionName==='AIR HIT WALL'||s.actionName==='SOFT BONK',1500);
console.log('wallhit', s.actionName, s.pos[2].toFixed(0), s.pos[1].toFixed(0));
if (s.actionName==='AIR HIT WALL') {
  await page.keyboard.down('Space'); await sleep(45); await page.keyboard.up('Space');
  s=await waitFor(s=>s.actionName==='WALL KICK',1000);
  console.log('wallkick', s.actionName);
  await sleep(110);
  await page.screenshot({path:'screenshots/09-wallkick.png'});
}
await page.keyboard.up('w'); await sleep(300);
console.log('errors',errors);
await browser.close();
