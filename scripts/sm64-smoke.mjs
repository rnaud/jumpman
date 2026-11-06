import puppeteer from 'puppeteer-core';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
const base = process.env.URL || 'http://127.0.0.1:5173/';
const out = process.env.SCREENSHOT_DIR || '/tmp/jumpman-sm64-shots';
await fs.mkdir(out, { recursive: true });
const browser = await puppeteer.launch({executablePath:'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',headless:true,args:['--no-sandbox','--enable-unsafe-swiftshader','--use-gl=angle','--use-angle=swiftshader'],defaultViewport:{width:1440,height:900}});
try {
 const page=await browser.newPage();
 const errors=[];const warnings=[];
 page.on('pageerror',e=>errors.push(e.message));
 page.on('console',m=>{if(m.type()==='error')errors.push(m.text());if(m.type()==='warn' && m.text().startsWith('[jumpman]'))warnings.push(m.text());});
 await page.goto(base,{waitUntil:'networkidle0',timeout:60000});
 await page.waitForFunction(()=>window.__JUMPMAN__?.debugState.characterLoaded,{timeout:30000});
 const state=()=>page.evaluate(()=>window.__JUMPMAN__.debugState);
 const sleep=ms=>new Promise(r=>setTimeout(r,ms));
 let s=await state();console.log('BOOT',JSON.stringify({world:s.world,character:s.character,clips:s.characterClips,triangles:s.collisionTriangles,spawn:s.pos}));
 assert.equal(s.world,'bob'); assert.equal(s.character,'sm64');assert.equal(s.characterClips,209);assert(s.collisionTriangles>1000);assert(s.floorNormal);
 await page.click('#start-button');await sleep(400);
 s=await state();assert(['anim_C3','anim_C4','anim_C5'].includes(s.characterClip));
 assert.equal(s.cameraMode,'lakitu');
 await page.keyboard.press('c');await sleep(100);s=await state();assert.equal(s.cameraMode,'close');
 await page.keyboard.press('c');await page.keyboard.press('c');await page.keyboard.press('c');await sleep(100);s=await state();assert.equal(s.cameraMode,'lakitu');
 console.log('CAMERA',s.cameraMode);
 await page.screenshot({path:`${out}/battlefield.png`});
 await page.keyboard.down('w');await sleep(1200);s=await state();console.log('RUN',s.characterClip,s.forwardVel,s.pos);assert.equal(s.characterClip,'anim_72');assert(s.forwardVel>10);
 await page.screenshot({path:`${out}/running.png`});
 await page.keyboard.up('w');
 await page.evaluate(()=>window.__JUMPMAN__.teleport3(-6558,0,6464,0x6000));
 await sleep(150);await page.keyboard.down('Space');
 await page.waitForFunction(()=>{const s=window.__JUMPMAN__.debugState;return s.actionName==='JUMP' && s.pos[1]>s.floorHeight+10;},{timeout:5000});s=await state();console.log('JUMP',s.characterClip,s.actionName,s.pos);assert.equal(s.characterClip,'anim_4D');assert(s.pos[1]>s.floorHeight+10);
 await page.screenshot({path:`${out}/jump.png`});await page.keyboard.up('Space');await page.keyboard.up('w');await sleep(1000);
 await page.keyboard.press('v');s=await state();assert.equal(s.character,'procedural');await page.keyboard.press('v');s=await state();assert.equal(s.character,'sm64');
 await page.keyboard.press('r');await sleep(100);s=await state();assert.equal(s.coins,0);assert(s.floorNormal);
 // Coin collection/star completion and reset use the same gameplay path on the imported course.
 for(let i=0;i<s.coinPositions.length;i++){
   await page.evaluate(index=>{const game=window.__JUMPMAN__,coin=game.debugState.coinPositions[index];game.teleport3(coin.x,coin.y-55,coin.z);},i);
   await sleep(150);
 }
 s=await state();console.log('COINS',s.coins,s.starReady);assert.equal(s.coins,13);assert(!s.starReady); // Bob's star is mission-gated, not coin-gated.
 await page.evaluate(()=>{const g=window.__JUMPMAN__;g.missionComplete=true;g.starReady=true;const [x,y,z]=g.debugState.starPos;g.teleport3(x,y-60,z);});await sleep(200);assert((await state()).won);
 await page.click('#castle-button');await page.waitForFunction(()=>window.__JUMPMAN__?.debugState.world==='castle_grounds',{timeout:20000});
 await page.keyboard.press('r');await sleep(100);s=await state();assert(!s.won);assert.equal(s.coins,0);
 await page.goto(new URL('?level=playground',base).href,{waitUntil:'networkidle0'});await page.waitForFunction(()=>window.__JUMPMAN__?.debugState.characterLoaded);s=await state();assert.equal(s.world,'playground');assert.equal(s.character,'sm64');
 await page.click('#start-button');await sleep(300);await page.screenshot({path:`${out}/playground.png`});
 // The optional CC0/custom loader still works alongside the native path.
 await page.goto(new URL('?level=playground&character=cc0',base).href,{waitUntil:'networkidle0'});
 await page.waitForFunction(()=>window.__JUMPMAN__?.debugState.characterLoaded);
 s=await state();assert.equal(s.character,'gltf');assert.equal(s.characterClip,'Idle_Loop');
 assert.deepEqual(errors,[]);assert.deepEqual(warnings,[]);
 console.log('PASS: imported world, native clips, movement/jump, swapping, coins/star, reset, playground; no browser errors.');
} finally {await browser.close();}
