use crate::media::errors::{MediaError, Result};

pub struct G711;

impl G711 {
    // 13 位域段边界（标准 G.711 表）
    const SEG_AEND: [i16; 8] = [0x1F, 0x3F, 0x7F, 0xFF, 0x1FF, 0x3FF, 0x7FF, 0xFFF];

    fn search_segment(val: i16) -> i16 {
        for i in 0..8 {
            if val <= G711::SEG_AEND[i] {
                return i as i16;
            }
        }
        8
    }

    // ---------------- A-law ----------------

    pub fn alaw_encode(sample: i16) -> u8 {
        // 16 位 -> 13 位
        let mut pcm_val: i32 = (sample as i32) >> 3;

        let mask: u8;
        if pcm_val >= 0 {
            mask = 0xD5;
        } else {
            mask = 0x55;
            pcm_val = -pcm_val - 1;
        }

        let seg = G711::search_segment(pcm_val as i16);
        if seg >= 8 {
            return 0x7F ^ mask;
        }

        let aval = (seg as u8) << 4;
        let q = if seg < 2 {
            (pcm_val >> 1) & 0x0F
        } else {
            (pcm_val >> seg) & 0x0F
        } as u8;

        (aval | q) ^ mask
    }

    pub fn alaw_decode(code: u8) -> i16 {
        let a_val = code ^ 0x55;
        let sign_bit = a_val & 0x80;
        let seg = ((a_val & 0x70) >> 4) as i32;
        let quant = (a_val & 0x0F) as i32;

        let mut t = quant << 4;
        match seg {
            0 => t += 8,
            1 => t += 0x108,
            _ => {
                t += 0x108;
                t <<= seg - 1;
            }
        }

        if sign_bit != 0 {
            t as i16
        } else {
            -(t as i16)
        }
    }

    // ---------------- μ-law ----------------

    pub fn ulaw_encode(sample: i16) -> u8 {
        let sign = if sample < 0 { 0x80 } else { 0 };
        let mut s = if sample < 0 { !sample } else { sample };

        if s > 8159 {
            s = 8159;
        }

        s = (s + 33).clamp(1, 8159);
        let e = 7 - ((s >> 9) as i16).leading_zeros() as i16;
        let s = if e > 0 { (s >> (e + 3)) } else { s >> 4 };

        let compressed = (sign | (e << 4) | (s & 0x0F)) as u8;
        !compressed
    }

    pub fn ulaw_decode(code: u8) -> i16 {
        let inverted = !code;
        let sign = if (inverted & 0x80) != 0 { -1 } else { 1 };
        let e = ((inverted >> 4) & 0x07) as i16;
        let s = ((inverted & 0x0F) << (e + 4)) as i16;

        let decoded = if e == 0 { s - 33 } else { s + 33 * (e - 1) };

        if sign < 0 {
            -decoded
        } else {
            decoded
        }
    }

    // ---------------- slice helpers ----------------

    pub fn alaw_encode_slice(samples: &[i16]) -> Vec<u8> {
        samples.iter().map(|&s| Self::alaw_encode(s)).collect()
    }

    pub fn alaw_decode_slice(codes: &[u8]) -> Vec<i16> {
        codes.iter().map(|&c| Self::alaw_decode(c)).collect()
    }

    pub fn ulaw_encode_slice(samples: &[i16]) -> Vec<u8> {
        samples.iter().map(|&s| Self::ulaw_encode(s)).collect()
    }

    pub fn ulaw_decode_slice(codes: &[u8]) -> Vec<i16> {
        codes.iter().map(|&c| Self::ulaw_decode(c)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alaw_zero() {
        // 标准 A-law 静音编码
        assert_eq!(G711::alaw_encode(0), 0xD5);
        assert_eq!(G711::alaw_decode(0xD5), 0);
    }

    #[test]
    fn test_alaw_sign() {
        // 正数编码 bit7 = 1，负数编码 bit7 = 0
        assert!(G711::alaw_encode(1000) & 0x80 != 0);
        assert!(G711::alaw_encode(-1000) & 0x80 == 0);
    }

    #[test]
    fn test_alaw_roundtrip() {
        let samples: Vec<i16> = vec![
            -32768, -20000, -10000, -4000, -1000, -100, -1, 0, 1, 100, 1000, 4000, 10000, 20000,
            32767,
        ];
        for &s in &samples {
            let e = G711::alaw_encode(s);
            let d = G711::alaw_decode(e);
            // A-law 是对数量化，绝对误差随幅度增大；
            // 小信号误差很小，大信号误差可以到几百。
            let tol = (s.unsigned_abs() as i32 / 64).max(64);
            assert!(
                (s as i32 - d as i32).abs() <= tol,
                "s={} e=0x{:02X} d={} tol={}",
                s,
                e,
                d,
                tol
            );
        }
    }

    #[test]
    fn test_ulaw_roundtrip() {
        let samples: Vec<i16> = vec![-1000, 0, 1000, 32767, -32768];
        let encoded = G711::ulaw_encode_slice(&samples);
        let decoded = G711::ulaw_decode_slice(&encoded);

        for (orig, dec) in samples.iter().zip(decoded.iter()) {
            let tol = (orig.unsigned_abs() as i32 / 64).max(64);
            assert!(
                (orig - dec).abs() <= tol,
                "Original: {}, Decoded: {}",
                orig,
                dec
            );
        }
    }
}
