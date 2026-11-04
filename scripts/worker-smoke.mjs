import assert from 'node:assert/strict';
import { readdir } from 'node:fs/promises';
assert(process.env.SITE_PASSWORD, 'Set SITE_PASSWORD to test the deployed password gate.');
const base=process.env.URL||'http://127.0.0.1:8787';
const asset='/assets/'+(await readdir('dist/assets')).find(p=>p.startsWith('mario_geo-')&&p.endsWith('.glb'));
for(const path of ['/',asset]) {
 const r=await fetch(base+path);assert.equal(r.status,401);assert.match(r.headers.get('Cache-Control'),/no-store/);
 console.log('Protected',path,r.status);
}
const login=await fetch(base+'/__login?next=%2F%3Flevel%3Dpss',{method:'POST',headers:{Origin:base},body:new URLSearchParams({password:process.env.SITE_PASSWORD}),redirect:'manual'});
assert.equal(login.status,303);assert.equal(login.headers.get('Location'),'/?level=pss');
const cookie=login.headers.get('Set-Cookie').split(';')[0];
for(const path of ['/',asset]) {
 const r=await fetch(base+path,{headers:{Cookie:cookie}});assert.equal(r.status,200);
 assert(!r.headers.get('Content-Type').includes('text/html')||path==='/');
 console.log('Unlocked',path,r.status);
}
console.log('PASS: password login and asset gate.');
