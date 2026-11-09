//! Byte-exact port of stb_image_write's PNG writer (`stbi_write_png`, compression level 8, automatic per-line filter
//! choice), which the decomp's `n64graphics` uses. Matching it bit for bit keeps the extracted textures identical to the
//! ones the Python/C pipeline produces.

const ZHASH: usize = 16384;

fn crc_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    for (n, e) in t.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
        }
        *e = c;
    }
    t
}

fn crc32(table: &[u32; 256], buf: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in buf {
        crc = (crc >> 8) ^ table[(b ^ (crc & 0xff) as u8) as usize];
    }
    !crc
}

struct Bits {
    out: Vec<u8>,
    buf: u32,
    count: i32,
}

impl Bits {
    fn add(&mut self, code: u32, bits: i32) {
        self.buf |= code << self.count;
        self.count += bits;
        while self.count >= 8 {
            self.out.push(self.buf as u8);
            self.buf >>= 8;
            self.count -= 8;
        }
    }
    fn rev(code: u32, bits: i32) -> u32 {
        let (mut res, mut code) = (0u32, code);
        for _ in 0..bits {
            res = (res << 1) | (code & 1);
            code >>= 1;
        }
        res
    }
    fn huff(&mut self, n: u32) {
        let (code, bits) = match n {
            0..=143 => (0x30 + n, 8),
            144..=255 => (0x190 + n - 144, 9),
            256..=279 => (n - 256, 7),
            _ => (0xc0 + n - 280, 8),
        };
        self.add(Self::rev(code, bits), bits);
    }
    fn huffb(&mut self, n: u32) {
        if n <= 143 {
            self.huff(n)
        } else {
            let (code, bits) = (0x190 + n - 144, 9);
            self.add(Self::rev(code, bits), bits);
        }
    }
}

fn zhash(d: &[u8]) -> usize {
    let mut h: u32 = d[0] as u32 + ((d[1] as u32) << 8) + ((d[2] as u32) << 16);
    h ^= h << 3;
    h = h.wrapping_add(h >> 5);
    h ^= h << 4;
    h = h.wrapping_add(h >> 17);
    h ^= h << 25;
    h = h.wrapping_add(h >> 6);
    h as usize
}

fn countm(a: &[u8], b: &[u8], limit: usize) -> usize {
    let mut i = 0;
    while i < limit && i < 258 && a[i] == b[i] {
        i += 1;
    }
    i
}

/// `stbi_zlib_compress(data, len, &out_len, quality)`: fixed-Huffman deflate with a bucketed hash chain.
pub fn zlib_compress(data: &[u8], quality: usize) -> Vec<u8> {
    const LENGTHC: [usize; 30] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258, 259];
    const LENGTHEB: [i32; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
    const DISTC: [usize; 31] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577, 32768];
    const DISTEB: [i32; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
    let quality = quality.max(5);
    let n = data.len();
    let mut w = Bits { out: vec![0x78, 0x5e], buf: 0, count: 0 };
    w.add(1, 1);
    w.add(1, 2);
    let mut table: Vec<Vec<usize>> = vec![Vec::new(); ZHASH];
    let mut i = 0usize;
    while i + 3 < n {
        let h = zhash(&data[i..]) & (ZHASH - 1);
        let mut best = 3usize;
        let mut bestloc: Option<usize> = None;
        for &p in &table[h] {
            // `hlist[j]-data > i-32768` in signed arithmetic
            if p as i64 > i as i64 - 32768 {
                let d = countm(&data[p..], &data[i..], n - i);
                if d >= best {
                    best = d;
                    bestloc = Some(p);
                }
            }
        }
        if table[h].len() == 2 * quality {
            table[h].drain(..quality);
        }
        table[h].push(i);
        if bestloc.is_some() {
            // lazy matching: a better match at the next byte turns this one into a literal
            let h2 = zhash(&data[i + 1..]) & (ZHASH - 1);
            for &p in &table[h2] {
                if p as i64 > i as i64 - 32767 {
                    let e = countm(&data[p..], &data[i + 1..], n - i - 1);
                    if e > best {
                        bestloc = None;
                        break;
                    }
                }
            }
        }
        if let Some(loc) = bestloc {
            let d = i - loc;
            let mut j = 0;
            while best > LENGTHC[j + 1] - 1 {
                j += 1;
            }
            w.huff(j as u32 + 257);
            if LENGTHEB[j] != 0 {
                w.add((best - LENGTHC[j]) as u32, LENGTHEB[j]);
            }
            let mut j = 0;
            while d > DISTC[j + 1] - 1 {
                j += 1;
            }
            w.add(Bits::rev(j as u32, 5), 5);
            if DISTEB[j] != 0 {
                w.add((d - DISTC[j]) as u32, DISTEB[j]);
            }
            i += best;
        } else {
            w.huffb(data[i] as u32);
            i += 1;
        }
    }
    while i < n {
        w.huffb(data[i] as u32);
        i += 1;
    }
    w.huff(256);
    while w.count != 0 {
        w.add(0, 1);
    }
    let (mut s1, mut s2) = (1u32, 0u32);
    let mut j = 0;
    let mut blocklen = n % 5552;
    while j < n {
        for k in 0..blocklen {
            s1 += data[j + k] as u32;
            s2 += s1;
        }
        s1 %= 65521;
        s2 %= 65521;
        j += blocklen;
        blocklen = 5552;
    }
    w.out.extend_from_slice(&[(s2 >> 8) as u8, s2 as u8, (s1 >> 8) as u8, s1 as u8]);
    w.out
}

fn paeth(a: i32, b: i32, c: i32) -> u8 {
    let p = a + b - c;
    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}

fn encode_line(px: &[u8], stride: usize, width: usize, y: usize, n: usize, filter: usize, line: &mut [i8]) {
    const MAPPING: [u8; 5] = [0, 1, 2, 3, 4];
    const FIRSTMAP: [u8; 5] = [0, 1, 0, 5, 6];
    let ty = if y != 0 { MAPPING[filter] } else { FIRSTMAP[filter] };
    let z = y * stride;
    let at = |i: isize| px[(z as isize + i) as usize] as i32;
    let up = stride as isize;
    for i in 0..n as isize {
        let v = match ty {
            0 | 1 | 5 | 6 => at(i),
            2 => at(i) - at(i - up),
            3 => at(i) - (at(i - up) >> 1),
            _ => at(i) - paeth(0, at(i - up), 0) as i32,
        };
        line[i as usize] = v as i8;
    }
    for i in n as isize..(width * n) as isize {
        let nn = n as isize;
        let v = match ty {
            0 => at(i),
            1 => at(i) - at(i - nn),
            2 => at(i) - at(i - up),
            3 => at(i) - ((at(i - nn) + at(i - up)) >> 1),
            4 => at(i) - paeth(at(i - nn), at(i - up), at(i - up - nn)) as i32,
            5 => at(i) - (at(i - nn) >> 1),
            _ => at(i) - paeth(at(i - nn), 0, 0) as i32,
        };
        line[i as usize] = v as i8;
    }
}

/// `stbi_write_png` into memory: 8-bit, `n` components (1..4: grey, grey+alpha, RGB, RGBA), stride = width * n.
pub fn write_png(pixels: &[u8], x: usize, y: usize, n: usize) -> Vec<u8> {
    let ctype = [0u8, 0, 4, 2, 6];
    let stride = x * n;
    let mut filt = vec![0u8; (stride + 1) * y];
    let mut line = vec![0i8; stride];
    for j in 0..y {
        let (mut best, mut best_val) = (0usize, i32::MAX);
        for f in 0..5 {
            encode_line(pixels, stride, x, j, n, f, &mut line);
            let est: i32 = line.iter().map(|&v| (v as i32).abs()).sum();
            if est < best_val {
                best_val = est;
                best = f;
            }
        }
        // stb re-encodes with the winner (the loop always ends on filter 4 + 1, so it always does)
        encode_line(pixels, stride, x, j, n, best, &mut line);
        filt[j * (stride + 1)] = best as u8;
        for (d, s) in filt[j * (stride + 1) + 1..(j + 1) * (stride + 1)].iter_mut().zip(&line) {
            *d = *s as u8;
        }
    }
    let z = zlib_compress(&filt, 8);
    let table = crc_table();
    let mut out = vec![137, 80, 78, 71, 13, 10, 26, 10];
    let mut chunk = |tag: &[u8; 4], body: &[u8]| {
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(tag);
        out.extend_from_slice(body);
        let c = crc32(&table, &out[start..]);
        out.extend_from_slice(&c.to_be_bytes());
    };
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(x as u32).to_be_bytes());
    ihdr.extend_from_slice(&(y as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, ctype[n], 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    out
}
