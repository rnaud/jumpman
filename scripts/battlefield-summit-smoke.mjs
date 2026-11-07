import { fightSummitKing } from './battlefield-missions.mjs';
import { climbBattlefieldSummit } from './battlefield-climb.mjs';
import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath:process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args:['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  const page=await browser.newPage();
  // Repeat the same enemy and water-bomb timing on every controller replay.
  await page.evaluateOnNewDocument(()=>{
    let seed=64;
    Math.random=()=>((seed=(Math.imul(seed,1664525)+1013904223)>>>0)/0x100000000);
  });
  await page.goto(new URL('?level=bob&act=1',process.env.URL || 'http://localhost:5173/').href,{waitUntil:'networkidle0'});
  await page.click('#start-button');
  await page.evaluate(climbBattlefieldSummit);
  const result=await page.evaluate(fightSummitKing);
  console.log(result);
  assert.deepEqual(result.health,[3,2,1,0]);
  assert.equal(result.pages.length,10);
  assert.deepEqual(result.target,[2000,4500,-4500]);
  assert(result.saved && result.respawns===0);
  await page.waitForSelector('#course-result button:not(:disabled)',{visible:true,timeout:90000});
  await page.keyboard.press('Space');
  await page.waitForFunction(()=>document.querySelector('#course-result').hidden);
  await page.reload({waitUntil:'domcontentloaded',timeout:90000});
  await page.waitForFunction(()=>window.__JUMPMAN__?.save.hasStar('bob',1),{timeout:90000});
}finally{await browser.close();}
