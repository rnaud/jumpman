//! Port of `scripts/export-ssb64-music.py`: the Super Smash Bros. (US) music from the user's ROM. Every BGM sequence of
//! `S1_music.sbk` (a libultra ALSeqFile of compact-MIDI files, the format `n_alCSPlay` plays), the music bank
//! `B1_sounds1.ctl` / `.tbl` and `music.json` (the gmMusicID names of `src/gm/gmsound.h`, the menu and victory themes).
//! Writes `<out>/seq_NN.bin`, `bank.ctl`, `bank.tbl`, `music.json`, byte-identical to the Python exporter. The player is
//! `native/src/ssb64_music.rs` (Banjo-Kazooie's software sequence player at SSB64's 32 kHz).
use crate::pyc::{dumps, read, R};
use serde_json::{json, Map, Value};
use std::path::Path;

const SBK: (usize, usize) = (0xB277B0, 0xB4E5C0);
const CTL: (usize, usize) = (0xB4E5C0, 0xB54CE0);
const TBL: (usize, usize) = (0xB54CE0, 0xC6B650);
pub const RATE: u32 = 32000;
const VOLUME: u32 = 0x7800;

const MENU: [(&str, &str); 9] = [
    ("menu", "ModeSelect"),
    ("select", "BattleSelect"),
    ("results", "Results"),
    ("training", "TrainingMode"),
    ("data", "Data"),
    ("opening", "Opening"),
    ("credits", "Staffroll"),
    ("stage_clear", "1PStageClear"),
    ("game_over", "1PGameOver"),
];
const VICTORY: [(&str, &str); 12] = [
    ("mario", "WinMario"),
    ("luigi", "WinMario"),
    ("fox", "WinFox"),
    ("donkey", "WinDonkey"),
    ("samus", "WinMetroid"),
    ("link", "WinZelda"),
    ("yoshi", "WinYoshi"),
    ("captain", "WinFZero"),
    ("kirby", "WinKirby"),
    ("pikachu", "WinPMonsters"),
    ("purin", "WinPMonsters"),
    ("ness", "WinMother"),
];

fn be32(d: &[u8], o: usize) -> R<u32> {
    d.get(o..o + 4).map(|b| u32::from_be_bytes(b.try_into().unwrap())).ok_or_else(|| format!("read past the end at {o:#x}"))
}

/// gmMusicID (src/gm/gmsound.h) names in enum order, without `nSYAudioBGM`.
pub fn music_names(decomp: &Path) -> R<Vec<String>> {
    let text = read(&decomp.join("src/gm/gmsound.h"))?;
    let start = text.find("typedef enum gmMusicID").ok_or("gmsound.h: no gmMusicID")?;
    let body = &text[start..];
    let body = &body[body.find('{').ok_or("gmsound.h: no enum body")? + 1..body.find('}').ok_or("gmsound.h: no enum end")?];
    let mut names = Vec::new();
    for line in body.split('\n') {
        let line = line.split("//").next().unwrap_or("").trim();
        for part in line.split(',') {
            let part = part.split('=').next().unwrap_or("").trim();
            if let Some(rest) = part.strip_prefix("nSYAudioBGM") {
                if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && part != "nSYAudioBGMEnd" {
                    names.push(rest.to_string());
                }
            }
        }
    }
    Ok(names)
}

/// The ALSeqFile's sequences.
pub fn sequences(sbk: &[u8]) -> R<Vec<Vec<u8>>> {
    if sbk.len() < 4 || i16::from_be_bytes([sbk[0], sbk[1]]) != 0x5331 {
        return Err("S1_music.sbk: not an ALSeqFile".into());
    }
    let count = i16::from_be_bytes([sbk[2], sbk[3]]) as usize;
    (0..count)
        .map(|i| {
            let off = be32(sbk, 4 + 8 * i)? as usize;
            let len = be32(sbk, 8 + 8 * i)? as i32 as usize;
            sbk.get(off..off + len).map(|s| s.to_vec()).ok_or_else(|| format!("sequence {i} outside the file"))
        })
        .collect()
}

pub fn export(source: &Path, z64: &[u8], out: &Path) -> R<usize> {
    if crate::hash::hex(&crate::hash::sha1(z64)) != crate::ssb64::SHA1 {
        return Err("not Super Smash Bros. (US)".into());
    }
    let names = music_names(&crate::ssb64::decomp_dir(source))?;
    let part = |r: (usize, usize)| z64.get(r.0..r.1).ok_or_else(|| "ROM too small".to_string());
    let seqs = sequences(part(SBK)?)?;
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut tracks = Vec::new();
    let mut by_name = std::collections::HashMap::new();
    for (i, data) in seqs.iter().enumerate() {
        let file = format!("seq_{i:02}.bin");
        std::fs::write(out.join(&file), data).map_err(|e| e.to_string())?;
        let division = be32(data, 0x40)?;
        let mut live = 0;
        for k in 0..16 {
            if be32(data, 4 * k)? != 0 {
                live += 1;
            }
        }
        let name = names.get(i).cloned().unwrap_or_else(|| format!("Seq{i}"));
        by_name.insert(name.clone(), i);
        tracks.push(json!({"id": i, "file": file, "name": name, "bytes": data.len(), "division": division, "channels": live, "volume": VOLUME}));
    }
    std::fs::write(out.join("bank.ctl"), part(CTL)?).map_err(|e| e.to_string())?;
    std::fs::write(out.join("bank.tbl"), part(TBL)?).map_err(|e| e.to_string())?;
    let pick = |table: &[(&str, &str)]| -> Map<String, Value> { table.iter().filter_map(|(k, v)| by_name.get(*v).map(|id| (k.to_string(), json!(id)))).collect() };
    let doc = crate::ssb64::sort_keys(json!({"game": "ssb64", "rate": RATE, "tracks": tracks, "menu": pick(&MENU), "victory": pick(&VICTORY)}));
    std::fs::write(out.join("music.json"), dumps(&doc, Some(1)) + "\n").map_err(|e| e.to_string())?;
    Ok(seqs.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_alseqfile() {
        let mut sbk = vec![0x53, 0x31, 0, 2];
        sbk.extend_from_slice(&20u32.to_be_bytes());
        sbk.extend_from_slice(&3u32.to_be_bytes());
        sbk.extend_from_slice(&23u32.to_be_bytes());
        sbk.extend_from_slice(&1u32.to_be_bytes());
        sbk.extend_from_slice(&[1, 2, 3, 9]);
        assert_eq!(sequences(&sbk).unwrap(), vec![vec![1, 2, 3], vec![9]]);
        assert!(sequences(&[0, 0, 0, 0]).is_err());
    }
}
