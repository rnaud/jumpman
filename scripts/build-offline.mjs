// Include both Vite's hashed bundles and lazy-loaded files copied from public/.
import { createHash } from 'node:crypto';
import { readdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

const files = [];
async function collect(dir) {
  for (const entry of await readdir(join('dist', dir), { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) await collect(path);
    else if (path !== 'sw.js') files.push(path);
  }
}
await collect('');
const template = await readFile('scripts/offline-worker.js', 'utf8');
const hash = createHash('sha256').update(template);
const urls = [];
for (const path of files.sort()) {
  hash.update(path).update(createHash('sha256').update(await readFile(join('dist', path))).digest());
  urls.push(path === 'index.html' ? '/' : '/' + path.split('/').map(encodeURIComponent).join('/'));
}
await writeFile('dist/sw.js', template.replace('__BUILD__', hash.digest('hex')).replace('/* ASSET_LIST */ []', JSON.stringify(urls)));
console.log(`Offline build includes ${urls.length} files.`);
