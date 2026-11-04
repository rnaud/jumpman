interface Env {
  ASSETS: { fetch(request: Request): Promise<Response> };
  SITE_PASSWORD: string;
  SESSION_SECRET: string;
  LOGIN_LIMITER: { limit(options: { key: string }): Promise<{ success: boolean }> };
}
const COOKIE = '__Host-jumpman';
const MAX_AGE = 86400;
const encode = (text: string) => new TextEncoder().encode(text);
const hex = (bytes: ArrayBuffer) => Array.from(new Uint8Array(bytes), b => b.toString(16).padStart(2, '0')).join('');
async function signature(value: string, secret: string): Promise<string> {
  const key = await crypto.subtle.importKey('raw', encode(secret), { name: 'HMAC', hash: 'SHA-256' }, false, ['sign']);
  return hex(await crypto.subtle.sign('HMAC', key, encode(value)));
}
async function equal(a: string, b: string): Promise<boolean> {
  const [ah, bh] = await Promise.all([a, b].map(v => crypto.subtle.digest('SHA-256', encode(v))));
  const aa = new Uint8Array(ah), bb = new Uint8Array(bh);
  let difference = 0;
  for (let i = 0; i < aa.length; i++) difference |= aa[i] ^ bb[i];
  return difference === 0;
}
function protect(response: Response): Response {
  const result = new Response(response.body, response);
  result.headers.set('Cache-Control', 'private, no-store');
  result.headers.set('X-Content-Type-Options', 'nosniff');
  result.headers.set('Referrer-Policy', 'same-origin');
  result.headers.set('X-Frame-Options', 'DENY');
  return result;
}
const escape = (s: string) => s.replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);
function login(next: string, message = '', status = 401): Response {
  return protect(new Response(`<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Jumpman · Enter password</title><style>body{margin:0;min-height:100dvh;display:grid;place-items:center;background:#101d27;color:#fff4d6;font:16px system-ui}main{width:min(340px,80vw);padding:36px;border:1px solid #46545d;border-radius:20px;background:#182938}h1{font-size:42px;margin:8px 0;color:#ffd24a}label{display:block;margin:24px 0 8px}input,button{box-sizing:border-box;width:100%;padding:14px;border-radius:8px;font:inherit}input{background:#101d27;color:white;border:1px solid #78828a}button{margin-top:16px;background:#ffd24a;border:0;font-weight:bold;cursor:pointer}p{line-height:1.5;color:#c4c9cc}.error{color:#ffb4a8}</style><main><small>ONE MORE STAR</small><h1>JUMPMAN</h1><p>A little corner of the Mushroom Kingdom.</p><form action="/__login?next=${escape(encodeURIComponent(next))}" method="post"><label for="password">Password</label><input id="password" name="password" type="password" autocomplete="current-password" required autofocus maxlength="256"><button>LET’S GO</button><p class="error" role="alert">${escape(message)}</p></form></main></html>`, { status, headers: { 'Content-Type': 'text/html; charset=utf-8', 'Content-Security-Policy': "default-src 'none'; style-src 'unsafe-inline'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'" } }));
}
export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    if (!env.SITE_PASSWORD || !env.SESSION_SECRET) return protect(new Response('Site access is not configured.', { status: 503 }));
    const url = new URL(request.url);
    if (url.pathname === '/__login' && request.method === 'POST') {
      if (request.headers.get('Origin') !== url.origin) return protect(new Response('Invalid origin.', { status: 403 }));
      const allowed = await env.LOGIN_LIMITER.limit({ key: request.headers.get('CF-Connecting-IP') ?? 'unknown' });
      if (!allowed.success) return login('/', 'Too many attempts. Please wait a minute.', 429);
      if (Number(request.headers.get('Content-Length')) > 2048) return protect(new Response('Request too large.', { status: 413 }));
      let password: FormDataEntryValue | null;
      try { password = (await request.formData()).get('password'); } catch { return login('/', 'Please enter the password.'); }
      const next = url.searchParams.get('next') ?? '/';
      const destination = next.startsWith('/') && !next.startsWith('//') && !/[\\\r\n]/.test(next) ? next : '/';
      if (typeof password !== 'string' || password.length > 256 || !await equal(password, env.SITE_PASSWORD)) return login(destination, 'That password didn’t match. Try again.');
      const expires = String(Math.floor(Date.now() / 1000) + MAX_AGE);
      // Bind sessions to the current password, so changing it also invalidates cookies.
      const token = `${expires}.${await signature(`${expires}:${env.SITE_PASSWORD}`, env.SESSION_SECRET)}`;
      return protect(new Response(null, { status: 303, headers: { Location: destination, 'Set-Cookie': `${COOKIE}=${token}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=${MAX_AGE}` } }));
    }
    const token = request.headers.get('Cookie')?.split(';').map(v => v.trim()).find(v => v.startsWith(`${COOKIE}=`))?.slice(COOKIE.length + 1) ?? '';
    const match = /^(\d{10})\.([a-f0-9]{64})$/.exec(token);
    const now = Math.floor(Date.now() / 1000);
    if (match && Number(match[1]) > now && Number(match[1]) <= now + MAX_AGE && await equal(match[2], await signature(`${match[1]}:${env.SITE_PASSWORD}`, env.SESSION_SECRET))) {
      return protect(await env.ASSETS.fetch(request));
    }
    return login(url.pathname + url.search);
  },
};
