//! 敏感配置的静态加密（应用层）。
//!
//! 签名密钥在库里按密文存放，主密钥来自环境变量，不入库（drone.md §9.1 / §12.4）。
//! 加密算法：AES-256-GCM（ring），密文格式 `v1:<hex(nonce||ciphertext)>`。

use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::rand::{SecureRandom, SystemRandom};

const PREFIX: &str = "v1:";
const KEY_LEN: usize = 32;

/// 主密钥加密器；未配置主密钥时不做加密（由调用方决定跳过）。
#[derive(Clone)]
pub struct SecretBox {
    key: LessSafeKey,
}

impl SecretBox {
    /// 主密钥必须是 32 字节（64 位十六进制）。
    pub fn from_hex(hex: &str) -> anyhow::Result<SecretBox> {
        let raw = hex_decode(hex.trim())?;
        if raw.len() != KEY_LEN {
            anyhow::bail!("signing master key must be {KEY_LEN} bytes");
        }
        let unbound =
            UnboundKey::new(&AES_256_GCM, &raw).map_err(|e| anyhow::anyhow!("invalid master key: {e}"))?;
        Ok(SecretBox {
            key: LessSafeKey::new(unbound),
        })
    }

    /// 加密为 `v1:<hex(nonce||ciphertext)>`；空串原样返回空串。
    pub fn seal(&self, plaintext: &str) -> anyhow::Result<String> {
        if plaintext.is_empty() {
            return Ok(String::new());
        }
        let nonce = nonce_bytes();
        let mut in_out = plaintext.as_bytes().to_vec();
        self.key
            .seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::empty(), &mut in_out)
            .map_err(|_| anyhow::anyhow!("seal failed"))?;
        let mut blob = nonce.to_vec();
        blob.extend_from_slice(&in_out);
        Ok(format!("{PREFIX}{}", hex_encode(&blob)))
    }

    /// 解密 `v1:` 前缀密文；非 `v1:` 前缀按明文处理（兼容未加密的存量值）。
    pub fn open(&self, sealed: &str) -> anyhow::Result<String> {
        if sealed.is_empty() {
            return Ok(String::new());
        }
        let Some(body) = sealed.strip_prefix(PREFIX) else {
            return Ok(sealed.to_string());
        };
        let blob = hex_decode(body)?;
        if blob.len() <= NONCE_LEN {
            anyhow::bail!("sealed secret too short");
        }
        let (nonce, ciphertext) = blob.split_at(NONCE_LEN);
        let mut in_out = ciphertext.to_vec();
        let plaintext = self
            .key
            .open_in_place(
                Nonce::assume_unique_for_key(nonce.try_into().map_err(|_| anyhow::anyhow!("bad nonce"))?),
                Aad::empty(),
                &mut in_out,
            )
            .map_err(|_| anyhow::anyhow!("open failed (wrong master key?)"))?;
        Ok(String::from_utf8(plaintext.to_vec())?)
    }
}

fn nonce_bytes() -> [u8; NONCE_LEN] {
    let mut buf = [0u8; NONCE_LEN];
    SystemRandom::new()
        .fill(&mut buf)
        .expect("system rng must be available");
    buf
}

fn hex_decode(s: &str) -> anyhow::Result<Vec<u8>> {
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    anyhow::ensure!(cleaned.len() % 2 == 0, "hex string must have even length");
    let bytes = cleaned.as_bytes();
    let mut out = Vec::with_capacity(cleaned.len() / 2);
    for pair in bytes.chunks(2) {
        let hi = (pair[0] as char)
            .to_digit(16)
            .ok_or_else(|| anyhow::anyhow!("invalid hex digit"))?;
        let lo = (pair[1] as char)
            .to_digit(16)
            .ok_or_else(|| anyhow::anyhow!("invalid hex digit"))?;
        out.push(((hi << 4) | lo) as u8);
    }
    Ok(out)
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const MASTER: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    #[test]
    fn seal_and_open_round_trip() {
        let sb = SecretBox::from_hex(MASTER).expect("master key");
        let sealed = sb.seal("aabbccddeeff00112233445566778899").expect("seal");
        assert!(sealed.starts_with("v1:"));
        assert!(!sealed.contains("aabbccddeeff"));
        let opened = sb.open(&sealed).expect("open");
        assert_eq!(opened, "aabbccddeeff00112233445566778899");
    }

    #[test]
    fn empty_stays_empty() {
        let sb = SecretBox::from_hex(MASTER).expect("master key");
        assert_eq!(sb.seal("").expect("seal"), "");
        assert_eq!(sb.open("").expect("open"), "");
    }

    #[test]
    fn wrong_master_key_fails_to_open() {
        let sb = SecretBox::from_hex(MASTER).expect("master key");
        let sealed = sb.seal("secret").expect("seal");
        let other = SecretBox::from_hex(&"ff".repeat(32)).expect("master key");
        assert!(other.open(&sealed).is_err());
    }

    #[test]
    fn master_key_must_be_32_bytes() {
        assert!(SecretBox::from_hex("0011").is_err());
    }

    #[test]
    fn plaintext_without_prefix_is_returned_as_is() {
        let sb = SecretBox::from_hex(MASTER).expect("master key");
        assert_eq!(
            sb.open("legacy-plaintext").expect("open"),
            "legacy-plaintext"
        );
    }
}
