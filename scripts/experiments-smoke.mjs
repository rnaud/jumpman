// Browser smoke for the five experiments: each starts from the menu or URL,
// produces its signature interaction, and leaves the game clean on exit.
import puppeteer from 'puppeteer-core';
import assert from 'node:assert/strict';

const base = process.env.URL || 'http://127.0.0.1:5173/';
const browser = await puppeteer.launch({
  executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  headless: true,
  args: ['--no-sandbox', '--enable-unsafe-swiftshader', '--use-gl=angle', '--use-angle=swiftshader'],
  defaultViewport: { width: 1280, height: 800 },
});
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
try {
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
  const state = () => page.evaluate(() => window.__JUMPMAN__.debugState);
  const open = async query => {
    await page.goto(new URL(query, base).href, { waitUntil: 'networkidle0', timeout: 90000 });
    await page.waitForFunction(() => window.__JUMPMAN__?.debugState.experiment && !window.__JUMPMAN__.debugState.experiment.starting, { timeout: 60000 });
  };
  // Advance the 30 Hz simulation synchronously (headless rAF is throttled).
  const step = frames => page.evaluate(n => { for (let i = 0; i < n; i++) window.__JUMPMAN__.stepWorld(); }, frames);

  // Menu button → experiment, URL persistence, badge.
  await page.goto(new URL('?level=bob', base).href, { waitUntil: 'networkidle0', timeout: 90000 });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.world === 'bob', { timeout: 60000 });
  await page.click('#start-overlay [data-experiment="god"]');
  await page.waitForFunction(() => window.__JUMPMAN__.debugState.experiment?.id === 'god' && !window.__JUMPMAN__.debugState.experiment.starting, { timeout: 30000 });
  assert.equal(new URL(page.url()).searchParams.get('experiment'), 'god');
  assert.equal(await page.$eval('#experiment-badge', node => node.hidden), false);

  // God: starter blocks; a bridge painted under falling Mario catches him; springs launch.
  let snap = await state();
  assert(snap.experiment.pieces.length >= 4, 'god starter platforms missing');
  const caught = await page.evaluate(() => {
    const g = window.__JUMPMAN__, e = g.activeExperiment, m = g.mario;
    g.teleport3(m.pos[0], 1500, m.pos[2]);
    m.setAction(0x0100088C, 0);
    g.stepWorld();
    e.setTool('bridge');
    const r = g.canvas.getBoundingClientRect();
    const V = g.camera.position.constructor;
    const at = (x, y, z) => { const v = new V(x, y, z).project(g.camera); return { clientX: r.left + (v.x * .5 + .5) * r.width, clientY: r.top + (-v.y * .5 + .5) * r.height, button: 0 }; };
    const y = m.pos[1] - 40;
    e.pointerDown(at(m.pos[0] - 300, y, m.pos[2]));
    for (let i = -300; i <= 300; i += 60) e.pointerMove(at(m.pos[0] + i, y, m.pos[2]));
    e.pointerUp();
    const planks = [...e.pieces.values()].filter(p => p.kind === 'plank');
    for (let i = 0; i < 60; i++) { g.stepWorld(); if (!(m.action & 0x800)) return planks.some(p => p.surfaces.has(m.floor)); }
    return false;
  });
  assert(caught, 'painted bridge did not catch falling Mario');

  // Hundred: a full crowd of full-physics Marios with every personality; racers progress.
  await open('?level=bob&experiment=hundred');
  await page.waitForFunction(() => window.__JUMPMAN__.debugState.experiment.count === 100 && !window.__JUMPMAN__.debugState.experiment.loading, { timeout: 90000 });
  snap = await state();
  assert(Object.values(snap.experiment.counts).every(count => count > 0), 'a personality is missing');
  const progress = await page.evaluate(() => {
    const crowd = window.__JUMPMAN__.activeExperiment.crowd;
    for (let i = 0; i < 1200; i++) crowd.stepOnce();
    return Math.max(...crowd.agents.filter(a => a.personality === 'racer').map(a => a.waypoint));
  });
  assert(progress >= 6, `racers barely moved (best waypoint ${progress})`);
  await page.evaluate(() => window.__JUMPMAN__.inspectAgent(1));
  await sleep(300);
  assert.equal(await page.$eval('.xp-inspector', node => node.hidden), false, 'inspector did not open');

  // Dimensions: side view locks Mario to the lane and clips the world; top-down slices above.
  await open('?level=bob&experiment=dimensions');
  const lane = await page.evaluate(() => {
    const g = window.__JUMPMAN__, e = g.activeExperiment;
    e.setMode('side');
    const p0 = [...g.mario.pos];
    window.dispatchEvent(new KeyboardEvent('keydown', { code: 'KeyD' }));
    for (let i = 0; i < 40; i++) g.stepWorld();
    window.dispatchEvent(new KeyboardEvent('keyup', { code: 'KeyD' }));
    const yaw = e.laneYaw * Math.PI / 32768;
    const dx = g.mario.pos[0] - p0[0], dz = g.mario.pos[2] - p0[2];
    return { along: dx * Math.sin(yaw) + dz * Math.cos(yaw), across: -dx * Math.cos(yaw) + dz * Math.sin(yaw) };
  });
  assert(lane.along > 200, 'Mario did not run along the 2D lane');
  assert(Math.abs(lane.across) < 1, 'Mario drifted off the 2D lane');
  await page.keyboard.press('Digit3');
  await sleep(1200);
  snap = await state();
  assert.equal(snap.experiment.mode, 'top');
  assert.equal(snap.experiment.clipping, 1, 'top-down did not slice the world');

  // 2.5D path: Mario follows the race route up the mountain, side-on.
  const path = await page.evaluate(() => {
    const g = window.__JUMPMAN__, e = g.activeExperiment;
    e.setMode('path');
    const s0 = e.debug().rail.s;
    window.dispatchEvent(new KeyboardEvent('keydown', { code: 'KeyD' }));
    for (let i = 0; i < 90; i++) g.stepWorld();
    window.dispatchEvent(new KeyboardEvent('keyup', { code: 'KeyD' }));
    const hit = e.rail.nearest(g.mario.pos);
    return { s0, s1: e.debug().rail.s, off: Math.hypot(hit.point[0] - g.mario.pos[0], hit.point[2] - g.mario.pos[2]) };
  });
  assert(path.s1 - path.s0 > 1500, `Mario did not advance along the 2.5D path (${path.s0} → ${path.s1})`);
  assert(path.off < 1, 'Mario left the 2.5D path');

  // Endless Run: auto-run collects coins; a fall ends the run and Space restarts it.
  await open('?level=bob&experiment=runner&runSeed=5');
  const run = await page.evaluate(() => {
    const g = window.__JUMPMAN__;
    for (let i = 0; i < 120; i++) g.stepWorld();
    const running = g.debugState.experiment;
    g.mario.pos[2] = 4000;
    for (let i = 0; i < 300 && g.debugState.experiment.state !== 'over'; i++) g.stepWorld();
    return { running, over: g.debugState.experiment.state };
  });
  assert.equal(run.running.state, 'running');
  assert(run.running.distance > 20 && run.running.coins > 0, 'runner did not run or collect coins');
  assert.equal(run.over, 'over', 'falling did not end the run');
  await page.keyboard.press('Space');
  snap = await state();
  assert.equal(snap.experiment.state, 'ready', 'Space did not restart the run');

  // Pause menu: swap character mid-level, then return to the title screen.
  await page.evaluate(() => document.getElementById('settings-button').click());
  await page.evaluate(() => document.querySelector('[data-character="link"]').click());
  await page.waitForFunction(() => window.__JUMPMAN__.debugState.character === 'link', { timeout: 60000 });
  await page.evaluate(() => document.getElementById('main-menu-button').click());
  snap = await state();
  assert.equal(snap.started, false);
  assert.equal(snap.experiment, null, 'main menu did not leave the experiment');
  assert.equal(await page.$eval('#start-overlay', node => node.hidden), false, 'title screen not shown');

  // Diorama: frozen player, real crowd and goombas, pokes, then a clean exit.
  await open('?level=bob&experiment=diorama');
  await page.waitForFunction(() => window.__JUMPMAN__.debugState.experiment.ready, { timeout: 90000 });
  snap = await state();
  assert(snap.experiment.marios >= 20 && snap.experiment.goombas > 0, 'diorama cast missing');
  assert(await page.evaluate(() => window.__JUMPMAN__.activeExperiment.pokeMario(0)), 'poke failed');
  await page.evaluate(() => window.__JUMPMAN__.stopExperiment());
  snap = await state();
  assert.equal(snap.experiment, null, 'experiment did not stop');
  assert.equal(new URL(page.url()).searchParams.get('experiment'), null);

  assert.deepEqual(errors, []);
  console.log('PASS: god bridge catch, 100-Mario crowd, 2D + top-down + 2.5D path, endless run, character swap + main menu, diorama, clean exit');
} finally {
  await browser.close();
}
