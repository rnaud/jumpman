//! Rust port of `worker/index.ts` (workers-rs). All logic lives in [`core`]; this file adapts it
//! to the Workers runtime: bindings (`SITE_PASSWORD`, `SESSION_SECRET`, `LOGIN_LIMITER`, `ASSETS`).
pub mod core;

use core::{Config, Limiter, Outcome, Req, PROTECT_HEADERS};
use wasm_bindgen::{JsCast, JsValue};
use worker::*;

/// `env.LOGIN_LIMITER.limit({ key })` (Workers Rate Limiting binding; no typed wrapper in workers-rs).
struct JsLimiter(JsValue);

impl Limiter for JsLimiter {
    async fn allow(&self, key: &str) -> bool {
        let call = || -> std::result::Result<js_sys::Promise, JsValue> {
            let limit: js_sys::Function = js_sys::Reflect::get(&self.0, &"limit".into())?.dyn_into()?;
            let options = js_sys::Object::new();
            js_sys::Reflect::set(&options, &"key".into(), &key.into())?;
            limit.call1(&self.0, &options)?.dyn_into()
        };
        // As in the TS, a failing limiter is an error, not an allow; treat it as "deny".
        let Ok(promise) = call() else { return false };
        match wasm_bindgen_futures::JsFuture::from(promise).await {
            Ok(result) => js_sys::Reflect::get(&result, &"success".into()).ok().and_then(|v| v.as_bool()).unwrap_or(false),
            Err(_) => false,
        }
    }
}

fn to_response(r: core::Resp) -> Result<Response> {
    let headers = Headers::new();
    for (k, v) in &r.headers {
        headers.append(k, v)?;
    }
    Ok(Response::builder().with_status(r.status).with_headers(headers).body(ResponseBody::Body(r.body.into_bytes())))
}

fn protect(response: Response) -> Result<Response> {
    let headers = Headers::new();
    for (k, v) in response.headers() {
        if !PROTECT_HEADERS.iter().any(|(p, _)| p.eq_ignore_ascii_case(&k)) {
            headers.append(&k, &v)?;
        }
    }
    for (k, v) in PROTECT_HEADERS {
        headers.set(k, v)?;
    }
    let status = response.status_code();
    let (_, body) = response.into_parts();
    Ok(Response::builder().with_status(status).with_headers(headers).body(body))
}

#[event(fetch)]
async fn fetch(mut request: Request, env: Env, _ctx: Context) -> Result<Response> {
    let secret = |name: &str| env.secret(name).map(|s| s.to_string()).or_else(|_| env.var(name).map(|v| v.to_string())).unwrap_or_default();
    let (site_password, session_secret) = (secret("SITE_PASSWORD"), secret("SESSION_SECRET"));
    let method = request.method().to_string();
    let headers: Vec<(String, String)> = request.headers().into_iter().collect();
    let is_login = method == "POST" && request.url()?.path() == "/__login";
    // Only the login POST has a body we read; the TS caps it at 2 KiB via Content-Length first.
    let body = if is_login && !site_password.is_empty() && !session_secret.is_empty() { request.bytes().await.unwrap_or_default() } else { vec![] };
    let ip = request.headers().get("CF-Connecting-IP")?;
    let core_req = Req { method, url: request.url()?.to_string(), headers, body };
    let limiter = JsLimiter(js_sys::Reflect::get(env.as_ref(), &"LOGIN_LIMITER".into()).unwrap_or(JsValue::UNDEFINED));
    let now = (Date::now().as_millis() / 1000) as u64;
    let cfg = Config { site_password: &site_password, session_secret: &session_secret };
    match core::handle(&core_req, &cfg, &limiter, ip.as_deref(), now).await {
        Outcome::Respond(r) => to_response(r),
        Outcome::Serve => protect(env.assets("ASSETS")?.fetch_request(request).await?),
    }
}
