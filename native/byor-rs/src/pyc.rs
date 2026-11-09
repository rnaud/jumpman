//! Python-compatible helpers the exporter ports lean on: `repr(float)`, `json.dumps`, `int(s)` / `int(s, 0)`, `round`, and
//! the regex helpers shared by the decomp-source readers (`clean`, `args`).
use regex::{Captures, Regex};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub type R<T> = Result<T, String>;

static CACHE: OnceLock<Mutex<HashMap<String, Regex>>> = OnceLock::new();

/// Compiled-regex cache (patterns are built dynamically in a few places).
pub fn re(pattern: &str) -> Regex {
    let mut m = CACHE.get_or_init(Default::default).lock().unwrap();
    m.entry(pattern.to_string()).or_insert_with(|| Regex::new(pattern).unwrap_or_else(|e| panic!("bad regex {pattern}: {e}"))).clone()
}

/// `repr(float)`.
pub fn py_float(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0.0".into() } else { "0.0".into() };
    }
    let mut e = format!("{x:e}"); // shortest round-trip digits: d.ddde<exp>
    // Rust breaks exact decimal ties upwards, Python's repr (dtoa) to even: redo the formatting at the same precision in exact
    // mode (which rounds half to even) and keep it when it still round-trips.
    let n = e.split_once('e').unwrap().0.chars().filter(char::is_ascii_digit).count();
    let exact = format!("{:.*e}", n - 1, x);
    if exact.parse::<f64>() == Ok(x) {
        e = exact;
    }
    let (mant, exp) = e.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    let neg = mant.starts_with('-');
    let digits: String = mant.chars().filter(char::is_ascii_digit).collect();
    let sign = if neg { "-" } else { "" };
    if (-4..16).contains(&exp) {
        if exp >= 0 {
            let int_len = exp as usize + 1;
            if digits.len() <= int_len {
                format!("{sign}{digits}{}.0", "0".repeat(int_len - digits.len()))
            } else {
                format!("{sign}{}.{}", &digits[..int_len], &digits[int_len..])
            }
        } else {
            format!("{sign}0.{}{digits}", "0".repeat((-exp - 1) as usize))
        }
    } else {
        let m = if digits.len() == 1 { digits.clone() } else { format!("{}.{}", &digits[..1], &digits[1..]) };
        format!("{sign}{m}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs())
    }
}

fn dump_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 || (c as u32) > 0x7f => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{:04x}", u));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// `json.dumps(v, separators=(',', ':'))` (`indent = None`) or with `indent=2`-style layout when `indent` is `Some(2)`.
pub fn dumps(v: &Value, indent: Option<usize>) -> String {
    let mut out = String::new();
    dump(v, indent, 0, &mut out);
    out
}

fn dump(v: &Value, indent: Option<usize>, level: usize, out: &mut String) {
    let nl = |out: &mut String, l: usize| {
        if let Some(i) = indent {
            out.push('\n');
            out.push_str(&" ".repeat(i * l));
        }
    };
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            if n.is_f64() {
                out.push_str(&py_float(n.as_f64().unwrap()));
            } else {
                out.push_str(&n.to_string());
            }
        }
        Value::String(s) => dump_str(s, out),
        Value::Array(a) => {
            if a.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                nl(out, level + 1);
                dump(x, indent, level + 1, out);
            }
            nl(out, level);
            out.push(']');
        }
        Value::Object(m) => {
            if m.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (i, (k, x)) in m.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                nl(out, level + 1);
                dump_str(k, out);
                out.push(':');
                if indent.is_some() {
                    out.push(' ');
                }
                dump(x, indent, level + 1, out);
            }
            nl(out, level);
            out.push('}');
        }
    }
}

/// `int(s)`: optional whitespace, sign, decimal digits (underscores between digits allowed in Python; not needed here).
pub fn int(s: &str) -> R<i64> {
    s.trim().parse::<i64>().map_err(|_| format!("invalid literal for int(): {s:?}"))
}

/// `int(s, 0)`: C-like prefixes.
pub fn int0(s: &str) -> R<i64> {
    let t = s.trim();
    let (neg, body) = match t.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let v = if let Some(h) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        i64::from_str_radix(h, 16)
    } else if let Some(o) = body.strip_prefix("0o").or_else(|| body.strip_prefix("0O")) {
        i64::from_str_radix(o, 8)
    } else if let Some(b) = body.strip_prefix("0b").or_else(|| body.strip_prefix("0B")) {
        i64::from_str_radix(b, 2)
    } else if body.len() > 1 && body.starts_with('0') && body.bytes().any(|c| c != b'0') {
        return Err(format!("invalid literal for int() with base 0: {s:?}"));
    } else {
        body.parse::<i64>()
    }
    .map_err(|_| format!("invalid literal for int() with base 0: {s:?}"))?;
    Ok(if neg { -v } else { v })
}

/// `round(x)` for a float (half to even), returning an int.
pub fn round(x: f64) -> i64 {
    let r = x.round();
    if (x - x.trunc()).abs() == 0.5 && r % 2.0 != 0.0 {
        (r - x.signum()) as i64
    } else {
        r as i64
    }
}

/// `float(s)` as the decomp's `-1.0f` style literals use it after `.rstrip('f')`.
pub fn float(s: &str) -> R<f64> {
    s.trim().parse::<f64>().map_err(|_| format!("could not convert string to float: {s:?}"))
}

pub fn j<T: Into<Value>>(v: T) -> Value {
    v.into()
}

pub fn ints(a: &[String]) -> R<Vec<Value>> {
    a.iter().map(|s| int(s).map(Value::from)).collect()
}

/// `re.sub(r'/\*.*?\*/|//[^\n]*', '', text, flags=re.S)`
pub fn clean(text: &str) -> String {
    re(r"(?s)/\*.*?\*/|//[^\n]*").replace_all(text, "").into_owned()
}

fn bparam_subst(text: &str) -> String {
    re(r"BPARAM([1-4])\(([^()]*)\)")
        .replace_all(text, |m: &Captures| match &m[1] {
            "2" => m[2].to_string(),
            "1" => format!("first_{}", &m[2]),
            _ => "0".to_string(),
        })
        .into_owned()
}

/// The decomp scripts' `args(text, command)`: every `command(...)` call (no nested parentheses) split on commas.
/// `command` is a regex fragment (`SPECIAL_OBJECT(?:_WITH_YAW)?` is used that way).
pub fn args(text: &str, command: &str) -> Vec<Vec<String>> {
    let text = bparam_subst(text);
    re(&format!(r"\b{command}\s*\(([^()]*)\)")).captures_iter(&clean(&text)).map(|c| c[1].split(',').map(|s| s.trim().to_string()).collect()).collect()
}

pub fn read(path: &std::path::Path) -> R<String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn float_repr() {
        for (x, s) in [(1.0, "1.0"), (0.1, "0.1"), (100.0, "100.0"), (1e16, "1e+16"), (1.5e-5, "1.5e-05"), (0.0001, "0.0001"), (-2.5, "-2.5"), (123456.789, "123456.789"), (0.01, "0.01"), (22.5, "22.5"), (1e15, "1000000000000000.0"), (1.4999771118164062, "1.4999771118164062"), (1.49997711181640625, "1.4999771118164062"), (0.5, "0.5"), (2.5, "2.5"), (5e-324, "5e-324")] {
            assert_eq!(py_float(x), s);
        }
    }
    #[test]
    fn rounding_and_ints() {
        assert_eq!(round(0.5), 0);
        assert_eq!(round(1.5), 2);
        assert_eq!(round(-0.5), 0);
        assert_eq!(round(2.5), 2);
        assert_eq!(int0("0x1F").unwrap(), 31);
        assert_eq!(int0("0").unwrap(), 0);
        assert!(int0("010").is_err());
        assert_eq!(int(" -12 ").unwrap(), -12);
    }
    #[test]
    fn args_splits_and_expands_bparams() {
        let a = args("OBJECT(MODEL_X, 1, 2, 3, 0, 90, 0, BPARAM1(0x01) | BPARAM2(WARP_NODE_0A), bhvFoo)", "OBJECT");
        assert_eq!(a[0][7], "first_0x01 | WARP_NODE_0A");
    }
}
