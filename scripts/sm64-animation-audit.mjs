import puppeteer from 'puppeteer-core';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
const base = process.env.URL || 'http://127.0.0.1:5173/';
const out = process.env.SCREENSHOT_DIR || '/tmp/jumpman-animation-audit';
await fs.mkdir(out,{recursive:true});
const browser = await puppeteer.launch({executablePath:'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',headless:true,args:['--no-sandbox','--enable-unsafe-swiftshader','--use-gl=angle','--use-angle=swiftshader'],defaultViewport:{width:1100,height:800}});
try {
 const page = await browser.newPage(), errors=[];
 page.on('pageerror',e=>errors.push(e.message));
 await page.goto(new URL('?level=playground',base).href,{waitUntil:'networkidle0'});
 await page.waitForFunction(()=>window.__JUMPMAN__?.debugState.characterLoaded);
 const result = await page.evaluate(async()=>{
  const THREE = await import('/node_modules/.vite/deps/three.js');
  const {Sm64MarioView} = await import('/src/game/Sm64MarioView.ts');
  const A = await import('/src/sim/state.ts');
  const tables = await Promise.all(['stationary','moving','airborne','automatic','object'].map(n=>import(`/src/sim/actions/${n}.ts`)));
  const actions = tables.flatMap(mod=>Object.entries(mod).filter(([k])=>k.endsWith('_ACTIONS')).flatMap(([,table])=>Object.keys(table).map(Number)));
  const view = await Sm64MarioView.load();
  const scene=new THREE.Scene();scene.background=new THREE.Color('#c9dbe6');scene.add(view.root);
  scene.add(new THREE.HemisphereLight(0xffffff,0x667788,2));
  const light=new THREE.DirectionalLight(0xffffff,2);light.position.set(150,300,250);scene.add(light);
  const camera=new THREE.PerspectiveCamera(35,1100/800,1,5000);camera.position.set(360,210,600);camera.lookAt(0,100,0);
  const renderer=new THREE.WebGLRenderer({antialias:true});renderer.setSize(1100,800);
  document.body.replaceChildren(renderer.domElement);
  const state={action:A.ACT_IDLE,actionArg:0,actionState:0,actionTimer:0,forwardVel:32,intendedMag:32,vel:[0,20,0],faceAngle:[0,0,0],squishTimer:0};
  const position=new THREE.Vector3();
  function pose(action,ticks=0,extra={}){
   view.update({...state,action:A.ACT_IDLE},0,position,0);
   const m={...state,action,...extra};view.update(m,0,position,0);
   for(let i=0;i<ticks;i++){m.actionTimer++;view.update(m,1/30,position,0);}
   view.faceCamera(camera);renderer.render(scene,camera);
   let finite=true;view.root.traverse(o=>{finite &&= [...o.position,...o.quaternion,...o.scale].every(Number.isFinite);});
   return {...view.animationDebug,yaw:view.root.rotation.y,pitch:view.root.rotation.x,finite};
  }
  window.__SM64_AUDIT__={pose,A};
  return {all:actions.map(a=>({action:A.actionName(a),...pose(a,4)})),
   run:pose(A.ACT_WALKING,9),side:pose(A.ACT_SIDE_FLIP,9),land:pose(A.ACT_JUMP_LAND),
   pound:pose(A.ACT_GROUND_POUND,2,{actionState:1}),wall:pose(A.ACT_WALL_KICK_AIR,4),
   dive:pose(A.ACT_DIVE,4,{faceAngle:[-8192,0,0]}),kick:pose(A.ACT_PUNCHING,4,{actionArg:2})};
 });
 assert.equal(result.all.length,55);assert(result.all.every(p=>p.finite));
 assert.equal(result.run.speed,8);assert.equal(result.run.frame,0);
 assert.equal(result.side.name,'anim_BF');assert.equal(result.side.yaw,Math.PI);
 assert.equal(result.land.frame,22);assert.equal(result.pound.frame,10);
 assert.equal(result.wall.name,'anim_CB');assert.equal(result.dive.pitch,Math.PI/4);assert.equal(result.kick.name,'anim_66');
 for (const [name,action,ticks] of [['run','ACT_WALKING',4],['side-flip','ACT_SIDE_FLIP',10],['side-land','ACT_SIDE_FLIP_LAND_STOP',4],['wall-kick','ACT_WALL_KICK_AIR',8]]) {
  await page.evaluate(({action,ticks})=>{const {pose,A}=window.__SM64_AUDIT__;pose(A[action],ticks);},{action,ticks});
  await page.screenshot({path:`${out}/${name}.png`});
 }
 assert.deepEqual(errors,[]);
 await fs.writeFile(`${out}/results.json`,JSON.stringify(result,null,2));
 console.log('PASS: all 55 implemented actions render finite native poses; run clock, side-flip yaw, landing/pound start frames, wall kick, dive pitch and combo kick verified.');
 console.log(JSON.stringify({run:result.run,side:result.side,land:result.land,pound:result.pound}));
} finally {await browser.close();}
