import { access, readFile } from 'node:fs/promises';
for (const id of ['bob', 'castle_grounds', 'wf', 'pss']) {
  const data = JSON.parse(await readFile(`assets/courses/${id}.json`, 'utf8'));
  await access(`assets/${data.model}`);
  for (const object of data.objects) await access(`assets/${object.model}`);
}
for (const path of ['mario/mario_geo', 'coin/yellow_coin_no_shadow_geo', 'star/star_geo', 'goomba/goomba_geo']) await access(`assets/actors/${path}.glb`);
console.log('All four native courses and required shared actors are available for deployment.');
