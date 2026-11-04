import puppeteer from 'puppeteer-core';
const browser = await puppeteer.launch({executablePath:'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',headless:true,args:['--no-sandbox','--enable-unsafe-swiftshader','--use-gl=angle','--use-angle=swiftshader','--window-size=1440,900'],defaultViewport:{width:1440,height:900}});
const page = await browser.newPage();
page.on('pageerror', e=>console.log('ERR',e.message));
page.on('console', m=>{ if(m.type()==='error') console.log('CONSOLE',m.text()); });
await page.goto('http://127.0.0.1:5173/?level=playground&character=procedural',{waitUntil:'networkidle0'});
await new Promise(r=>setTimeout(r,900));
await page.click('#start-button');
await new Promise(r=>setTimeout(r,600));
const spots = [
  ['spawn', 0, 80, 0x8000],
  ['jump-garden', 0, -1500, 0x8000],
  ['slide-rink', 1500, 700, 0x8000],
  ['canyon', -1480, 100, 0x8000],
  ['long-jump', 0, -1400, 0x8000],
  ['tower', 1180, -1500, 0x8000],
  ['slope-lab', 0, 1950, 0x8000],
  ['stairs', -1000, 900, 0x8000],
];
for (const [name,x,z,yaw] of spots) {
  await page.evaluate((x,z,yaw)=>window.__JUMPMAN__.teleport(x,z,yaw), x,z,yaw);
  await new Promise(r=>setTimeout(r,900));
  await page.screenshot({path:`screenshots/zone-${name}.png`});
  console.log(name, await page.evaluate(()=>window.__JUMPMAN__.debugState.pos));
}
await browser.close();
