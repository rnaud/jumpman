import { access, readFile } from 'node:fs/promises';
for (const id of ['bob', 'castle_grounds', 'castle_inside', 'wf', 'bbh', 'ccm', 'jrb', 'lll', 'ssl', 'wdw', 'sl', 'ttm', 'thi', 'ttc', 'rr', 'bitdw', 'bitfs', 'bits', 'pss']) {
  const data = JSON.parse(await readFile(`assets/courses/${id}.json`, 'utf8'));
  if (id.startsWith('castle_') && data.doors?.length !== 2) throw new Error(`Re-export the native doors for ${id}`);
  if (id === 'castle_grounds' && !data.waterBoxes?.length) throw new Error('Re-export castle water regions');
  await access(`assets/${data.model}`);
  for (const object of data.objects) await access(`assets/${object.model}`);
}
for (const path of ['mario/mario_geo', 'coin/yellow_coin_no_shadow_geo', 'star/star_geo', 'goomba/goomba_geo']) await access(`assets/actors/${path}.glb`);
await access('assets/audio/manifest.json');
for (const path of Object.keys(JSON.parse(await readFile('assets/audio/manifest.json', 'utf8')))) await access(`assets/audio/${path}`);
console.log('All nineteen native courses and required shared actors and converted audio are available for deployment.');
