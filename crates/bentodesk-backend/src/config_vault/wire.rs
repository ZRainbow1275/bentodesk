//! On-disk wire format for `settings.vault`.
//!
//! The file is a single JSON object: `{ version, mode, salt, nonce, tag,
//! ciphertext }`. Byte fields (`salt`, `nonce`, `tag`, `ciphertext`) are
//! encoded as standard (RFC 4648 §4) base64 strings.
//!
//! Why base64-in-JSON instead of CBOR / a binary format:
//! - serde_json is already in §8 whitelist; CBOR is not.
//! - Atomic-write helpers in `bentodesk-backend::storage` are JSON-shaped.
//! - A 32 KiB ciphertext base64-encodes to ~43 KiB; the size penalty is
//!   negligible relative to the safety bound (`MAX_JSON_STATE_BYTES = 128 MiB`).
//!
//! Spec §8 forbids the `base64` crate; the encoder/decoder is hand-rolled
//! at the bottom of this file (~80 LOC, RFC 4648 strict).

use serde::{Deserialize, Serialize};

/// Wire-format version number. Increment on any byte-layout change.
pub const VAULT_VERSION: u8 = 1;

/// One encryption mode tag. Stored as `u8` on disk so the format never
/// depends on serde enum-tag spelling.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeTag {
    None = 0,
    Dpapi = 1,
    Passphrase = 2,
}

impl ModeTag {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::None),
            1 => Some(Self::Dpapi),
            2 => Some(Self::Passphrase),
            _ => None,
        }
    }
}

/// On-disk record persisted at `<state_dir>/settings.vault`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaultRecord {
    pub version: u8,
    /// `0` = None, `1` = DPAPI, `2` = Passphrase. Numeric for forward-compat.
    pub mode_tag: u8,
    /// 16-byte Argon2id salt (base64). Empty for `None` / `Dpapi` modes.
    pub salt_b64: String,
    /// 12-byte AES-GCM nonce (base64). Empty for `None` mode.
    /// For `Dpapi` mode this is a sentinel (DPAPI manages its own IV).
    pub nonce_b64: String,
    /// 16-byte AES-GCM auth tag (base64). Empty for `None` mode.
    /// For `Dpapi` mode this is a sentinel (DPAPI carries its own MAC).
    pub tag_b64: String,
    /// Ciphertext bytes (base64). For `None` mode this is the plaintext JSON.
    /// For `Dpapi` mode this is the opaque DPAPI blob.
    /// For `Passphrase` mode this is the AES-GCM ciphertext of the inner JSON.
    pub ciphertext_b64: String,
}

impl VaultRecord {
    /// Construct a `None`-mode record carrying plaintext JSON bytes.
    pub fn plaintext(json_bytes: &[u8]) -> Self {
        Self {
            version: VAULT_VERSION,
            mode_tag: ModeTag::None as u8,
            salt_b64: String::new(),
            nonce_b64: String::new(),
            tag_b64: String::new(),
            ciphertext_b64: base64_encode(json_bytes),
        }
    }

    /// Construct a `Dpapi`-mode record carrying the opaque DPAPI blob.
    pub fn dpapi(blob: &[u8]) -> Self {
        Self {
            version: VAULT_VERSION,
            mode_tag: ModeTag::Dpapi as u8,
            salt_b64: String::new(),
            nonce_b64: String::new(),
            tag_b64: String::new(),
            ciphertext_b64: base64_encode(blob),
        }
    }

    /// Construct a `Passphrase`-mode record from the AES-GCM ciphertext + tag
    /// + nonce + Argon2id salt.
    pub fn passphrase(salt: &[u8], nonce: &[u8], tag: &[u8], ciphertext: &[u8]) -> Self {
        Self {
            version: VAULT_VERSION,
            mode_tag: ModeTag::Passphrase as u8,
            salt_b64: base64_encode(salt),
            nonce_b64: base64_encode(nonce),
            tag_b64: base64_encode(tag),
            ciphertext_b64: base64_encode(ciphertext),
        }
    }

    /// Decode the base64-encoded byte fields, returning a typed view.
    pub fn decoded_bytes(&self) -> Result<DecodedRecord, WireError> {
        Ok(DecodedRecord {
            version: self.version,
            mode_tag: ModeTag::from_u8(self.mode_tag)
                .ok_or(WireError::UnknownModeTag { tag: self.mode_tag })?,
            salt: base64_decode(&self.salt_b64).map_err(|e| WireError::Base64 {
                field: "salt",
                inner: e,
            })?,
            nonce: base64_decode(&self.nonce_b64).map_err(|e| WireError::Base64 {
                field: "nonce",
                inner: e,
            })?,
            tag: base64_decode(&self.tag_b64).map_err(|e| WireError::Base64 {
                field: "tag",
                inner: e,
            })?,
            ciphertext: base64_decode(&self.ciphertext_b64).map_err(|e| WireError::Base64 {
                field: "ciphertext",
                inner: e,
            })?,
        })
    }
}

/// Decoded view of a [`VaultRecord`] with byte fields materialised.
#[derive(Debug, Clone)]
pub struct DecodedRecord {
    pub version: u8,
    pub mode_tag: ModeTag,
    pub salt: Vec<u8>,
    pub nonce: Vec<u8>,
    pub tag: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

/// Errors surfaced by the wire-format helpers.
#[derive(Debug)]
pub enum WireError {
    /// `mode_tag` was not 0/1/2.
    UnknownModeTag { tag: u8 },
    /// One of the base64 fields failed to decode.
    Base64 {
        field: &'static str,
        inner: Base64Error,
    },
}

impl core::fmt::Display for WireError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownModeTag { tag } => write!(f, "unknown mode tag: {tag}"),
            Self::Base64 { field, inner } => write!(f, "base64 decode failed for {field}: {inner}"),
        }
    }
}

impl core::error::Error for WireError {}

// ─── Base64 (RFC 4648 §4 standard alphabet) — hand-rolled ───────────

const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Decode error.
#[derive(Debug, PartialEq, Eq)]
pub enum Base64Error {
    /// Length was not a multiple of 4 (after padding).
    InvalidLength { len: usize },
    /// Encountered a character outside the alphabet.
    InvalidByte { at: usize, byte: u8 },
    /// Padding occurred before the end of the string.
    PaddingMidStream { at: usize },
}

impl core::fmt::Display for Base64Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidLength { len } => write!(f, "base64 length {len} not multiple of 4"),
            Self::InvalidByte { at, byte } => write!(f, "base64 invalid byte at {at}: {byte:#x}"),
            Self::PaddingMidStream { at } => write!(f, "base64 padding mid-stream at {at}"),
        }
    }
}

impl core::error::Error for Base64Error {}

/// Encode bytes to standard base64 (with `=` padding).
pub fn base64_encode(input: &[u8]) -> String {
    if input.is_empty() {
        return String::new();
    }
    let out_len = input.len().div_ceil(3) * 4;
    let mut out = String::with_capacity(out_len);
    let mut i = 0;
    while i + 3 <= input.len() {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8) | (input[i + 2] as u32);
        out.push(B64_ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(B64_ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        out.push(B64_ALPHABET[((n >> 6) & 0x3f) as usize] as char);
        out.push(B64_ALPHABET[(n & 0x3f) as usize] as char);
        i += 3;
    }
    let rem = input.len() - i;
    match rem {
        0 => {}
        1 => {
            let n = (input[i] as u32) << 16;
            out.push(B64_ALPHABET[((n >> 18) & 0x3f) as usize] as char);
            out.push(B64_ALPHABET[((n >> 12) & 0x3f) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
            out.push(B64_ALPHABET[((n >> 18) & 0x3f) as usize] as char);
            out.push(B64_ALPHABET[((n >> 12) & 0x3f) as usize] as char);
            out.push(B64_ALPHABET[((n >> 6) & 0x3f) as usize] as char);
            out.push('=');
        }
        _ => unreachable!("rem in 0..=2"),
    }
    out
}

/// Decode a standard base64 string with `=` padding.
pub fn base64_decode(input: &str) -> Result<Vec<u8>, Base64Error> {
    if input.is_empty() {
        return Ok(Vec::new());
    }
    let bytes = input.as_bytes();
    if bytes.len() % 4 != 0 {
        return Err(Base64Error::InvalidLength { len: bytes.len() });
    }

    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let mut i = 0;
    while i < bytes.len() {
        let mut quad = [0u32; 4];
        let mut pad = 0u32;
        for j in 0..4 {
            let b = bytes[i + j];
            if b == b'=' {
                pad += 1;
                quad[j] = 0;
            } else {
                let decoded =
                    decode_char(b).ok_or(Base64Error::InvalidByte { at: i + j, byte: b })?;
                if pad != 0 {
                    return Err(Base64Error::PaddingMidStream { at: i + j });
                }
                quad[j] = decoded as u32;
            }
        }
        let n = (quad[0] << 18) | (quad[1] << 12) | (quad[2] << 6) | quad[3];
        out.push(((n >> 16) & 0xff) as u8);
        if pad < 2 {
            out.push(((n >> 8) & 0xff) as u8);
        }
        if pad < 1 {
            out.push((n & 0xff) as u8);
        }
        i += 4;
    }
    Ok(out)
}

fn decode_char(b: u8) -> Option<u8> {
    match b {
        b'A'..=b'Z' => Some(b - b'A'),
        b'a'..=b'z' => Some(b - b'a' + 26),
        b'0'..=b'9' => Some(b - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trip_empty() {
        assert_eq!(base64_encode(&[]), "");
        assert_eq!(base64_decode("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn base64_known_vectors() {
        // RFC 4648 §10 examples.
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");

        assert_eq!(base64_decode("Zg==").unwrap(), b"f");
        assert_eq!(base64_decode("Zm8=").unwrap(), b"fo");
        assert_eq!(base64_decode("Zm9v").unwrap(), b"foo");
        assert_eq!(base64_decode("Zm9vYg==").unwrap(), b"foob");
        assert_eq!(base64_decode("Zm9vYmE=").unwrap(), b"fooba");
        assert_eq!(base64_decode("Zm9vYmFy").unwrap(), b"foobar");
    }

    #[test]
    fn base64_round_trip_random_bytes() {
        let input: Vec<u8> = (0u8..=255).collect();
        let encoded = base64_encode(&input);
        let decoded = base64_decode(&encoded).expect("decode");
        assert_eq!(decoded, input);
    }

    #[test]
    fn base64_rejects_invalid_byte() {
        assert!(matches!(
            base64_decode("Zg=*"),
            Err(Base64Error::InvalidByte { .. })
        ));
    }

    #[test]
    fn base64_rejects_bad_length() {
        assert!(matches!(
            base64_decode("abc"),
            Err(Base64Error::InvalidLength { .. })
        ));
    }

    #[test]
    fn vault_record_plaintext_round_trip() {
        let r = VaultRecord::plaintext(b"{}");
        let json = serde_json::to_string(&r).expect("encode");
        let r2: VaultRecord = serde_json::from_str(&json).expect("decode");
        assert_eq!(r, r2);
        assert_eq!(r2.decoded_bytes().unwrap().ciphertext, b"{}");
        assert_eq!(r2.decoded_bytes().unwrap().mode_tag, ModeTag::None);
    }

    #[test]
    fn vault_record_dpapi_round_trip() {
        let r = VaultRecord::dpapi(&[0xde, 0xad, 0xbe, 0xef]);
        let json = serde_json::to_string(&r).expect("encode");
        let r2: VaultRecord = serde_json::from_str(&json).expect("decode");
        assert_eq!(r, r2);
        let dec = r2.decoded_bytes().unwrap();
        assert_eq!(dec.mode_tag, ModeTag::Dpapi);
        assert_eq!(dec.ciphertext, vec![0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn vault_record_passphrase_round_trip() {
        let salt = [0x11u8; 16];
        let nonce = [0x22u8; 12];
        let tag = [0x33u8; 16];
        let ciphertext = vec![0x44u8; 64];
        let r = VaultRecord::passphrase(&salt, &nonce, &tag, &ciphertext);
        let json = serde_json::to_string(&r).expect("encode");
        let r2: VaultRecord = serde_json::from_str(&json).expect("decode");
        let dec = r2.decoded_bytes().unwrap();
        assert_eq!(dec.mode_tag, ModeTag::Passphrase);
        assert_eq!(dec.salt, salt.to_vec());
        assert_eq!(dec.nonce, nonce.to_vec());
        assert_eq!(dec.tag, tag.to_vec());
        assert_eq!(dec.ciphertext, ciphertext);
    }

    #[test]
    fn unknown_mode_tag_is_rejected() {
        let r = VaultRecord {
            version: 1,
            mode_tag: 99,
            salt_b64: String::new(),
            nonce_b64: String::new(),
            tag_b64: String::new(),
            ciphertext_b64: String::new(),
        };
        assert!(matches!(
            r.decoded_bytes(),
            Err(WireError::UnknownModeTag { tag: 99 })
        ));
    }
}
