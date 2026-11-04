import puppeteer from 'puppeteer-core';
const browser = await puppeteer.launch({executablePath:'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',headless:true,args:['--no-sandbox','--enable-unsafe-swiftshader','--use-gl=angle','--use-angle=swiftshader','--window-size=1440,900'],defaultViewport:{width:1440,height:900}});
const page = await browser.newPage();
const errors=[]; page.on('pageerror',e=>errors.push(e.message));
await page.goto('http://127.0.0.1:5173/?level=playground&character=procedural',{waitUntil:'networkidle0'});
await new Promise(r=>setTimeout(r,800)); await page.click('#start-button'); await new Promise(r=>setTimeout(r,400));
const state=()=>page.evaluate(()=>window.__JUMPMAN__.debugState);
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
let s=await state();
console.log('coins total', s.coinPositions.length);
for (let i=0;i<s.coinPositions.length;i++) {
  const c=s.coinPositions[i];
  for (let attempt=0; attempt<3; attempt++) {
    await page.evaluate((x,y,z)=>window.__JUMPMAN__.teleport3(x,y,z), c.x, c.y-55, c.z);
    await sleep(140);
    s=await state();
    if (s.coins>i) break;
  }
  console.log(i, +(s.coins), s.coinPositions[i].collected, 'starReady', s.starReady);
}
s=await state();
console.log('all collected', s.coins, '/', s.coinPositions.length, 'starReady', s.starReady);
await page.evaluate(()=>{const st=window.__JUMPMAN__.debugState.starPos; window.__JUMPMAN__.teleport3(st[0], st[1]-60, st[2]);});
await sleep(500);
s=await state();
console.log('won', s.won, 'overlay hidden', await page.evaluate(()=>document.getElementById('win-overlay').hidden));
await page.screenshot({path:'screenshots/10-win.png'});
console.log('errors',errors);
await browser.close();
