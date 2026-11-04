import { describe, it, expect, vi } from 'vitest';
import worker from '../worker/index';
const origin = 'https://jumpman.example';
function setup() {
  return { SITE_PASSWORD: 'test-password', SESSION_SECRET: 'test-only-secret-0123456789', ASSETS: { fetch: vi.fn(async () => new Response('asset')) }, LOGIN_LIMITER: { limit: vi.fn(async () => ({ success: true })) } };
}
function submit(password: string, next = '/?level=pss') {
  return new Request(`${origin}/__login?next=${encodeURIComponent(next)}`, { method: 'POST', headers: { Origin: origin }, body: new URLSearchParams({ password }) });
}
describe('Worker password gate', () => {
  it('protects HTML and direct asset links before serving any bytes', async () => {
    const env = setup();
    for (const path of ['/', '/assets/mario.glb', '/models/quaternius/AnimationLibrary.glb']) {
      expect((await worker.fetch(new Request(origin + path), env)).status).toBe(401);
    }
    expect(env.ASSETS.fetch).not.toHaveBeenCalled();
    expect((await worker.fetch(submit('wrong'), env)).status).toBe(401);
    expect((await worker.fetch(new Request(origin), { ...env, SITE_PASSWORD: '' })).status).toBe(503);
  });
  it('issues secure signed sessions, preserves the selected course and rejects tampering', async () => {
    const env = setup();
    const login = await worker.fetch(submit(env.SITE_PASSWORD), env);
    expect(login.status).toBe(303); expect(login.headers.get('Location')).toBe('/?level=pss');
    const cookie = login.headers.get('Set-Cookie')!;
    expect(cookie).toContain('HttpOnly; Secure; SameSite=Strict');
    const request = (value: string) => new Request(origin + '/assets/mario.glb', { headers: { Cookie: value } });
    expect(await (await worker.fetch(request(cookie), env)).text()).toBe('asset');
    expect((await worker.fetch(request(cookie.replace(/=\d/, '=0')), env)).status).toBe(401);
    expect((await worker.fetch(request(cookie), { ...env, SITE_PASSWORD: 'changed' })).status).toBe(401);
  });
  it('rejects expired sessions, cross-site login, redirects off-site and excess attempts', async () => {
    const env = setup();
    const login = await worker.fetch(submit(env.SITE_PASSWORD, '//evil.example'), env);
    expect(login.headers.get('Location')).toBe('/');
    vi.useFakeTimers();
    try {
      vi.setSystemTime(Date.now() + 86401000);
      expect((await worker.fetch(new Request(origin, { headers: { Cookie: login.headers.get('Set-Cookie')! } }), env)).status).toBe(401);
    } finally { vi.useRealTimers(); }
    const foreign = submit(env.SITE_PASSWORD); foreign.headers.set('Origin', 'https://evil.example');
    expect((await worker.fetch(foreign, env)).status).toBe(403);
    env.LOGIN_LIMITER.limit.mockResolvedValue({ success: false });
    expect((await worker.fetch(submit(env.SITE_PASSWORD), env)).status).toBe(429);
  });
});
