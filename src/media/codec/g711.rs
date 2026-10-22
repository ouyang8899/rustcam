
pub struct G711;

impl G711 {
    const SEG_AEND: [i16; 8] = [0x1F, 0x3F, 0x7F, 0xFF, 0x1FF, 0x3FF, 0x7FF, 0xFFF];

    fn search_segment(val: i16) -> i16 {
        for i in 0..8 {
            if val <= Self::SEG_AEND[i] {
                return i as i16;
            }
        }
        8
    }

    pub fn alaw_encode(sample: i16) -> u8 {
        let mut pcm_val: i32 = (sample as i32) >> 3;

        let mask: u8;
        if pcm_val >= 0 {
            mask = 0xD5;
        } else {
            mask = 0x55;
            pcm_val = -pcm_val - 1;
        }

        let seg = Self::search_segment(pcm_val as i16);
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

    pub fn ulaw_encode(sample: i16) -> u8 {
        const BIAS: i32 = 0x84;
        const CLIP: i32 = 32635;

        let mut s = sample as i32;
        let sign;
        if s < 0 {
            s = -s;
            sign = 0x80u8;
        } else {
            sign = 0u8;
        }
        if s > CLIP {
            s = CLIP;
        }
        s += BIAS;

        let seg = {
            let mut seg = 0;
            let mut mask = 0x4000;
            while seg < 8 && (s & mask) == 0 {
                seg += 1;
                mask >>= 1;
            }
            7 - seg
        };

        let step = ((s >> (seg + 3)) & 0x0F) as u8;
        !(sign | ((seg as u8) << 4) | step)
    }

    pub fn ulaw_decode(code: u8) -> i16 {
        let u = !code;
        let t = (((u & 0x0F) as i32) << 3) + 0x84;
        let seg = ((u >> 4) & 0x07) as i32;
        let mut t = t << seg;
        t -= 0x84;
        if u & 0x80 != 0 {
            (-t) as i16
        } else {
            t as i16
        }
    }

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
    fn test_alaw_roundtrip() {
        let samples: Vec<i16> = vec![
            -32768, -20000, -10000, -4000, -1000, -100, -1, 0, 1, 100, 1000, 4000, 10000, 20000,
            32767,
        ];
        for &s in &samples {
            let e = G711::alaw_encode(s);
            let d = G711::alaw_decode(e);

            let orig_abs = s.unsigned_abs() as i32;
            let tol = if orig_abs == 0 {
                16
            } else {
                (orig_abs / 2).max(1024)
            };
            assert!(
                (s as i32 - d as i32).abs() <= tol,
                "A-law: s={} e=0x{:02X} d={} tol={}",
                s,
                e,
                d,
                tol
            );
        }
    }

    #[test]
    fn test_ulaw_roundtrip() {
        let samples: Vec<i16> = vec![
            -32768, -20000, -10000, -4000, -1000, -100, -1, 0, 1, 100, 1000, 4000, 10000, 20000,
            32767,
        ];
        for &s in &samples {
            let e = G711::ulaw_encode(s);
            let d = G711::ulaw_decode(e);
            let orig_abs = s.unsigned_abs() as i32;

            let tol = if orig_abs == 0 { 32 } else { (orig_abs * 7 / 10).max(4096) };
            assert!(
                (s as i32 - d as i32).abs() <= tol,
                "μ-law: s={} e=0x{:02X} d={} tol={}",
                s, e, d, tol
            );
        }
    }

    #[test]
    fn test_alaw_encode_decode() {

        assert_eq!(G711::alaw_encode(1000) & 0x80, 0x80);
        assert_eq!(G711::alaw_encode(-1000) & 0x80, 0x00);
    }

    #[test]
    fn test_ulaw_encode_decode() {

        assert_eq!(G711::ulaw_encode(0) & 0x80, 0x80);
        assert_eq!(G711::ulaw_encode(-1000) & 0x80, 0x00);
        assert_eq!(G711::ulaw_encode(1000) & 0x80, 0x80);
    }

    #[test]
    fn test_ulaw_standard_vectors() {

        assert_eq!(G711::ulaw_encode(0), 0xFF);
        assert_eq!(G711::ulaw_encode(i16::MAX), 0x80);
        assert_eq!(G711::ulaw_encode(i16::MIN), 0x00);
    }
}
