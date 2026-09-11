use crate::media::errors::{MediaError, Result};

pub struct G711;

impl G711 {
    const SIGN_BIT: i16 = 0x80;
    const QUANT_MASK: i16 = 0x0F;
    const SEG_SHIFT: i16 = 4;
    const SEG_MASK: i16 = 0x70;

    const SEG_END: [i16; 8] = [0xFF, 0x1FF, 0x3FF, 0x7FF, 0xFFF, 0x1FFF, 0x3FFF, 0x7FFF];

    fn search_segment(val: i16) -> i16 {
        let mut r#try = 0;
        if val < 0 {
            let negated = !val;
            if negated >= G711::SEG_END[1] {
                r#try |= 0x08;
            }
            if negated >= G711::SEG_END[3] {
                r#try |= 0x04;
            }
            if negated >= G711::SEG_END[5] {
                r#try |= 0x02;
            }
            if negated >= G711::SEG_END[7] {
                r#try |= 0x01;
            }
        }
        r#try
    }

    pub fn alaw_encode(sample: i16) -> u8 {
        let sign = if sample < 0 { 0x80 } else { 0 };
        let mut s = if sample < 0 { !sample } else { sample };

        if s > 8159 {
            s = 8159;
        }

        let segment = G711::search_segment(s);
        s = s >> (segment + 3);

        let compressed = if segment >= 8 {
            127
        } else {
            ((segment as u8) << 4) | ((s & 0x0F) as u8)
        };

        if sign != 0 {
            compressed ^ 0x55
        } else {
            compressed
        }
    }

    pub fn alaw_decode(code: u8) -> i16 {
        let inverted = code ^ 0x55;
        let sign = if (inverted & 0x80) != 0 { -1 } else { 1 };
        let mut s = (inverted & 0x7F) as i16;

        let segment = s >> 4;
        let s = if segment == 0 {
            (s << 1) | 1
        } else {
            ((s & 0x0F) << (segment + 3)) + (G711::SEG_END[(segment - 1) as usize] + 1)
        };

        if sign < 0 {
            -s
        } else {
            s
        }
    }

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

        let mut decoded = if e == 0 { s - 33 } else { s + 33 * (e - 1) };

        if sign < 0 {
            -decoded
        } else {
            decoded
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
        let samples: Vec<i16> = vec![-1000, 0, 1000, 32767, -32768];
        let encoded = G711::alaw_encode_slice(&samples);
        let decoded = G711::alaw_decode_slice(&encoded);

        for (orig, dec) in samples.iter().zip(decoded.iter()) {
            assert!(
                (orig - dec).abs() <= 2,
                "Original: {}, Decoded: {}",
                orig,
                dec
            );
        }
    }

    #[test]
    fn test_ulaw_roundtrip() {
        let samples: Vec<i16> = vec![-1000, 0, 1000, 32767, -32768];
        let encoded = G711::ulaw_encode_slice(&samples);
        let decoded = G711::ulaw_decode_slice(&encoded);

        for (orig, dec) in samples.iter().zip(decoded.iter()) {
            assert!(
                (orig - dec).abs() <= 2,
                "Original: {}, Decoded: {}",
                orig,
                dec
            );
        }
    }
}
