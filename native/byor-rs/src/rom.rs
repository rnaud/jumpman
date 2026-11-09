//! ROM detection: byte order normalisation (.z64 / .v64 / .n64) and game identification by checksum.
//! Mirrors `byteswap_to_z64`, `identify` and the `ROMS` table of `byor/pipeline.py`.
use crate::hash;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Algo {
    Sha1,
    Md5,
}

pub struct GameRom {
    pub id: &'static str,
    pub label: &'static str,
    pub algo: Algo,
    pub digest: &'static str,
}

/// Same four ROMs and digests as `ROMS` in pipeline.py.
pub const ROMS: [GameRom; 4] = [
    GameRom { id: "sm64", label: "Super Mario 64 (US)", algo: Algo::Sha1, digest: "9bef1128717f958171a4afac3ed78ee2bb4e86ce" },
    GameRom { id: "mk64", label: "Mario Kart 64 (US 1.0)", algo: Algo::Sha1, digest: "579c48e211ae952530ffc8738709f078d5dd215e" },
    GameRom { id: "oot", label: "Ocarina of Time (US 1.2)", algo: Algo::Md5, digest: "57a9719ad547c516342e1a15d5c28c3d" },
    GameRom { id: "bk", label: "Banjo-Kazooie (US 1.0)", algo: Algo::Md5, digest: "b29599651a13f681c9923d69354bf4a3" },
];

/// Accepts big-endian (.z64), byte-swapped (.v64) and little-endian (.n64) dumps; anything else is returned unchanged.
pub fn byteswap_to_z64(mut data: Vec<u8>) -> Vec<u8> {
    match data.get(..4) {
        Some([0x80, 0x37, 0x12, 0x40]) => {}
        Some([0x37, 0x80, 0x40, 0x12]) => data.chunks_exact_mut(2).for_each(|c| c.swap(0, 1)),
        Some([0x40, 0x12, 0x37, 0x80]) => data.chunks_exact_mut(4).for_each(|c| c.reverse()),
        _ => {}
    }
    data
}

pub fn digest_of(algo: Algo, data: &[u8]) -> String {
    match algo {
        Algo::Sha1 => hash::hex(&hash::sha1(data)),
        Algo::Md5 => hash::hex(&hash::md5(data)),
    }
}

/// `(game id, z64 bytes)` of a ROM image in any byte order.
pub fn identify_bytes(data: Vec<u8>) -> Option<(&'static str, Vec<u8>)> {
    let data = byteswap_to_z64(data);
    // A real ROM is at least a few MB: a stray small file is rejected without hashing it.
    if data.len() < 1 << 20 {
        return None;
    }
    ROMS.iter().find(|r| digest_of(r.algo, &data) == r.digest).map(|r| (r.id, data))
}

pub fn identify(path: &std::path::Path) -> std::io::Result<Option<(&'static str, Vec<u8>)>> {
    Ok(identify_bytes(std::fs::read(path)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn byte_orders() {
        let z = vec![0x80, 0x37, 0x12, 0x40, 1, 2, 3, 4];
        let v = vec![0x37, 0x80, 0x40, 0x12, 2, 1, 4, 3];
        let n = vec![0x40, 0x12, 0x37, 0x80, 4, 3, 2, 1];
        assert_eq!(byteswap_to_z64(z.clone()), z);
        assert_eq!(byteswap_to_z64(v), z);
        assert_eq!(byteswap_to_z64(n), z);
        assert_eq!(byteswap_to_z64(vec![1, 2, 3]), vec![1, 2, 3]);
    }
}
