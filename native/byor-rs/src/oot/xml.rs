//! A tiny XML reader for the zeldaret/oot `assets/xml/**/*.xml` ZAPD descriptions (elements, attributes, comments; no
//! namespaces, no text content that matters). Replaces Python's `xml.etree.ElementTree` for `definitions()`.
use crate::pyc::R;

#[derive(Debug, Default, Clone)]
pub struct Element {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Element>,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
}

fn decode(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';') else { break };
        let ent = &rest[1..end];
        match ent {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ => {
                let n = ent.strip_prefix("#x").and_then(|h| u32::from_str_radix(h, 16).ok()).or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()));
                if let Some(c) = n.and_then(char::from_u32) {
                    out.push(c);
                }
            }
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Parses a document and returns its root element.
pub fn parse(text: &str) -> R<Element> {
    let b = text.as_bytes();
    let mut i = 0;
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        if text[i..].starts_with("<!--") {
            i = text[i..].find("-->").map(|e| i + e + 3).ok_or("unterminated comment")?;
        } else if text[i..].starts_with("<?") || text[i..].starts_with("<!") {
            i = text[i..].find('>').map(|e| i + e + 1).ok_or("unterminated declaration")?;
        } else if text[i..].starts_with("</") {
            let e = stack.pop().ok_or("unbalanced end tag")?;
            i = text[i..].find('>').map(|x| i + x + 1).ok_or("unterminated end tag")?;
            match stack.last_mut() {
                Some(p) => p.children.push(e),
                None => root = Some(e),
            }
        } else {
            i += 1;
            let start = i;
            while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' && b[i] != b'/' {
                i += 1;
            }
            let mut el = Element { tag: text[start..i].to_string(), ..Default::default() };
            let mut selfclose = false;
            loop {
                while i < b.len() && b[i].is_ascii_whitespace() {
                    i += 1;
                }
                if i >= b.len() {
                    return Err("unterminated tag".into());
                }
                if b[i] == b'>' {
                    i += 1;
                    break;
                }
                if b[i] == b'/' {
                    selfclose = true;
                    i += 1;
                    continue;
                }
                let ns = i;
                while i < b.len() && b[i] != b'=' && !b[i].is_ascii_whitespace() && b[i] != b'>' {
                    i += 1;
                }
                let name = text[ns..i].to_string();
                while i < b.len() && b[i].is_ascii_whitespace() {
                    i += 1;
                }
                if i < b.len() && b[i] == b'=' {
                    i += 1;
                    while i < b.len() && b[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    let q = *b.get(i).ok_or("bad attribute")?;
                    if q != b'"' && q != b'\'' {
                        return Err(format!("bad attribute {name}"));
                    }
                    let vs = i + 1;
                    let ve = text[vs..].find(q as char).map(|e| vs + e).ok_or("unterminated attribute")?;
                    el.attrs.push((name, decode(&text[vs..ve])));
                    i = ve + 1;
                }
            }
            if selfclose {
                match stack.last_mut() {
                    Some(p) => p.children.push(el),
                    None => root = Some(el),
                }
            } else {
                stack.push(el);
            }
        }
    }
    root.ok_or_else(|| "no root element".to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_zapd_xml() {
        let r = super::parse("<Root>\n <!-- c -->\n <File Name=\"a\" Segment=\"6\">\n  <Array Name=\"v\" Count=\"2\"><Vtx/></Array>\n  <DList Name=\"d\" Offset=\"0x10\"/>\n </File>\n</Root>").unwrap();
        assert_eq!(r.tag, "Root");
        let f = &r.children[0];
        assert_eq!(f.attr("Name"), Some("a"));
        assert_eq!(f.children.len(), 2);
        assert_eq!(f.children[0].children[0].tag, "Vtx");
        assert_eq!(f.children[1].attr("Offset"), Some("0x10"));
    }
}
