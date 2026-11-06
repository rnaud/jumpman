import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args: ['--no-sandbox', '--enable-unsafe-swiftshader'],
});
try {
  const page = await browser.newPage();
  await page.goto(new URL('?level=bob', process.env.URL || 'http://localhost:5173').href);
  await page.click('#start-button');
  const result = await page.evaluate(() => {
    const game = window.__JUMPMAN__;
    const objects = game.playground.objects;
    const chomp = objects.manager.findByBehavior('chainChomp');
    game.teleport3(chomp.home[0], chomp.home[1], chomp.home[2] + 1800, 0);
    const view = objects.views.get(chomp.id);
    const jaw = view.group.getObjectByName('GEO_ANIMATED_PART_3');
    const poses = new Set();
    const lungePoses = new Set();
    let heldTicks = 0;
    for (let i = 0; i < 110; i++) {
      game.stepSimulation();
      objects.update(i / 30);
      if (!view.group.visible) throw Error('Chain Chomp disappeared within activation range');
      const pose = jaw.quaternion.toArray().join(',');
      poses.add(pose);
      if (chomp.vars.subAction === 1 && chomp.vars.timer < 30) {
        heldTicks++;
        lungePoses.add(pose);
      }
    }
    const links = chomp.links.slice(1).map(link => objects.views.get(link.id).group);
    for (const yaw of [0, Math.PI / 2, Math.PI]) {
      game.camera.rotation.set(0, yaw, 0);
      game.playground.faceCamera(game.camera);
      for (const link of links) {
        const rotation = link.getWorldQuaternion(link.quaternion.clone());
        if (rotation.angleTo(game.camera.quaternion) > 0.00001) throw Error('Chain sprite turned edge-on');
      }
    }
    return { poses: poses.size, lungePoses: lungePoses.size, heldTicks,
      billboards: links.filter(link => link.userData.objectBillboard && link.visible).length };
  });
  if (process.env.SCREENSHOT) {
    const image = await page.evaluate(() => {
      const g = window.__JUMPMAN__;
      const chomp = g.playground.objects.manager.findByBehavior('chainChomp');
      const [x,y,z] = chomp.pos;
      g.camera.position.set(x + 760, y + 420, z + 920);
      g.camera.lookAt(x, y + 240, z);
      g.playground.faceCamera(g.camera);
      g.renderer.render(g.scene, g.camera);
      return g.renderer.domElement.toDataURL();
    });
    const { writeFile } = await import('node:fs/promises');
    await writeFile(process.env.SCREENSHOT, Buffer.from(image.split(',')[1], 'base64'));
  }
  console.log(result);
  assert(result.poses > 10 && result.lungePoses === 1 && result.heldTicks >= 20 && result.billboards === 4);
} finally {
  await browser.close();
}
