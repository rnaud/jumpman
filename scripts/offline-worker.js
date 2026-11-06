const prefix = 'jumpman-offline-';
const cacheName = prefix + '__BUILD__';
const urls = /* ASSET_LIST */ [];
const known = new Set(urls);

self.addEventListener('install', event => {
  event.waitUntil((async () => {
    const cache = await caches.open(cacheName);
    try {
      for (let i = 0; i < urls.length; i += 16) {
        const batch = urls.slice(i, i + 16);
        await cache.addAll(batch.map(url => new Request(url, { cache: 'reload' })));
        const percent = Math.round(100 * (i + batch.length) / urls.length);
        for (const client of await self.clients.matchAll({ includeUncontrolled: true })) {
          client.postMessage({ type: 'offline-progress', percent });
        }
      }
    } catch (error) {
      await caches.delete(cacheName);
      throw error;
    }
  })());
});

self.addEventListener('activate', event => {
  event.waitUntil((async () => {
    for (const name of await caches.keys()) {
      if (name.startsWith(prefix) && name !== cacheName) await caches.delete(name);
    }
    await self.clients.claim();
  })());
});

self.addEventListener('fetch', event => {
  const { request } = event;
  const url = new URL(request.url);
  if (request.method !== 'GET' || url.origin !== self.location.origin) return;
  const key = request.mode === 'navigate' && url.pathname === '/' ? '/' : url.pathname;
  if (!known.has(key)) return;
  // Always honor the server's password challenge while connected.
  event.respondWith(fetch(request).catch(async error => {
    const response = await (await caches.open(cacheName)).match(key);
    if (response) return response;
    throw error;
  }));
});
