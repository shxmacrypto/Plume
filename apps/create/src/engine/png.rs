//! PNG for the pictures kept inside a project file: RGBA or grey, 8 bits. Written here (zlib by miniz_oxide) so a
//! project opens the same on any PC, and read back (all five row filters; no interlacing, as written).

use super::img::{Gray, Img};

fn crc_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    for (n, v) in t.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *v = c;
    }
    t
}

fn crc(t: &[u32; 256], parts: &[&[u8]]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for p in parts {
        for b in *p {
            c = t[((c ^ *b as u32) & 0xff) as usize] ^ (c >> 8);
        }
    }
    c ^ 0xffff_ffff
}

fn chunk(out: &mut Vec<u8>, t: &[u32; 256], kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc(t, &[kind, data]).to_be_bytes());
}

fn encode(w: u32, h: u32, channels: usize, data: &[u8]) -> Vec<u8> {
    let t = crc_table();
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, if channels == 4 { 6 } else { 0 }, 0, 0, 0]);
    chunk(&mut out, &t, b"IHDR", &ihdr);
    // each row with the "up" filter (photos and flat areas both shrink well)
    let stride = w as usize * channels;
    let mut raw = Vec::with_capacity((stride + 1) * h as usize);
    for y in 0..h as usize {
        raw.push(2);
        let row = &data[y * stride..(y + 1) * stride];
        if y == 0 {
            raw.extend_from_slice(row);
        } else {
            let prev = &data[(y - 1) * stride..y * stride];
            raw.extend(row.iter().zip(prev).map(|(a, b)| a.wrapping_sub(*b)));
        }
    }
    let z = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);
    chunk(&mut out, &t, b"IDAT", &z);
    chunk(&mut out, &t, b"IEND", &[]);
    out
}

pub fn encode_rgba(img: &Img) -> Vec<u8> {
    encode(img.w, img.h, 4, &img.data)
}

pub fn encode_gray(g: &Gray) -> Vec<u8> {
    encode(g.w, g.h, 1, &g.data)
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i16 + b as i16 - c as i16;
    let (pa, pb, pc) = ((p - a as i16).abs(), (p - b as i16).abs(), (p - c as i16).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// (width, height, channels, pixels). Only 8-bit grey (1), grey+alpha (2), RGB (3) and RGBA (4).
pub fn decode(bytes: &[u8]) -> Option<(u32, u32, usize, Vec<u8>)> {
    if bytes.len() < 8 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let mut at = 8;
    let (mut w, mut h, mut ch) = (0u32, 0u32, 0usize);
    let mut idat = vec![];
    while at + 8 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[at..at + 4].try_into().ok()?) as usize;
        let kind = &bytes[at + 4..at + 8];
        let data = bytes.get(at + 8..at + 8 + len)?;
        match kind {
            b"IHDR" => {
                w = u32::from_be_bytes(data[0..4].try_into().ok()?);
                h = u32::from_be_bytes(data[4..8].try_into().ok()?);
                if data[8] != 8 || data[12] != 0 {
                    return None;
                }
                ch = match data[9] {
                    0 => 1,
                    4 => 2,
                    2 => 3,
                    6 => 4,
                    _ => return None,
                };
            }
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => break,
            _ => {}
        }
        at += 12 + len;
    }
    if w == 0 || h == 0 || ch == 0 {
        return None;
    }
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&idat).ok()?;
    let stride = w as usize * ch;
    if raw.len() < (stride + 1) * h as usize {
        return None;
    }
    let mut out = vec![0u8; stride * h as usize];
    for y in 0..h as usize {
        let f = raw[y * (stride + 1)];
        let src = &raw[y * (stride + 1) + 1..(y + 1) * (stride + 1)];
        for x in 0..stride {
            let a = if x >= ch { out[y * stride + x - ch] } else { 0 };
            let b = if y > 0 { out[(y - 1) * stride + x] } else { 0 };
            let c = if y > 0 && x >= ch { out[(y - 1) * stride + x - ch] } else { 0 };
            let v = src[x];
            out[y * stride + x] = match f {
                0 => v,
                1 => v.wrapping_add(a),
                2 => v.wrapping_add(b),
                3 => v.wrapping_add(((a as u16 + b as u16) / 2) as u8),
                4 => v.wrapping_add(paeth(a, b, c)),
                _ => return None,
            };
        }
    }
    Some((w, h, ch, out))
}

pub fn decode_rgba(bytes: &[u8]) -> Option<Img> {
    let (w, h, ch, px) = decode(bytes)?;
    let mut img = Img::new(w, h);
    for (d, s) in img.data.as_chunks_mut::<4>().0.iter_mut().zip(px.chunks_exact(ch)) {
        match ch {
            1 => d.copy_from_slice(&[s[0], s[0], s[0], 255]),
            2 => d.copy_from_slice(&[s[0], s[0], s[0], s[1]]),
            3 => d.copy_from_slice(&[s[0], s[1], s[2], 255]),
            _ => d.copy_from_slice(s),
        }
    }
    Some(img)
}

pub fn decode_gray(bytes: &[u8]) -> Option<Gray> {
    let (w, h, ch, px) = decode(bytes)?;
    Some(Gray { w, h, data: px.chunks_exact(ch).map(|s| s[0]).collect() })
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            if k <= c.len() {
                out.push(B64[(n >> (18 - 6 * k)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub fn unbase64(s: &str) -> Option<Vec<u8>> {
    let mut val = [255u8; 256];
    for (i, c) in B64.iter().enumerate() {
        val[*c as usize] = i as u8;
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut acc = 0u32;
    let mut bits = 0;
    for b in s.bytes() {
        if b == b'=' || b.is_ascii_whitespace() {
            continue;
        }
        let v = val[b as usize];
        if v == 255 {
            return None;
        }
        acc = acc << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let mut img = Img::new(17, 9);
        for y in 0..9 {
            for x in 0..17 {
                img.set(x, y, [(x * 15) as u8, (y * 28) as u8, ((x * y) % 256) as u8, (255 - x * 3) as u8]);
            }
        }
        let png = encode_rgba(&img);
        assert_eq!(decode_rgba(&png), Some(img));
        let g = Gray { w: 5, h: 3, data: (0..15).map(|v| v * 17).collect() };
        assert_eq!(decode_gray(&encode_gray(&g)), Some(g));
        for s in ["", "a", "ab", "abc", "abcd", "hello world!"] {
            assert_eq!(unbase64(&base64(s.as_bytes())).unwrap(), s.as_bytes());
        }
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert!(decode(b"not a png").is_none());
    }
}
