import puppeteer from 'puppeteer-core';
import assert from 'node:assert/strict';
const browser = await puppeteer.launch({
  executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', headless: true,
  args: ['--no-sandbox', '--enable-unsafe-swiftshader', '--use-gl=angle', '--use-angle=swiftshader'],
  defaultViewport: { width: 1280, height: 800 },
});
try {
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.goto((process.env.URL || 'http://127.0.0.1:5174/') + '?level=playground&experiment=kart', { waitUntil: 'networkidle0', timeout: 90000 });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.experiment?.ready, { timeout: 60000 });
  const state = () => page.evaluate(() => window.__JUMPMAN__.debugState);
  const paint = () => page.evaluate(() => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r))));
  const step = n => page.evaluate(n => { for (let i = 0; i < n; i++) window.__JUMPMAN__.stepWorld(); }, n);
  let s = await state();
  assert.equal(s.experiment.mode, 'driving');
  assert(s.experiment.triangles > 3000);
  const start = s.pos;
  await page.keyboard.down('w');
  await step(200);
  await page.keyboard.up('w');
  s = await state();
  assert(Math.hypot(s.pos[0] - start[0], s.pos[2] - start[2]) > 2500, 'kart should accelerate along the road');
  await page.screenshot({ path: '/tmp/mk64-driving.png' });
  await page.click('[data-kart-toggle]');
  await step(2);
  s = await state();
  assert.equal(s.experiment.mode, 'walking');
  const parked = s.experiment.kart;
  const onFoot = s.pos;
  await page.keyboard.down('w');
  await step(40);
  await page.keyboard.up('w');
  s = await state();
  assert(Math.hypot(s.pos[0] - onFoot[0], s.pos[2] - onFoot[2]) > 300, 'Mario should walk');
  assert.deepEqual(s.experiment.kart, parked, 'empty kart stays parked');
  await page.keyboard.press('f');
  await step(1);
  assert.equal((await state()).experiment.mode, 'walking', 'cannot enter remotely');
  await page.evaluate(() => {
    const g = window.__JUMPMAN__, p = g.debugState.experiment.kart;
    g.teleport3(p[0] + 170, p[1], p[2]);
  });
  await page.screenshot({ path: '/tmp/mk64-walking.png' });
  await page.keyboard.press('f');
  await step(2);
  assert.equal((await state()).experiment.mode, 'driving');
  await page.keyboard.press('r');
  await step(1);
  s = await state();
  assert.equal(s.experiment.speed, 0);
  // The kart rests ~0.2 course units (about 1.5 world units) in the floor, like the decomp's soft ground contact.
  assert.deepEqual(s.experiment.kart.map(Math.round), start.map(Math.round));
  await page.keyboard.down('w');
  await page.keyboard.down('d');
  await step(120);
  await page.keyboard.up('d');
  await page.keyboard.up('w');
  assert(Math.abs((await state()).experiment.yaw - Math.PI) > 0.3, 'steering must change heading');
  await page.keyboard.down('s');
  await step(300);
  await page.keyboard.up('s');
  assert((await state()).experiment.reversing, 'braking to a stop then holding back must reverse');
  await page.keyboard.press('r');
  await page.keyboard.press('f');
  await step(2);
  const grounded = (await state()).pos[1];
  await page.keyboard.down('Space');
  await step(5);
  await page.keyboard.up('Space');
  assert((await state()).pos[1] > grounded + 40, 'Mario must jump on foot');
  // Original grass is traversable and must not become SM64 burning surface type 1.
  const grass = await page.evaluate(() => {
    const g = window.__JUMPMAN__, e = g.activeExperiment;
    for (const mesh of e.meshes.filter(m => m.name.includes('Grass'))) {
      const p = mesh.geometry.getAttribute('position');
      for (let i = 0; i < p.count; i += 3) {
        const x = (p.getX(i) + p.getX(i+1) + p.getX(i+2))/3;
        const y = (p.getY(i) + p.getY(i+1) + p.getY(i+2))/3;
        const z = (p.getZ(i) + p.getZ(i+1) + p.getZ(i+2))/3;
        const hit = e.world.findFloor(x, y + 20, z);
        if (hit.surface?.normal[1] > .95 && Math.abs(hit.height-y) < 1) {
          g.teleport3(x, y, z);
          for (let j = 0; j < 10; j++) g.stepWorld();
          return { type: hit.surface.type, health: g.mario.health, action: g.debugState.actionName };
        }
      }
    }
  });
  assert(grass && grass.type !== 1 && !grass.action.includes('BURN'), 'grass should be safe to walk on');
  // Race mode: T starts it, 3-2-1-GO, CPU karts move, items work, results card offers Retry / Free roam.
  await page.keyboard.press('r');
  await page.keyboard.press('t');
  await step(2);
  let race = (await state()).experiment;
  assert.equal(race.race, true, 'T starts a race');
  assert.equal(race.phase, 'countdown');
  await paint();
  assert.equal(await page.$eval('.kart-banner', n => n.textContent), '3');
  const cpuStart = race.cpus;
  await step(60);
  assert.deepEqual((await state()).experiment.cpus, cpuStart, 'CPUs wait for GO');
  await page.keyboard.down('w');
  await step(60);
  race = (await state()).experiment;
  assert.equal(race.phase, 'racing');
  assert.notDeepEqual(race.cpus, cpuStart, 'CPUs drive after GO');
  await paint();
  assert.equal(await page.$eval('.kart-lap', n => n.textContent), 'LAP 1/3');
  await page.screenshot({ path: '/tmp/mk64-race.png' });
  // Item: give a mushroom, press E, boost starts.
  await page.evaluate(() => { window.__JUMPMAN__.activeExperiment.race.race.racers[0].item = 'mushroom'; });
  await page.keyboard.press('e');
  await step(2);
  race = (await state()).experiment;
  assert(race.boost > 0 && race.item === null, 'E uses the held item');
  // Skip ahead by scripting the player's gate progress through all three laps.
  await page.keyboard.up('w');
  await page.evaluate(() => {
    const r = window.__JUMPMAN__.activeExperiment.race.race;
    for (let lap = 0; lap < 4; lap++) for (let g = 0; g < 24; g++) {
      r.raceTick += 5;
      const { c, t } = r.track.gates[g];
      r.racers[0].last = [c[0] - t[0] * 10, c[1] - t[1] * 10];
      r.progressStep(0, [c[0] + t[0] * 10, c[1] + t[1] * 10]);
    }
  });
  await step(2);
  await paint();
  assert.equal((await state()).experiment.phase, 'finished');
  assert(await page.$eval('.kart-result', n => !n.hidden), 'results card is shown');
  await page.screenshot({ path: '/tmp/mk64-results.png' });
  await page.click('[data-kart-retry]');
  await step(2);
  race = (await state()).experiment;
  assert.equal(race.phase, 'countdown', 'Retry restarts the countdown');
  assert.equal(race.gates, 0);
  await page.keyboard.press('n');
  await step(2);
  assert.equal((await state()).experiment.race, false, 'N returns to free roam');
  assert.equal((await state()).experiment.mode, 'driving');
  await page.setViewport({ width: 390, height: 844, isMobile: true, hasTouch: true });
  await page.waitForFunction(() => window.__JUMPMAN__?.debugState.experiment?.ready, { timeout: 60000 });
  await page.keyboard.press('r');
  await page.tap('[data-kart-toggle]');
  await step(2);
  assert.equal((await state()).experiment.mode, 'walking', 'touch button must dismount');
  await page.evaluate(() => window.__JUMPMAN__.stopExperiment());
  assert.equal((await state()).experiment, null);
  assert.deepEqual(errors, []);
  console.log('PASS: native course loads, driving, exit, walking, parked persistence, proximity, re-entry, reset, race mode (countdown, CPUs, items, results, retry), cleanup.');
} finally { await browser.close(); }
