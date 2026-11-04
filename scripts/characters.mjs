import puppeteer from 'puppeteer-core';
const browser = await puppeteer.launch({executablePath:'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',headless:true,args:['--no-sandbox','--enable-unsafe-swiftshader','--use-gl=angle','--use-angle=swiftshader','--window-size=1440,900'],defaultViewport:{width:1440,height:900}});
const page = await browser.newPage();
const errors=[]; page.on('pageerror',e=>errors.push(e.message));
page.on('console',m=>{ if(m.type()==='error') errors.push(m.text()); });
const URL = process.env.URL || 'http://127.0.0.1:5173/?level=playground&character=cc0';
await page.goto(URL,{waitUntil:'networkidle0',timeout:60000});
await new Promise(r=>setTimeout(r,1200));
const state=()=>page.evaluate(()=>window.__JUMPMAN__.debugState);
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function waitFor(fn,timeout=25000){const t=Date.now();while(Date.now()-t<timeout){const s=await state();if(fn(s))return s;await sleep(150);}return await state();}
let s=await waitFor(s=>s.characterLoaded);
console.log('loaded',s.characterLoaded,'rig',s.character);
if(s.character !== 'procedural') await page.evaluate(()=>window.__JUMPMAN__.swapCharacter());
await page.click('#start-button');
await sleep(600);
await page.screenshot({path:'screenshots/11-procedural.png'});

await page.keyboard.press('v');
await sleep(700);
s=await state(); console.log('after V',s.character,'clip',s.characterClip);
await page.screenshot({path:'screenshots/12-cc0-idle.png'});

await page.keyboard.down('w');
await sleep(1200);
s=await state(); console.log('run clip',s.characterClip,'fv',s.forwardVel.toFixed(1));
await page.screenshot({path:'screenshots/13-cc0-run.png'});
await page.keyboard.up('w');
await sleep(500);

await page.keyboard.down('Space'); await sleep(150);
s=await state(); console.log('jump clip',s.characterClip,'y',s.pos[1].toFixed(0));
await page.screenshot({path:'screenshots/14-cc0-jump.png'});
await page.keyboard.up('Space');
await sleep(900);

await page.keyboard.press('v');
await sleep(500);
s=await state(); console.log('back to',s.character,'clip',s.characterClip);
await page.screenshot({path:'screenshots/15-procedural-close.png'});
console.log('errors',errors);
await browser.close();
