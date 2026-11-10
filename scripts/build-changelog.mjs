#!/usr/bin/env node
// Builds a static changelog page (one entry per commit on main) from git history.
// Usage: node scripts/build-changelog.mjs [outDir]   (default: changelog-site)
import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync, readFileSync, existsSync, copyFileSync } from 'node:fs';
import path from 'node:path';

const outDir = process.argv[2] ?? 'changelog-site';
const repoUrl = (process.env.REPO_URL ?? 'https://github.com/rnaud/jumpman').replace(/\/$/, '');
const SEP = '\x1e', FIELD = '\x1f';

const raw = execFileSync(
  'git',
  ['log', process.env.REF ?? 'main', '--no-merges', `--format=%H${FIELD}%an${FIELD}%aI${FIELD}%s${FIELD}%b${SEP}`],
  { encoding: 'utf8', maxBuffer: 256 * 1024 * 1024 },
);

const esc = (s) => s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);

const commits = raw.split(SEP).map((r) => r.trim()).filter(Boolean).map((r) => {
  const [hash, author, date, subject, body = ''] = r.split(FIELD);
  const cleanBody = body.split('\n').filter((l) => !/^Co-Authored-By:/i.test(l)).join('\n').trim();
  return { hash, author, date, subject, body: cleanBody };
});

// Screenshots: images added/changed in a commit under changelog/ are attached to it.
// `<name>-before.<ext>` + `<name>-after.<ext>` become a before/after slider; others show as plain images.
const IMG = /^changelog\/.+\.(png|jpe?g|webp|gif)$/i;
for (const c of commits) {
  const files = execFileSync('git', ['show', '--name-only', '--diff-filter=AM', '--format=', c.hash], { encoding: 'utf8' })
    .split('\n').filter((f) => IMG.test(f));
  const saved = new Map();
  for (const f of files) {
    const rel = `img/${c.hash.slice(0, 8)}/${path.basename(f)}`;
    mkdirSync(`${outDir}/${path.dirname(rel)}`, { recursive: true });
    writeFileSync(`${outDir}/${rel}`, execFileSync('git', ['show', `${c.hash}:${f}`], { maxBuffer: 64 * 1024 * 1024 }));
    saved.set(path.basename(f).replace(/\.[^.]+$/, ''), rel);
  }
  c.pairs = []; c.singles = [];
  for (const [stem, rel] of saved) {
    const m = stem.match(/^(.*)-before$/);
    if (m && saved.has(`${m[1]}-after`)) c.pairs.push({ name: m[1], before: rel, after: saved.get(`${m[1]}-after`) });
    else if (!/-after$/.test(stem) || !saved.has(stem.replace(/-after$/, '-before'))) c.singles.push({ name: stem, src: rel });
  }
}

// Backfilled screenshots: changelog/shots.json maps a commit hash (prefix) to [{name, before, after}] with
// paths relative to the repo root (read from the working tree, so old commits can get images later).
if (existsSync('changelog/shots.json')) {
  const manifest = JSON.parse(readFileSync('changelog/shots.json', 'utf8'));
  for (const c of commits) {
    const key = Object.keys(manifest).find((k) => c.hash.startsWith(k));
    for (const p of key ? manifest[key] : []) {
      const [b, a] = [p.before, p.after].map((f) => {
        const rel = `img/${c.hash.slice(0, 8)}/${path.basename(f)}`;
        mkdirSync(`${outDir}/${path.dirname(rel)}`, { recursive: true });
        copyFileSync(f, `${outDir}/${rel}`);
        return rel;
      });
      c.pairs.push({ name: p.name, before: b, after: a });
    }
  }
}

const media = (c) => [
  ...c.pairs.map((p) => `<figure class="cmp" style="--pos:50%">
    <img src="${p.after}" alt="After: ${esc(p.name)}" loading="lazy">
    <img class="before" src="${p.before}" alt="Before: ${esc(p.name)}" loading="lazy">
    <span class="tag b">Before</span><span class="tag a">After</span>
    <input type="range" min="0" max="100" value="50" aria-label="Before/after slider: ${esc(p.name)}">
  </figure>`),
  ...c.singles.map((s) => `<figure><img src="${s.src}" alt="${esc(s.name)}" loading="lazy"></figure>`),
].join('');

const byDay = new Map();
for (const c of commits) {
  const day = c.date.slice(0, 10);
  if (!byDay.has(day)) byDay.set(day, []);
  byDay.get(day).push(c);
}

const days = [...byDay].map(([day, list]) => `
  <section class="day"><h2>${day} <span>${list.length} change${list.length === 1 ? '' : 's'}</span></h2>
  ${list.map((c) => `
    <article>
      <h3>${esc(c.subject)}</h3>
      ${media(c)}
      ${c.body ? `<details><summary>Details</summary><pre>${esc(c.body)}</pre></details>` : ''}
      <p class="meta"><time datetime="${c.date}">${c.date.slice(11, 16)} UTC${c.date.slice(19) === 'Z' ? '' : ' ' + c.date.slice(19)}</time> ·
      <a href="${repoUrl}/commit/${c.hash}"><code>${c.hash.slice(0, 8)}</code></a></p>
    </article>`).join('')}
  </section>`).join('');

const html = `<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Jumpman Changelog</title>
<style>
:root{--bg:#fff;--fg:#1c1c1e;--mut:#6b6b70;--line:#e3e3e6;--acc:#d62828}
@media(prefers-color-scheme:dark){:root{--bg:#141416;--fg:#ececee;--mut:#9a9aa0;--line:#2c2c30;--acc:#ff6b6b}}
body{background:var(--bg);color:var(--fg);font:16px/1.5 system-ui,sans-serif;margin:0}
main{max-width:760px;margin:0 auto;padding:32px 16px 64px}
h1{margin:0 0 4px}.sub{color:var(--mut);margin:0 0 24px}
#q{width:100%;box-sizing:border-box;padding:10px 12px;font:inherit;border:1px solid var(--line);border-radius:8px;background:var(--bg);color:var(--fg);margin-bottom:24px}
.day h2{font-size:1.1rem;border-bottom:1px solid var(--line);padding-bottom:4px;margin-top:32px}
.day h2 span{color:var(--mut);font-weight:400;font-size:.85rem;margin-left:8px}
article{padding:10px 0}article h3{font-size:1rem;margin:0}
.meta{color:var(--mut);font-size:.8rem;margin:4px 0 0}a{color:var(--acc)}
details{margin-top:4px}summary{cursor:pointer;color:var(--mut);font-size:.85rem}
figure{margin:8px 0;position:relative;line-height:0}figure img{width:100%;border-radius:8px;border:1px solid var(--line)}
.cmp{overflow:hidden;border-radius:8px}.cmp img{display:block;border-radius:0}
.cmp .before{position:absolute;inset:0;height:100%;object-fit:cover;clip-path:inset(0 calc(100% - var(--pos)) 0 0)}
.cmp::after{content:"";position:absolute;top:0;bottom:0;left:var(--pos);width:2px;background:#fff;box-shadow:0 0 4px #0008;pointer-events:none}
.cmp input{position:absolute;inset:0;width:100%;height:100%;margin:0;opacity:0;cursor:ew-resize}
.tag{position:absolute;top:8px;font:600 11px/1 system-ui;background:#000a;color:#fff;padding:4px 6px;border-radius:4px}.tag.b{left:8px}.tag.a{right:8px}
pre{white-space:pre-wrap;font:inherit;font-size:.9rem;margin:6px 0 0}
</style></head><body><main>
<h1>Jumpman Changelog</h1>
<p class="sub">${commits.length} commits · built ${new Date().toISOString().slice(0, 16).replace('T', ' ')} UTC · <a href="${repoUrl}">GitHub</a></p>
<input id="q" type="search" placeholder="Filter changes…" aria-label="Filter changes">
${days}
</main>
<script>
const q=document.getElementById('q');
document.querySelectorAll('.cmp input').forEach(i=>i.addEventListener('input',()=>i.parentNode.style.setProperty('--pos',i.value+'%')));
q.addEventListener('input',()=>{const t=q.value.toLowerCase();
document.querySelectorAll('article').forEach(a=>a.hidden=!a.textContent.toLowerCase().includes(t));
document.querySelectorAll('.day').forEach(d=>d.hidden=![...d.querySelectorAll('article')].some(a=>!a.hidden));});
</script></body></html>`;

mkdirSync(outDir, { recursive: true });
writeFileSync(`${outDir}/index.html`, html);
console.log(`Wrote ${outDir}/index.html with ${commits.length} commits across ${byDay.size} days`);
