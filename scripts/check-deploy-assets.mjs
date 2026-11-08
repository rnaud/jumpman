import { access, readFile } from 'node:fs/promises';
for (const id of ['bob', 'castle_grounds', 'castle_inside', 'castle_inside.2', 'castle_inside.3', 'wf', 'bbh', 'ccm', 'jrb', 'jrb.2', 'lll', 'ssl', 'wdw', 'sl', 'ttm', 'thi', 'ttc', 'rr', 'bitdw', 'bitfs', 'bits', 'bowser_1', 'bowser_2', 'bowser_3', 'pss', 'sa', 'hmc', 'ddd', 'ddd.2']) {
  const data = JSON.parse(await readFile(`assets/courses/${id}.json`, 'utf8'));
  if (id.startsWith('castle_') && (data.doors?.length ?? 0) < 2) throw new Error(`Re-export the native doors for ${id}`);
  if (id === 'castle_grounds' && !data.waterBoxes?.length) throw new Error('Re-export castle water regions');
  await access(`assets/${data.model}`);
  for (const room of data.roomModels ?? []) await access(`assets/${room}`);
  for (const painting of data.paintingArt?.layouts ?? []) {
    for (const part of painting.parts) await access(`assets/extracted/levels/castle_inside/${part.texture}`);
  }
  for (const object of data.objects) {
    await access(`assets/${object.model}`);
    const bytes = await readFile(`assets/${object.model}`);
    const jsonSize = bytes.readUInt32LE(12);
    const doc = JSON.parse(bytes.subarray(20, 20 + jsonSize).toString());
    if (!doc.meshes?.length) throw new Error(`Meshless course object: ${object.model}`);
  }
}
for (const path of ['mario/mario_geo', 'coin/yellow_coin_no_shadow_geo', 'star/star_geo', 'goomba/goomba_geo']) await access(`assets/actors/${path}.glb`);
await access('assets/actors/bowser/bowser_geo_no_shadow.glb');
await access('assets/actors/bowser_flame/bowser_flames_geo.glb');
await access('assets/actors/treasure_chest/treasure_chest_base_geo.glb');
for (const path of ['actors/warp_pipe/warp_pipe_geo.glb', 'actors/poundable_pole/wooden_post_geo.glb']) await access(`assets/${path}`);
await access('assets/audio/manifest.json');
await access('assets/audio/music/sm64-us.wasm');
await access('assets/audio/music/courses.json');
for (const path of Object.keys(JSON.parse(await readFile('assets/audio/manifest.json', 'utf8')))) await access(`assets/audio/${path}`);
console.log('Verified native courses, castle areas, shared actors and audio.');

// The kart menu is always exposed; do not deploy a broken entry after a fresh checkout.
const raceway = JSON.parse(await readFile('public/mk64/luigi-raceway.json', 'utf8'));
await access('public/mk64/mario-kart.png');
for (const item of ['mushroom', 'banana', 'shell', 'question']) await access(`public/mk64/item-${item}.png`);
await access('assets/audio/kart-voices.json');
for (const texture of Object.values(raceway.textures)) await access(`public/mk64/${texture.file}`);
console.log('Verified Mario Kart 64 Raceway and kart assets.');
