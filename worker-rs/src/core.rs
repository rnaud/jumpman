//! Engine-independent port of `worker/index.ts`: the password gate. Pure functions over plain
//! request/response structs so it is unit-testable natively; `lib.rs` adapts it to workers-rs.
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

pub const COOKIE: &str = "__Host-jumpman";
pub const MAX_AGE: u64 = 86400;

pub struct Config<'a> {
    pub site_password: &'a str,
    pub session_secret: &'a str,
}

#[derive(Clone, Debug, Default)]
pub struct Req {
    pub method: String,
    pub url: String,
    /// Header names are matched case-insensitively.
    pub headers: Vec<(String, String)>,
    /// Request body (only read for the login POST).
    pub body: Vec<u8>,
}

impl Req {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

#[derive(Clone, Debug, Default)]
pub struct Resp {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Resp {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

/// What the worker should do with a request.
#[derive(Debug)]
pub enum Outcome {
    /// Answer directly (already carries the `protect` headers).
    Respond(Resp),
    /// Authenticated: forward to the static assets, then add [`PROTECT_HEADERS`] to the response.
    Serve,
}

/// Rate limiter (`LOGIN_LIMITER.limit({ key })`).
pub trait Limiter {
    fn allow(&self, key: &str) -> impl std::future::Future<Output = bool>;
}

/// Headers `protect()` sets on every response.
pub const PROTECT_HEADERS: [(&str, &str); 4] = [("Cache-Control", "private, no-store"), ("X-Content-Type-Options", "nosniff"), ("Referrer-Policy", "same-origin"), ("X-Frame-Options", "DENY")];

fn protect(mut r: Resp) -> Resp {
    for (k, v) in PROTECT_HEADERS {
        r.headers.retain(|(h, _)| !h.eq_ignore_ascii_case(k));
        r.headers.push((k.into(), v.into()));
    }
    r
}

fn plain(status: u16, text: &str) -> Outcome {
    Outcome::Respond(protect(Resp { status, headers: vec![("Content-Type".into(), "text/plain;charset=UTF-8".into())], body: text.into() }))
}

pub fn signature(value: &str, secret: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("hmac takes any key length");
    mac.update(value.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Constant-time comparison of SHA-256 digests (`equal`).
pub fn equal(a: &str, b: &str) -> bool {
    let (ah, bh) = (Sha256::digest(a.as_bytes()), Sha256::digest(b.as_bytes()));
    ah.iter().zip(bh.iter()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// JavaScript `encodeURIComponent`.
pub fn encode_uri_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn login(next: &str, message: &str, status: u16) -> Resp {
    let html = format!(
        concat!(
            "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Jumpman · Enter password</title>",
            "<style>body{{margin:0;min-height:100dvh;display:grid;place-items:center;background:#101d27;color:#fff4d6;font:16px system-ui}}main{{width:min(340px,80vw);padding:36px;border:1px solid #46545d;border-radius:20px;background:#182938}}h1{{font-size:42px;margin:8px 0;color:#ffd24a}}label{{display:block;margin:24px 0 8px}}input,button{{box-sizing:border-box;width:100%;padding:14px;border-radius:8px;font:inherit}}input{{background:#101d27;color:white;border:1px solid #78828a}}button{{margin-top:16px;background:#ffd24a;border:0;font-weight:bold;cursor:pointer}}p{{line-height:1.5;color:#c4c9cc}}.error{{color:#ffb4a8}}</style>",
            "<main><small>ONE MORE STAR</small><h1>JUMPMAN</h1><p>A little corner of the Mushroom Kingdom.</p>",
            "<form action=\"/__login?next={next}\" method=\"post\"><label for=\"password\">Password</label>",
            "<input id=\"password\" name=\"password\" type=\"password\" autocomplete=\"current-password\" required autofocus maxlength=\"256\"><button>LET’S GO</button>",
            "<p class=\"error\" role=\"alert\">{message}</p></form></main></html>"
        ),
        next = escape(&encode_uri_component(next)),
        message = escape(message)
    );
    protect(Resp {
        status,
        headers: vec![
            ("Content-Type".into(), "text/html; charset=utf-8".into()),
            ("Content-Security-Policy".into(), "default-src 'none'; style-src 'unsafe-inline'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'".into()),
        ],
        body: html,
    })
}

/// `next` is only honoured when it is a same-site absolute path.
pub fn safe_destination(next: &str) -> &str {
    if next.starts_with('/') && !next.starts_with("//") && !next.contains(['\\', '\r', '\n']) {
        next
    } else {
        "/"
    }
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// `^(\d{10})\.([a-f0-9]{64})$`
fn parse_token(token: &str) -> Option<(&str, &str)> {
    let (exp, sig) = token.split_once('.')?;
    let ok = exp.len() == 10 && exp.bytes().all(|b| b.is_ascii_digit()) && sig.len() == 64 && sig.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    ok.then_some((exp, sig))
}

/// The worker's `fetch` handler. `now` is Unix seconds (`Date.now() / 1000`, floored).
pub async fn handle<L: Limiter>(req: &Req, cfg: &Config<'_>, limiter: &L, ip: Option<&str>, now: u64) -> Outcome {
    if cfg.site_password.is_empty() || cfg.session_secret.is_empty() {
        return plain(503, "Site access is not configured.");
    }
    let Ok(url) = url::Url::parse(&req.url) else { return plain(400, "Bad request.") };
    let origin = url.origin().ascii_serialization();
    if url.path() == "/__login" && req.method == "POST" {
        if req.header("Origin") != Some(origin.as_str()) {
            return plain(403, "Invalid origin.");
        }
        if !limiter.allow(ip.unwrap_or("unknown")).await {
            return Outcome::Respond(login("/", "Too many attempts. Please wait a minute.", 429));
        }
        // `Number(null)` is 0 and `Number("junk")` is NaN: neither exceeds the limit.
        if req.header("Content-Length").and_then(|v| v.trim().parse::<f64>().ok()).is_some_and(|n| n > 2048.0) {
            return plain(413, "Request too large.");
        }
        let password = form_urlencoded::parse(&req.body).find(|(k, _)| k == "password").map(|(_, v)| v.into_owned());
        let next = url.query_pairs().find(|(k, _)| k == "next").map(|(_, v)| v.into_owned()).unwrap_or_else(|| "/".into());
        let destination = safe_destination(&next);
        let Some(password) = password else { return Outcome::Respond(login("/", "Please enter the password.", 401)) };
        if utf16_len(&password) > 256 || !equal(&password, cfg.site_password) {
            return Outcome::Respond(login(destination, "That password didn’t match. Try again.", 401));
        }
        let expires = (now + MAX_AGE).to_string();
        // Bind sessions to the current password, so changing it also invalidates cookies.
        let token = format!("{expires}.{}", signature(&format!("{expires}:{}", cfg.site_password), cfg.session_secret));
        return Outcome::Respond(protect(Resp {
            status: 303,
            headers: vec![("Location".into(), destination.into()), ("Set-Cookie".into(), format!("{COOKIE}={token}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age={MAX_AGE}"))],
            body: String::new(),
        }));
    }
    let prefix = format!("{COOKIE}=");
    let token = req.header("Cookie").and_then(|c| c.split(';').map(str::trim).find(|v| v.starts_with(&prefix)).map(|v| &v[prefix.len()..])).unwrap_or("");
    if let Some((exp, sig)) = parse_token(token) {
        let exp_n: u64 = exp.parse().unwrap_or(0);
        if exp_n > now && exp_n <= now + MAX_AGE && equal(sig, &signature(&format!("{exp}:{}", cfg.site_password), cfg.session_secret)) {
            return Outcome::Serve;
        }
    }
    let query = url.query().map(|q| format!("?{q}")).unwrap_or_default();
    Outcome::Respond(login(&format!("{}{query}", url.path()), "", 401))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    // Mirrors tests/worker.test.ts.
    const ORIGIN: &str = "https://jumpman.example";
    const NOW: u64 = 1_800_000_000;

    struct Limit(Cell<bool>, RefCell<Vec<String>>);
    impl Limiter for Limit {
        fn allow(&self, key: &str) -> impl std::future::Future<Output = bool> {
            self.1.borrow_mut().push(key.to_string());
            std::future::ready(self.0.get())
        }
    }

    fn cfg() -> Config<'static> {
        Config { site_password: "test-password", session_secret: "test-only-secret-0123456789" }
    }
    fn limiter() -> Limit {
        Limit(Cell::new(true), RefCell::new(vec![]))
    }
    fn get(path: &str, cookie: Option<&str>) -> Req {
        Req { method: "GET".into(), url: format!("{ORIGIN}{path}"), headers: cookie.map(|c| vec![("Cookie".into(), c.into())]).unwrap_or_default(), body: vec![] }
    }
    fn submit(password: &str, next: &str) -> Req {
        Req {
            method: "POST".into(),
            url: format!("{ORIGIN}/__login?next={}", encode_uri_component(next)),
            headers: vec![("Origin".into(), ORIGIN.into())],
            body: form_urlencoded::Serializer::new(String::new()).append_pair("password", password).finish().into_bytes(),
        }
    }
    fn run(req: &Req, cfg: &Config, l: &Limit, now: u64) -> Outcome {
        pollster::block_on(handle(req, cfg, l, None, now))
    }
    fn status(o: Outcome) -> u16 {
        match o {
            Outcome::Respond(r) => r.status,
            Outcome::Serve => 200,
        }
    }
    fn respond(o: Outcome) -> Resp {
        match o {
            Outcome::Respond(r) => r,
            Outcome::Serve => panic!("expected a direct response"),
        }
    }

    #[test]
    fn protects_html_and_direct_asset_links_before_serving_any_bytes() {
        let l = limiter();
        for path in ["/", "/assets/mario.glb", "/models/quaternius/AnimationLibrary.glb"] {
            assert_eq!(status(run(&get(path, None), &cfg(), &l, NOW)), 401);
        }
        assert_eq!(status(run(&submit("wrong", "/?level=pss"), &cfg(), &l, NOW)), 401);
        let unset = Config { site_password: "", ..cfg() };
        assert_eq!(status(run(&get("", None), &unset, &l, NOW)), 503);
        let r = respond(run(&get("/", None), &cfg(), &l, NOW));
        assert_eq!(r.header("Cache-Control"), Some("private, no-store"));
        assert_eq!(r.header("X-Frame-Options"), Some("DENY"));
        assert!(r.header("Content-Security-Policy").unwrap().contains("frame-ancestors 'none'"));
    }

    #[test]
    fn issues_secure_signed_sessions_preserves_course_and_rejects_tampering() {
        let l = limiter();
        let login = respond(run(&submit("test-password", "/?level=pss"), &cfg(), &l, NOW));
        assert_eq!(login.status, 303);
        assert_eq!(login.header("Location"), Some("/?level=pss"));
        let cookie = login.header("Set-Cookie").unwrap().to_string();
        assert!(cookie.contains("HttpOnly; Secure; SameSite=Strict"));
        let serves = |c: &str, cfg: &Config| matches!(run(&get("/assets/mario.glb", Some(c)), cfg, &l, NOW), Outcome::Serve);
        assert!(serves(&cookie, &cfg()));
        let tampered = cookie.replacen("=1", "=0", 1);
        assert!(!serves(&tampered, &cfg()));
        assert!(!serves(&cookie, &Config { site_password: "changed", ..cfg() }));
    }

    #[test]
    fn rejects_expired_sessions_cross_site_login_offsite_redirects_and_excess_attempts() {
        let l = limiter();
        let login = respond(run(&submit("test-password", "//evil.example"), &cfg(), &l, NOW));
        assert_eq!(login.header("Location"), Some("/"));
        let cookie = login.header("Set-Cookie").unwrap().to_string();
        assert_eq!(status(run(&get("/", Some(&cookie)), &cfg(), &l, NOW + 86401)), 401);
        let mut foreign = submit("test-password", "/");
        foreign.headers = vec![("Origin".into(), "https://evil.example".into())];
        assert_eq!(status(run(&foreign, &cfg(), &l, NOW)), 403);
        l.0.set(false);
        assert_eq!(status(run(&submit("test-password", "/"), &cfg(), &l, NOW)), 429);
    }

    #[test]
    fn helpers_match_the_ts() {
        assert_eq!(encode_uri_component("/?level=pss&a b'"), "%2F%3Flevel%3Dpss%26a%20b'");
        assert_eq!(escape("<a href=\"x\">&'"), "&lt;a href=&quot;x&quot;&gt;&amp;&#39;");
        assert_eq!(safe_destination("/a?b=1"), "/a?b=1");
        assert_eq!(safe_destination("/a\\b"), "/");
        assert_eq!(safe_destination("http://x"), "/");
        // HMAC-SHA256("key", "The quick brown fox jumps over the lazy dog")
        assert_eq!(signature("The quick brown fox jumps over the lazy dog", "key"), "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8");
        assert!(equal("a", "a") && !equal("a", "b"));
    }

    #[test]
    fn oversized_and_malformed_bodies() {
        let l = limiter();
        let mut big = submit("x", "/");
        big.headers.push(("Content-Length".into(), "4096".into()));
        assert_eq!(status(run(&big, &cfg(), &l, NOW)), 413);
        let mut none = submit("x", "/");
        none.body = b"other=1".to_vec();
        let r = respond(run(&none, &cfg(), &l, NOW));
        assert_eq!(r.status, 401);
        assert!(r.body.contains("Please enter the password."));
    }
}
