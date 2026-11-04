import puppeteer from 'puppeteer-core';
const browser = await puppeteer.launch({executablePath:'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',headless:true,args:['--no-sandbox','--enable-unsafe-swiftshader','--use-gl=angle','--use-angle=swiftshader','--window-size=1440,900'],defaultViewport:{width:1440,height:900}});
const page = await browser.newPage();
const errors=[]; page.on('pageerror', e=>errors.push(e.message));
await page.goto('http://127.0.0.1:5173/?level=playground&character=procedural',{waitUntil:'networkidle0'});
await new Promise(r=>setTimeout(r,900));
await page.click('#start-button');
await new Promise(r=>setTimeout(r,500));
const before = await page.evaluate(()=>window.__JUMPMAN__.debugState);

// Drag the camera 320 px to the left.
await page.mouse.move(720, 450);
await page.mouse.down();
for (let i=1;i<=16;i++) await page.mouse.move(720 - i*20, 450, {steps:1});
await page.mouse.up();
await new Promise(r=>setTimeout(r,400));
const afterDrag = await page.evaluate(()=>window.__JUMPMAN__.debugState);

await page.keyboard.down('w');
await new Promise(r=>setTimeout(r,1200));
const moved = await page.evaluate(()=>window.__JUMPMAN__.debugState);
await page.keyboard.up('w');
await page.screenshot({path:'screenshots/05-camera-relative.png'});

const expected = ((afterDrag.cameraYaw + 0x8000) & 0xffff);
const expectedSigned = expected >= 0x8000 ? expected - 0x10000 : expected;
const raw = ((moved.faceYaw - expectedSigned + 0x18000) % 0x10000) - 0x8000;
const diff = Math.abs(raw);
console.log('before yaw', before.cameraYaw, 'after drag', afterDrag.cameraYaw, 'face', moved.faceYaw, 'expected', expectedSigned, 'diff', diff);
console.log('start pos', before.pos, 'moved pos', moved.pos, 'action', moved.actionName);
console.log('errors', errors);
await browser.close();
if (errors.length) process.exit(1);
if (afterDrag.cameraYaw === before.cameraYaw) { console.error('camera did not rotate'); process.exit(1); }
if (diff > 1200) { console.error('movement not camera-relative'); process.exit(1); }
