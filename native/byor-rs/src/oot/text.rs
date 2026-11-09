//! Dialogue: `kokiri_text` (message table of the `code` file + `nes_message_data_static` -> pages) and `export_message_art`
//! (font atlas, glyph widths, box art) of `export-oot-courses.py`.
use super::model::{decode_image, png};
use super::*;
use crate::pyc::re;
use serde_json::{json, Value};
use std::collections::HashMap;

pub struct Messages {
    text: Seg,
    starts: HashMap<u16, (u8, usize)>,
}

fn choices(ident: u16) -> Option<&'static [u16]> {
    match ident {
        0x1016 => Some(&[0x1017, 0x1018]),
        0x1035 => Some(&[0x1036, 0x1037]),
        0x1038 => Some(&[0x1039, 0x103A, 0x103B]),
        0x103E => Some(&[0x103F, 0x1040]),
        _ => None,
    }
}

impl Messages {
    pub fn new(ctx: &Ctx) -> R<Messages> {
        let code = ctx.segment("code")?;
        let text = ctx.segment("nes_message_data_static")?;
        let mut pos = 0x8010_F13Cusize - 0x8001_16E0;
        let mut starts = HashMap::new();
        loop {
            let (ident, style, pointer) = (u16_(&code, pos), code[pos + 2], u32_(&code, pos + 4));
            pos += 8;
            if ident == 0xffff {
                break;
            }
            starts.insert(ident, (style, (pointer & MASK) as usize));
        }
        Ok(Messages { text, starts })
    }

    /// `messages(ident, greeting)`: the pages (`[{text, style[, branches]}]`) of one message.
    pub fn pages(&self, ident: u16, greeting: bool) -> R<Vec<Value>> {
        let (style, mut p) = *self.starts.get(&ident).ok_or_else(|| format!("KeyError: {ident:#x}"))?;
        let text = &self.text;
        let mut result: Vec<Value> = Vec::new();
        let mut line = String::new();
        loop {
            let mut c = *text.get(p).ok_or("IndexError: message text")?;
            p += 1;
            if c == 0x0c {
                p += 1;
                c = 4;
            }
            if c == 2 || c == 4 || c == 7 {
                if !line.is_empty() {
                    let has_choice = line.contains('\u{1b}') || line.contains('\u{1c}');
                    if greeting && has_choice {
                        let intro = line.split(['\u{1b}', '\u{1c}']).next().unwrap_or("").trim_end();
                        if !intro.is_empty() {
                            result.push(json!({"text": intro, "style": style}));
                        }
                        return Ok(result);
                    }
                    let mut page = json!({"text": line, "style": style});
                    if has_choice {
                        let list = choices(ident).ok_or_else(|| format!("KeyError: {ident:#x}"))?;
                        let branches: R<Vec<Value>> = list.iter().map(|&a| self.pages(a, false).map(Value::Array)).collect();
                        page["branches"] = Value::Array(branches?);
                    }
                    result.push(page);
                }
                if c == 7 && greeting {
                    return Ok(result);
                }
                if c == 7 {
                    result.extend(self.pages(u16_(text, p), false)?);
                    return Ok(result);
                }
                if c == 2 {
                    return Ok(result);
                }
                line.clear();
            } else if c == 5 || c == 6 {
                line.push(c as char);
                line.push(*text.get(p).ok_or("IndexError: message text")? as char);
                p += 1;
            } else if c == 0x12 {
                p += 2;
            } else if c == 0x13 || c == 0x14 {
                p += 1;
            } else if c == 0x1e {
                p += 1;
                line.push('0');
            } else if c == 0x1f {
                line.push_str("10:00");
            } else if c == 0x0f {
                line.push_str("{name}");
            } else if c == 1 {
                line.push('\n');
            } else if (32..0xac).contains(&c) || c == 8 || c == 9 || c == 0x1b || c == 0x1c {
                line.push(c as char);
            } else if c != 0x0b && c != 0x1a {
                return Err(format!("Unsupported message control {c:x} in {ident:x}"));
            }
        }
    }

    pub fn get(&self, ident: u16) -> R<Vec<Value>> {
        self.pages(ident, false)
    }
    pub fn greet(&self, ident: u16) -> R<Vec<Value>> {
        self.pages(ident, true)
    }
}

/// `export_message_art(segment)`: `assets/oot-dialogue/{glyphs,black,wood,next,end,arrow}.png` and `widths.json`.
pub fn export_message_art(ctx: &Ctx, assets: &Path) -> R<()> {
    let folder = assets.join("oot-dialogue");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let font = ctx.segment("nes_font_static")?;
    let mut atlas = vec![0u8; 256 * 144 * 4];
    for g in 0..144.min(font.len() / 128) {
        for y in 0..16 {
            for x in 0..16 {
                let value = font[g * 128 + y * 8 + x / 2];
                let alpha = (if x % 2 == 0 { value >> 4 } else { value & 15 }) * 17;
                let off = ((g / 16 * 16 + y) * 256 + g % 16 * 16 + x) * 4;
                atlas[off..off + 4].copy_from_slice(&[255, 255, 255, alpha]);
            }
        }
    }
    std::fs::write(folder.join("glyphs.png"), png(256, 144, &atlas)).map_err(|e| e.to_string())?;
    let src = ctx.read_decomp("src/code/z_message.c")?;
    let src = re(r"#if OOT_NTSC\n([^#]+)#else\n[^#]+#endif").replace_all(&src, "$1").into_owned();
    let table = src.split("sFontWidths[144] = {").nth(1).ok_or("sFontWidths not found")?.split("};").next().unwrap_or("");
    let widths: Vec<String> = re(r"(\d+)\.0f").captures_iter(table).map(|m| m[1].trim_start_matches('0').to_string()).map(|s| if s.is_empty() { "0".into() } else { s }).collect();
    if widths.len() != 144 {
        return Err("AssertionError: 144 font widths".into());
    }
    std::fs::write(folder.join("widths.json"), format!("[{}]", widths.join(", "))).map_err(|e| e.to_string())?;
    let data = ctx.segment("message_static")?;
    let segs: model::Segs = HashMap::from([(7, data.as_slice())]);
    for (name, start, w, h, fmt) in [("black", 0u32, 128, 64, 4), ("wood", 4096, 128, 64, 3), ("next", 0x4000, 16, 16, 4), ("end", 0x4080, 16, 16, 4), ("arrow", 0x4100, 16, 16, 4)] {
        std::fs::write(folder.join(format!("{name}.png")), decode_image(&segs, 0x0700_0000 + start, 0, fmt, 0, 0, w, h)?).map_err(|e| e.to_string())?;
    }
    Ok(())
}
