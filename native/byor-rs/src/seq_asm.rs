//! A tiny assembler for the SM64 decomp's sound-player sequence (`sound/sequences/00_sound_player.s` with
//! `include/seq_macros.inc`), so the import builds sequence 0 from decomp source on every platform without clang.
//!
//! It implements exactly the subset those files use, with the semantics of `clang -x assembler-with-cpp` (the
//! reference build in `scripts/render-music.py`):
//! - C preprocessor: `#include "file"`, `#if` (`defined(X)`, `!`, `&&`, `||`, integers, parentheses), `#ifdef`, `#ifndef`,
//!   `#else`, `#endif`, `//` and `/* */` comments, `-D` names (defined as `1`);
//! - GNU as: `.section .rodata`, `.align`/`.balign N[, fill]`, `.byte`, `.set NAME, expr` / `NAME = expr`, labels (also
//!   `name\@:` inside macros), `.macro`/`.endm` with `\param`, `param=default` and `\@` (arguments split like llvm-mc: by
//!   commas, or by whitespace not next to an operator), `.if`/`.else`/`.endif`, and integer expressions with GNU
//!   precedence (`||`; `&&`; comparisons; `+ -`; `| & ^`; `* / % << >>`; unary `- + ~ !`).
//!
//! The Python twin is `native/byor/seq_asm.py` (used by `byor/inproc.py`). The byte-equality test against clang's output
//! is `tests::sound_player_matches_clang` (`BYOR_TEST_SOUND_PLAYER_BIN` = the `.rodata` clang produced).
//!
//! Anything else (another directive, an unknown macro, `#define`, a byte value out of range, a shift of a negative value)
//! is an error, so a decomp change outside this subset fails loudly instead of producing different bytes.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

type R<T> = Result<T, String>;

/// The assembled `.rodata` of `00_sound_player.s` for the US build (`-DVERSION_US`), given the decomp checkout.
pub fn sound_player(decomp: &Path) -> R<Vec<u8>> {
    let src = decomp.join("sound/sequences/00_sound_player.s");
    assemble_file(&src, &[decomp.join("include")], &["VERSION_US"])
}

/// Preprocess and assemble `path` (includes are looked up next to the including file, then in `include_dirs`).
pub fn assemble_file(path: &Path, include_dirs: &[PathBuf], defines: &[&str]) -> R<Vec<u8>> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let dirs = include_dirs.to_vec();
    let mut load = |name: &str, from: &str| -> R<(String, String)> {
        let here = Path::new(from).parent().map(Path::to_path_buf).unwrap_or_default();
        for d in std::iter::once(here).chain(dirs.iter().cloned()) {
            let p = d.join(name);
            if p.is_file() {
                return Ok((p.display().to_string(), std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?));
            }
        }
        Err(format!("{from}: #include \"{name}\" not found"))
    };
    let lines = preprocess(&text, &path.display().to_string(), defines, &mut load, 0)?;
    assemble_lines(&lines)
}

/// Preprocess and assemble an in-memory source; `includes` maps `#include` names to their text.
pub fn assemble_str(text: &str, includes: &[(&str, &str)], defines: &[&str]) -> R<Vec<u8>> {
    let mut load = |name: &str, from: &str| -> R<(String, String)> {
        includes.iter().find(|(n, _)| *n == name).map(|(n, t)| (n.to_string(), t.to_string())).ok_or_else(|| format!("{from}: #include \"{name}\" not found"))
    };
    let lines = preprocess(text, "<input>", defines, &mut load, 0)?;
    assemble_lines(&lines)
}

/// One source line after preprocessing, with its origin for error messages.
#[derive(Clone)]
struct Line {
    text: String,
    at: String,
}

// ---------------------------------------------------------------------------------------------------------------------
// C preprocessor subset

fn strip_comments(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let (mut i, mut start) = (0, 0);
    let mut in_str = false;
    while i < b.len() {
        if in_str {
            if b[i] == b'"' || b[i] == b'\n' {
                in_str = false;
            }
            i += 1;
        } else if b[i] == b'"' {
            in_str = true;
            i += 1;
        } else if b[i] == b'/' && b.get(i + 1) == Some(&b'/') {
            out.push_str(&text[start..i]);
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            start = i;
        } else if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            out.push_str(&text[start..i]);
            out.push(' ');
            let end = text[i + 2..].find("*/").map_or(b.len(), |e| i + 2 + e + 2);
            // keep the line count so error locations stay right
            out.extend(text[i..end.min(b.len())].chars().filter(|c| *c == '\n'));
            i = end;
            start = i.min(b.len());
        } else {
            i += 1;
        }
    }
    out.push_str(&text[start.min(b.len())..]);
    out
}

type Loader<'a> = dyn FnMut(&str, &str) -> R<(String, String)> + 'a;

fn preprocess(text: &str, file: &str, defines: &[&str], load: &mut Loader, depth: usize) -> R<Vec<Line>> {
    if depth > 16 {
        return Err(format!("{file}: #include nested too deeply"));
    }
    let text = strip_comments(text);
    let mut out = Vec::new();
    // (this branch active, some branch taken, parent active)
    let mut stack: Vec<(bool, bool, bool)> = Vec::new();
    let active = |s: &Vec<(bool, bool, bool)>| s.last().map_or(true, |t| t.0);
    for (n, raw) in text.lines().enumerate() {
        let at = format!("{file}:{}", n + 1);
        let t = raw.trim();
        if let Some(d) = t.strip_prefix('#') {
            let d = d.trim_start();
            let (word, rest) = d.split_once(|c: char| c.is_whitespace()).map_or((d, ""), |(w, r)| (w, r.trim()));
            let on = active(&stack);
            match word {
                "if" | "ifdef" | "ifndef" => {
                    let v = if !on {
                        false
                    } else if word == "if" {
                        cpp_expr(rest, defines).map_err(|e| format!("{at}: {e}"))? != 0
                    } else {
                        if !is_ident(rest) {
                            return Err(format!("{at}: #{word} needs one name"));
                        }
                        defines.contains(&rest) == (word == "ifdef")
                    };
                    stack.push((on && v, v, on));
                }
                "else" => {
                    let top = stack.last_mut().ok_or(format!("{at}: #else without #if"))?;
                    top.0 = top.2 && !top.1;
                    top.1 = true;
                }
                "endif" => {
                    stack.pop().ok_or(format!("{at}: #endif without #if"))?;
                }
                _ if !on => {}
                "include" => {
                    let name = rest.strip_prefix('"').and_then(|r| r.strip_suffix('"')).ok_or(format!("{at}: only #include \"file\" is supported"))?;
                    let (path, body) = load(name, file)?;
                    out.extend(preprocess(&body, &path, defines, load, depth + 1)?);
                }
                _ => return Err(format!("{at}: unsupported preprocessor directive #{word}")),
            }
            continue;
        }
        if !active(&stack) || t.is_empty() {
            continue;
        }
        // object-like macros from -D (value 1) would be replaced in the text; this source never uses them there
        if let Some(d) = defines.iter().find(|d| tokens(t).any(|w| w == **d)) {
            return Err(format!("{at}: {d} used outside a preprocessor directive"));
        }
        out.push(Line { text: t.to_string(), at });
    }
    if !stack.is_empty() {
        return Err(format!("{file}: unterminated #if"));
    }
    Ok(out)
}

fn ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '$'
}

fn is_ident(s: &str) -> bool {
    !s.is_empty() && !s.starts_with(|c: char| c.is_ascii_digit()) && s.chars().all(ident_char)
}

fn tokens(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| !ident_char(c)).filter(|w| !w.is_empty())
}

/// `#if` expressions: `defined(X)` / `defined X`, `!`, `&&`, `||`, integers, parentheses.
fn cpp_expr(s: &str, defines: &[&str]) -> R<i64> {
    // rewrite `defined(X)` into 1/0, then reuse the assembler's expression parser for the rest
    let mut t = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("defined") {
        t.push_str(&rest[..i]);
        let after = rest[i + 7..].trim_start();
        let (name, tail) = if let Some(a) = after.strip_prefix('(') {
            let j = a.find(')').ok_or("unclosed defined(")?;
            (a[..j].trim(), &a[j + 1..])
        } else {
            let j = after.find(|c: char| !ident_char(c)).unwrap_or(after.len());
            (&after[..j], &after[j..])
        };
        if !is_ident(name) {
            return Err(format!("bad defined() in #if {s}"));
        }
        t.push_str(if defines.contains(&name) { " 1 " } else { " 0 " });
        rest = tail;
    }
    t.push_str(rest);
    if let Some(w) = tokens(&t).find(|w| !w.starts_with(|c: char| c.is_ascii_digit())) {
        return Err(format!("unsupported #if operand {w}"));
    }
    let e = parse_expr(&t)?;
    e.eval(&|_| None)
}

// ---------------------------------------------------------------------------------------------------------------------
// expressions

#[derive(Clone, Debug)]
enum Expr {
    Num(i64),
    Sym(String),
    Un(char, Box<Expr>),
    Bin(&'static str, Box<Expr>, Box<Expr>),
}

impl Expr {
    fn eval(&self, sym: &dyn Fn(&str) -> Option<i64>) -> R<i64> {
        Ok(match self {
            Expr::Num(n) => *n,
            Expr::Sym(s) => sym(s).ok_or_else(|| format!("undefined symbol {s}"))?,
            Expr::Un(op, a) => {
                let a = a.eval(sym)?;
                match op {
                    '-' => a.wrapping_neg(),
                    '+' => a,
                    '~' => !a,
                    _ => (a == 0) as i64,
                }
            }
            Expr::Bin(op, a, b) => {
                let (a, b) = (a.eval(sym)?, b.eval(sym)?);
                match *op {
                    "||" => (a != 0 || b != 0) as i64,
                    "&&" => (a != 0 && b != 0) as i64,
                    "==" => (a == b) as i64,
                    "!=" | "<>" => (a != b) as i64,
                    "<" => (a < b) as i64,
                    "<=" => (a <= b) as i64,
                    ">" => (a > b) as i64,
                    ">=" => (a >= b) as i64,
                    "+" => a.wrapping_add(b),
                    "-" => a.wrapping_sub(b),
                    "|" => a | b,
                    "&" => a & b,
                    "^" => a ^ b,
                    "*" => a.wrapping_mul(b),
                    "/" | "%" if b == 0 => return Err("division by zero".into()),
                    "/" => a / b,
                    "%" => a % b,
                    "<<" | ">>" if !(0..64).contains(&b) => return Err(format!("shift by {b}")),
                    "<<" => a << b,
                    // logical or arithmetic depends on the assembler; refuse the case where they differ
                    ">>" if a < 0 => return Err(format!("right shift of a negative value ({a})")),
                    ">>" => a >> b,
                    _ => unreachable!(),
                }
            }
        })
    }

    /// Replace the symbols `sym` knows by their values (`.set` values are taken where they are used).
    fn resolve(self, sym: &dyn Fn(&str) -> Option<i64>) -> Expr {
        match self {
            Expr::Sym(s) => sym(&s).map_or(Expr::Sym(s), Expr::Num),
            Expr::Un(o, a) => Expr::Un(o, Box::new(a.resolve(sym))),
            Expr::Bin(o, a, b) => Expr::Bin(o, Box::new(a.resolve(sym)), Box::new(b.resolve(sym))),
            e => e,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(i64),
    Id(String),
    Op(&'static str),
}

const OPS: [&str; 22] = ["||", "&&", "==", "!=", "<>", "<=", ">=", "<<", ">>", "<", ">", "+", "-", "|", "&", "^", "*", "/", "%", "~", "!", "("];

fn lex(s: &str) -> R<Vec<Tok>> {
    let mut out = Vec::new();
    let mut r = s.trim_start();
    while !r.is_empty() {
        let c = r.chars().next().unwrap();
        if c.is_ascii_digit() {
            let j = r.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(r.len());
            out.push(Tok::Num(parse_int(&r[..j])?));
            r = &r[j..];
        } else if ident_char(c) {
            let j = r.find(|c: char| !ident_char(c)).unwrap_or(r.len());
            out.push(Tok::Id(r[..j].to_string()));
            r = &r[j..];
        } else if c == ')' {
            out.push(Tok::Op(")"));
            r = &r[1..];
        } else if let Some(op) = OPS.iter().find(|o| r.starts_with(**o)) {
            out.push(Tok::Op(op));
            r = &r[op.len()..];
        } else {
            return Err(format!("unexpected '{c}' in expression {s}"));
        }
        r = r.trim_start();
    }
    Ok(out)
}

/// GNU as integer literals: decimal, `0x` hex, `0b` binary, leading-zero octal.
fn parse_int(s: &str) -> R<i64> {
    let bad = || format!("bad number {s}");
    let (digits, radix) = if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        (h, 16)
    } else if let Some(b) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
        (b, 2)
    } else if s.len() > 1 && s.starts_with('0') {
        (&s[1..], 8)
    } else {
        (s, 10)
    };
    u64::from_str_radix(digits, radix).map(|v| v as i64).map_err(|_| bad())
}

fn prec(op: &str) -> Option<u8> {
    Some(match op {
        "||" => 1,
        "&&" => 2,
        "==" | "!=" | "<>" | "<" | "<=" | ">" | ">=" => 3,
        "+" | "-" => 4,
        "|" | "&" | "^" => 5,
        "*" | "/" | "%" | "<<" | ">>" => 6,
        _ => return None,
    })
}

fn parse_expr(s: &str) -> R<Expr> {
    let toks = lex(s)?;
    let mut i = 0;
    let e = parse_bin(&toks, &mut i, 1).map_err(|e| format!("{e} in expression `{s}`"))?;
    if i != toks.len() {
        return Err(format!("trailing tokens in expression `{s}`"));
    }
    Ok(e)
}

fn parse_bin(t: &[Tok], i: &mut usize, min: u8) -> R<Expr> {
    let mut lhs = parse_unary(t, i)?;
    while let Some(Tok::Op(op)) = t.get(*i) {
        let Some(p) = prec(op).filter(|p| *p >= min) else { break };
        *i += 1;
        let rhs = parse_bin(t, i, p + 1)?;
        lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
    }
    Ok(lhs)
}

fn parse_unary(t: &[Tok], i: &mut usize) -> R<Expr> {
    let tok = t.get(*i).ok_or("unexpected end")?.clone();
    *i += 1;
    match tok {
        Tok::Num(n) => Ok(Expr::Num(n)),
        Tok::Id(s) => Ok(Expr::Sym(s)),
        Tok::Op("(") => {
            let e = parse_bin(t, i, 1)?;
            if t.get(*i) != Some(&Tok::Op(")")) {
                return Err("missing )".into());
            }
            *i += 1;
            Ok(e)
        }
        Tok::Op(o @ ("-" | "+" | "~" | "!")) => Ok(Expr::Un(o.chars().next().unwrap(), Box::new(parse_unary(t, i)?))),
        Tok::Op(o) => Err(format!("unexpected {o}")),
    }
}

/// Split on commas outside parentheses.
fn split_args(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut depth) = (Vec::new(), String::new(), 0i32);
    for c in s.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() || !out.is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// Macro arguments the way llvm-mc reads them: commas separate, and outside parentheses so does whitespace, unless the
/// whitespace sits next to an operator (`envelope_line 7 20000` is two arguments, `x - 1` and `(a - b)` are one).
fn split_macro_args(s: &str) -> Vec<String> {
    const OPERATOR: &str = "+-~/*%=|^&!<>";
    let c: Vec<char> = s.trim().chars().collect();
    let (mut out, mut cur, mut depth, mut i) = (Vec::new(), String::new(), 0i32, 0);
    while i < c.len() {
        match c[i] {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
                i += 1;
                continue;
            }
            w if w.is_whitespace() && depth == 0 => {
                while i < c.len() && c[i].is_whitespace() {
                    i += 1;
                }
                let next = c.get(i).copied();
                let joined = next.is_some_and(|n| OPERATOR.contains(n)) || cur.trim_end().ends_with(|p: char| OPERATOR.contains(p));
                if joined || cur.trim().is_empty() || next.is_none() || next == Some(',') {
                    cur.push(' ');
                } else {
                    out.push(cur.trim().to_string());
                    cur.clear();
                }
                continue;
            }
            _ => {}
        }
        cur.push(c[i]);
        i += 1;
    }
    if !cur.trim().is_empty() || !out.is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

// ---------------------------------------------------------------------------------------------------------------------
// assembler

struct Macro {
    params: Vec<(String, Option<String>)>,
    body: Vec<Line>,
}

#[derive(Default)]
struct Asm {
    macros: HashMap<String, Macro>,
    labels: HashMap<String, i64>,
    sets: HashMap<String, i64>,
    bytes: Vec<(Expr, String)>,
    in_rodata: bool,
    /// (this branch active, parent active) of the open `.if`s
    conds: Vec<(bool, bool)>,
    /// `\@`: macros expanded so far
    count: usize,
    depth: usize,
}

fn assemble_lines(lines: &[Line]) -> R<Vec<u8>> {
    let mut a = Asm::default();
    let mut i = 0;
    while i < lines.len() {
        i = a.statement(lines, i)?;
    }
    if !a.conds.is_empty() {
        return Err("unterminated .if".into());
    }
    let labels = &a.labels;
    a.bytes
        .iter()
        .map(|(e, at)| {
            let v = e.eval(&|s| labels.get(s).copied()).map_err(|m| format!("{at}: {m}"))?;
            // like llvm-mc: a .byte must fit in 8 bits, signed or unsigned
            if !(-128..=255).contains(&v) {
                return Err(format!("{at}: .byte value {v} out of range"));
            }
            Ok(v as u8)
        })
        .collect()
}

impl Asm {
    fn active(&self) -> bool {
        self.conds.last().map_or(true, |c| c.0)
    }

    fn lookup(&self, s: &str) -> Option<i64> {
        self.sets.get(s).or_else(|| self.labels.get(s)).copied()
    }

    fn value(&self, s: &str, at: &str) -> R<i64> {
        parse_expr(s).and_then(|e| e.eval(&|n| self.lookup(n))).map_err(|e| format!("{at}: {e}"))
    }

    /// Handle `lines[i]` (a `.macro` consumes its body too); returns the index of the next line.
    fn statement(&mut self, lines: &[Line], i: usize) -> R<usize> {
        let Line { text, at } = &lines[i];
        let at = at.as_str();
        if text.contains(';') {
            return Err(format!("{at}: ';' statement separators are not supported"));
        }
        let mut s = text.trim();
        let (word, rest) = s.split_once(|c: char| c.is_whitespace()).map_or((s, ""), |(w, r)| (w, r.trim()));
        // conditionals are tracked even inside skipped blocks
        match word {
            ".if" => {
                let on = self.active();
                let v = on && self.value(rest, at)? != 0;
                self.conds.push((v, on));
                return Ok(i + 1);
            }
            ".else" => {
                let c = self.conds.last_mut().ok_or(format!("{at}: .else without .if"))?;
                c.0 = c.1 && !c.0;
                return Ok(i + 1);
            }
            ".endif" => {
                self.conds.pop().ok_or(format!("{at}: .endif without .if"))?;
                return Ok(i + 1);
            }
            _ if !self.active() => {
                if word == ".macro" {
                    return Err(format!("{at}: .macro inside .if is not supported"));
                }
                return Ok(i + 1);
            }
            ".macro" => return self.define(lines, i),
            _ => {}
        }
        // labels
        while let Some(j) = s.find(':') {
            let name = s[..j].trim();
            if !is_ident(name) {
                return Err(format!("{at}: bad label `{name}`"));
            }
            if self.labels.contains_key(name) || self.sets.contains_key(name) {
                return Err(format!("{at}: symbol {name} defined twice"));
            }
            if !self.in_rodata {
                return Err(format!("{at}: label {name} outside .section .rodata"));
            }
            self.labels.insert(name.to_string(), self.bytes.len() as i64);
            s = s[j + 1..].trim();
        }
        if s.is_empty() {
            return Ok(i + 1);
        }
        let (word, rest) = s.split_once(|c: char| c.is_whitespace()).map_or((s, ""), |(w, r)| (w, r.trim()));
        if let Some((name, value)) = s.split_once('=').filter(|(n, v)| is_ident(n.trim()) && !v.starts_with('=')) {
            return self.set(name.trim(), value, at).map(|_| i + 1);
        }
        match word {
            ".section" => {
                if rest != ".rodata" {
                    return Err(format!("{at}: only .section .rodata is supported, not {rest}"));
                }
                self.in_rodata = true;
            }
            ".align" | ".balign" => {
                let args = split_args(rest);
                if args.is_empty() || args.len() > 2 {
                    return Err(format!("{at}: {word} ALIGN[, FILL] (no max-skip) is supported"));
                }
                // x86 ELF: both take a byte alignment (0 means none)
                let n = self.value(&args[0], at)?;
                if n < 0 || (n > 1 && (n & (n - 1)) != 0) {
                    return Err(format!("{at}: bad alignment {n}"));
                }
                let fill = match args.get(1) {
                    Some(f) => self.value(f, at)?,
                    None => 0,
                };
                if !(-128..=255).contains(&fill) {
                    return Err(format!("{at}: bad fill value {fill}"));
                }
                self.need_rodata(at)?;
                while n > 1 && self.bytes.len() as i64 % n != 0 {
                    self.bytes.push((Expr::Num(fill), at.to_string()));
                }
            }
            ".byte" => {
                self.need_rodata(at)?;
                for a in split_args(rest) {
                    if a.is_empty() {
                        return Err(format!("{at}: empty .byte operand"));
                    }
                    let e = parse_expr(&a).map_err(|e| format!("{at}: {e}"))?.resolve(&|n| self.lookup(n));
                    self.bytes.push((e, at.to_string()));
                }
            }
            ".set" | ".equ" => {
                let (name, value) = rest.split_once(',').ok_or(format!("{at}: {word} NAME, VALUE"))?;
                self.set(name.trim(), value, at)?;
            }
            ".endm" => return Err(format!("{at}: .endm without .macro")),
            _ if self.macros.contains_key(word) => self.expand(word, rest, at)?,
            _ => return Err(format!("{at}: unsupported directive or unknown macro `{word}`")),
        }
        Ok(i + 1)
    }

    fn need_rodata(&self, at: &str) -> R<()> {
        if self.in_rodata { Ok(()) } else { Err(format!("{at}: data outside .section .rodata")) }
    }

    fn set(&mut self, name: &str, value: &str, at: &str) -> R<()> {
        if !is_ident(name) || self.labels.contains_key(name) {
            return Err(format!("{at}: cannot .set {name}"));
        }
        let v = self.value(value, at)?;
        self.sets.insert(name.to_string(), v);
        Ok(())
    }

    fn define(&mut self, lines: &[Line], i: usize) -> R<usize> {
        let at = &lines[i].at;
        let head = lines[i].text.trim()[6..].trim();
        let (name, params) = head.split_once(|c: char| c.is_whitespace() || c == ',').map_or((head, ""), |(n, p)| (n, p));
        if !is_ident(name) {
            return Err(format!("{at}: bad macro name `{name}`"));
        }
        let mut ps = Vec::new();
        for p in params.split(|c: char| c == ',' || c.is_whitespace()).filter(|p| !p.is_empty()) {
            let (n, d) = p.split_once('=').map_or((p, None), |(n, d)| (n, Some(d.to_string())));
            if !is_ident(n) || n.contains('.') {
                return Err(format!("{at}: bad macro parameter `{p}`"));
            }
            ps.push((n.to_string(), d));
        }
        let mut j = i + 1;
        let mut body = Vec::new();
        loop {
            let l = lines.get(j).ok_or(format!("{at}: .macro {name} without .endm"))?;
            let w = l.text.split_whitespace().next().unwrap_or("");
            if w == ".endm" {
                break;
            }
            if w == ".macro" {
                return Err(format!("{}: nested .macro", l.at));
            }
            body.push(l.clone());
            j += 1;
        }
        if self.macros.insert(name.to_string(), Macro { params: ps, body }).is_some() {
            return Err(format!("{at}: macro {name} defined twice"));
        }
        Ok(j + 1)
    }

    fn expand(&mut self, name: &str, args: &str, at: &str) -> R<()> {
        if self.depth > 32 {
            return Err(format!("{at}: macro recursion too deep"));
        }
        let m = &self.macros[name];
        let args = split_macro_args(args);
        if args.len() > m.params.len() {
            return Err(format!("{at}: {name} takes {} arguments, got {}", m.params.len(), args.len()));
        }
        let mut vals = HashMap::new();
        for (k, (p, d)) in m.params.iter().enumerate() {
            let v = match args.get(k).filter(|a| !a.is_empty()) {
                Some(a) => a.clone(),
                None => d.clone().ok_or(format!("{at}: {name}: missing argument {p}"))?,
            };
            vals.insert(p.clone(), v);
        }
        let n = self.count.to_string();
        self.count += 1;
        let mut body = Vec::with_capacity(m.body.len());
        for l in &m.body {
            body.push(Line { text: substitute(&l.text, &vals, &n).map_err(|e| format!("{}: {e}", l.at))?, at: format!("{} (from {at})", l.at) });
        }
        let conds = self.conds.len();
        self.depth += 1;
        let mut i = 0;
        while i < body.len() {
            i = self.statement(&body, i)?;
        }
        self.depth -= 1;
        if self.conds.len() != conds {
            return Err(format!("{at}: unbalanced .if in macro {name}"));
        }
        Ok(())
    }
}

/// `\param`, `\@` and `\()` inside a macro body.
fn substitute(text: &str, vals: &HashMap<String, String>, count: &str) -> R<String> {
    let mut out = String::with_capacity(text.len());
    let mut r = text;
    while let Some(i) = r.find('\\') {
        out.push_str(&r[..i]);
        r = &r[i + 1..];
        if let Some(t) = r.strip_prefix('@') {
            out.push_str(count);
            r = t;
        } else if let Some(t) = r.strip_prefix("()") {
            r = t;
        } else {
            let j = r.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(r.len());
            let v = vals.get(&r[..j]).ok_or_else(|| format!("unknown macro parameter \\{}", &r[..j]))?;
            out.push_str(v);
            r = &r[j..];
        }
    }
    out.push_str(r);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asm(s: &str) -> R<Vec<u8>> {
        assemble_str(&format!(".section .rodata\n{s}\n"), &[], &["VERSION_US"])
    }

    #[test]
    fn bytes_and_expressions() {
        assert_eq!(asm(".byte 1, 0x10, 0b11, 010, -1, 255").unwrap(), [1, 16, 3, 8, 0xff, 255]);
        assert_eq!(asm(".byte 0x1234 >> 8, 0x1234 & 0xff, (0x80 | (0x1234 & 0x7f00) >> 8)").unwrap(), [0x12, 0x34, 0x92]);
        // GNU precedence: | and & bind tighter than + and -
        assert_eq!(asm(".byte 1 + 2 | 4, 2 * 3 + 1, ~0 & 7, !0, (3 >= 2) + (1 == 2)").unwrap(), [7, 7, 7, 1, 1]);
        assert!(asm(".byte 256").unwrap_err().contains("out of range"));
        assert!(asm(".byte -129").unwrap_err().contains("out of range"));
        assert!(asm(".byte -1 >> 8").unwrap_err().contains("negative"));
    }

    #[test]
    fn labels_forward_and_backward() {
        let src = "start:\n.byte (fwd - start) >> 8, (fwd - start) & 0xff\nback: .byte back - start\n.byte 9\nfwd:\n.byte fwd";
        assert_eq!(asm(src).unwrap(), [0, 4, 2, 9, 4]);
        assert!(asm("x:\nx:").unwrap_err().contains("twice"));
        assert!(asm(".byte nowhere").unwrap_err().contains("undefined symbol nowhere"));
    }

    #[test]
    fn macros_defaults_counter_and_if() {
        let src = "s:\n.macro var x\n .if (\\x >= 0x80)\n  .byte (0x80 | (\\x & 0x7f00) >> 8), (\\x & 0xff)\n .else\n  .byte \\x\n .endif\n.endm\n\
                   .macro op a=3, b\n .byte 0x50 + \\a\n var \\b\n.endm\n\
                   .macro next v\n .byte \\v, (here\\@ - s)\n here\\@:\n.endm\n\
                   op , 5\nop 1, 0x123\nnext 7\nnext 8";
        assert_eq!(asm(src).unwrap(), [0x53, 5, 0x51, 0x81, 0x23, 7, 7, 8, 9]);
        assert!(asm(".macro m a\n.byte \\b\n.endm\nm 1").unwrap_err().contains("unknown macro parameter"));
        assert!(asm(".macro m a\n.byte \\a\n.endm\nm 1, 2").unwrap_err().contains("takes 1"));
        assert!(asm("nope 1").unwrap_err().contains("unknown macro"));
        assert!(asm(".word 1").unwrap_err().contains("unsupported"));
    }

    #[test]
    fn macro_arguments_split_like_llvm_mc() {
        let v = |s: &str| split_macro_args(s);
        assert_eq!(v("7 20000"), ["7", "20000"]);
        assert_eq!(v("46, 0xc, (75 - V), 20"), ["46", "0xc", "(75 - V)", "20"]);
        assert_eq!(v("x - 1, -2"), ["x - 1", "-2"]);
        assert_eq!(v(" a ,b "), ["a", "b"]);
        assert_eq!(v(", 5"), ["", "5"]);
        assert!(v("").is_empty());
        assert_eq!(asm(".macro e a, b\n.byte \\a >> 8, \\a & 0xff, \\b\n.endm\ne 0x1234 7\n.align 4, 0xee").unwrap(), [0x12, 0x34, 7, 0xee]);
    }

    #[test]
    fn set_values_are_taken_where_used() {
        assert_eq!(asm(".set V, 10\n.byte (75 - V)\nW = 3\n.set V, 0\n.byte V + W").unwrap(), [65, 3]);
    }

    #[test]
    fn preprocessor() {
        let inc = ".macro m\n#ifdef VERSION_EU\n.byte 1\n#else\n.byte 2\n#endif\n.endm\n";
        let src = "#include \"m.inc\"\n.section .rodata\n.align 0\nm // two\n/* skip\n.byte 9 */\n\
                   #if defined(VERSION_SH) || defined(VERSION_CN)\n.byte 3\n#else\n  #ifndef VERSION_JP\n.byte 4\n  #endif\n#endif\n\
                   #if defined(VERSION_US) && !defined(VERSION_EU)\n.byte 5\n#endif\n";
        assert_eq!(assemble_str(src, &[("m.inc", inc)], &["VERSION_US"]).unwrap(), [2, 4, 5]);
        assert!(assemble_str("#define X 1\n", &[], &[]).unwrap_err().contains("#define"));
        assert!(assemble_str("#include \"gone.inc\"\n", &[], &[]).unwrap_err().contains("not found"));
        assert!(assemble_str("#if 1\n", &[], &[]).unwrap_err().contains("unterminated"));
        assert!(assemble_str(".byte 1\n", &[], &[]).unwrap_err().contains("outside .section"));
    }

    /// Byte equality with the clang-assembled reference: BYOR_TEST_SOUND_PLAYER_BIN (the bytes `clang -x assembler-with-cpp
    /// -DVERSION_US` produced) and BYOR_TEST_SM64_DECOMP (default: the repo's reference/sm64 submodule).
    #[test]
    fn sound_player_matches_clang() {
        let Some(golden) = std::env::var_os("BYOR_TEST_SOUND_PLAYER_BIN") else {
            eprintln!("skipped: BYOR_TEST_SOUND_PLAYER_BIN not set");
            return;
        };
        let decomp = std::env::var_os("BYOR_TEST_SM64_DECOMP")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/sm64"));
        if !decomp.join("sound/sequences/00_sound_player.s").is_file() {
            eprintln!("skipped: no decomp at {}", decomp.display());
            return;
        }
        let want = std::fs::read(golden).unwrap();
        let got = sound_player(&decomp).unwrap();
        assert_eq!(got.len(), want.len());
        let first = got.iter().zip(&want).position(|(a, b)| a != b);
        assert!(first.is_none(), "first difference at byte {first:?}");
    }
}
