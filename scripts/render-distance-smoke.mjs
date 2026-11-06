import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args:['--no-sandbox','--enable-unsafe-swiftshader'],
});
try {
  const page = await browser.newPage();
  await page.goto(new URL('?level=bob',process.env.URL || 'http://127.0.0.1:5173/').href,{waitUntil:'networkidle0'});
  await page.click('#start-button');
  const result = await page.evaluate(() => {
    const g = window.__JUMPMAN__, world=g.playground, camera=g.camera.clone(), renderer=g.renderer;
    const enemy = world.goombas.find(e => e.spawner);
    enemy.step([7000,6000,7000],world.collision);
    world.update(g.time);
    const model=enemy.model, shadow=model.parent.children.find(n => n.name==='shadow-'+model.name &&
      n.position.x===model.position.x && n.position.z===model.position.z);
    if (!shadow?.visible || !model.visible) throw Error('Distant Goomba or its floor shadow is missing');
    const scene=new g.scene.constructor();
    scene.background=g.sun.color.clone().setHex(0xffffff);
    scene.add(g.scene.children.find(n => n.isHemisphereLight).clone(),model.clone(),shadow.clone());
    const actorCopy=scene.children[1], shadowCopy=scene.children[2];
    camera.position.copy(model.position); camera.position.y+=3300;camera.position.z+=3300;
    camera.lookAt(model.position); camera.updateProjectionMatrix();
    const gl=renderer.getContext();
    function pixels() {
      renderer.render(scene,camera);
      const data=new Uint8Array(gl.drawingBufferWidth*gl.drawingBufferHeight*4);
      gl.readPixels(0,0,gl.drawingBufferWidth,gl.drawingBufferHeight,gl.RGBA,gl.UNSIGNED_BYTE,data);
      let count=0;
      for(let i=0;i<data.length;i+=4) if(data[i]<240 || data[i+1]<240 || data[i+2]<240) count++;
      return count;
    }
    shadowCopy.visible=false;
    const actorPixels=pixels();
    actorCopy.visible=false; shadowCopy.visible=true;
    const shadowPixels=pixels();
    // Raycast both faces of one actual Battlefield fence triangle.
    let fence;
    g.courseScene.traverse(n=>{if(n.isMesh && n.material.name==='generic_09008800' && n.material.side===2) fence=n});
    if(!fence) throw Error('Two-sided Battlefield mesh missing');
    const geom=fence.geometry,p=geom.attributes.position,vec=()=>camera.position.clone();
    const [a,b,c]=[0,1,2].map(i=>vec().fromBufferAttribute(p,geom.index?geom.index.getX(i):i).applyMatrix4(fence.matrixWorld));
    const normal=vec().subVectors(b,a).cross(vec().subVectors(c,a)).normalize();
    const center=vec().copy(a).add(b).add(c).multiplyScalar(1/3);
    const ray=g.sideCameraRaycaster;
    const hits=[1,-1].map(side=>{
      ray.set(center.clone().addScaledVector(normal,side*2),normal.clone().multiplyScalar(-side));
      return ray.intersectObject(fence,false).some(hit=>hit.faceIndex===0);
    });
    return {distance:Math.round(camera.position.distanceTo(model.position)),actorPixels,shadowPixels,hits};
  });
  assert(result.actorPixels>0 && result.shadowPixels>0);
  assert.deepEqual(result.hits,[true,true]);
  console.log(result);
} finally {await browser.close()}
