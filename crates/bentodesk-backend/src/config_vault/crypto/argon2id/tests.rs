use super::*;

/// Helper to convert a hex string to bytes. Internal-only (test code).
fn hex(s: &str) -> Vec<u8> {
    let s = s.replace([' ', '\n'], "");
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let hi = ascii_hex_digit(bytes[i]);
        let lo = ascii_hex_digit(bytes[i + 1]);
        out.push((hi << 4) | lo);
        i += 2;
    }
    out
}

fn ascii_hex_digit(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

#[test]
fn blake2b_empty_input_matches_known_vector() {
    // RFC 7693 appendix A — BLAKE2b("") = ...
    let got = blake2b(64, b"");
    let expected = hex(
        "786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419\
         d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce",
    );
    assert_eq!(got, expected);
}

#[test]
fn blake2b_abc_matches_known_vector() {
    // RFC 7693 appendix A — BLAKE2b("abc") = ...
    let got = blake2b(64, b"abc");
    let expected = hex(
        "ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d1\
         7d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923",
    );
    assert_eq!(got, expected);
}

#[test]
fn argon2id_kdf_is_deterministic() {
    // Same inputs → same key. This is the contract the vault depends on.
    let p = Argon2Params {
        m_kib: 64, // small for test speed (still ≥ 8*p where p=1)
        iterations: 1,
        parallelism: 1,
        tag_len: 32,
    };
    let a = argon2id(b"hunter2", b"NaClNaClNaClNaCl", p).unwrap();
    let b = argon2id(b"hunter2", b"NaClNaClNaClNaCl", p).unwrap();
    assert_eq!(a, b, "argon2id must be deterministic");
    assert_eq!(a.len(), 32);
}

#[test]
fn argon2id_different_passwords_produce_different_keys() {
    let p = Argon2Params {
        m_kib: 64,
        iterations: 1,
        parallelism: 1,
        tag_len: 32,
    };
    let a = argon2id(b"correct horse", b"saltsaltsaltsalt", p).unwrap();
    let b = argon2id(b"battery staple", b"saltsaltsaltsalt", p).unwrap();
    assert_ne!(a, b);
}

#[test]
fn argon2id_different_salts_produce_different_keys() {
    let p = Argon2Params {
        m_kib: 64,
        iterations: 1,
        parallelism: 1,
        tag_len: 32,
    };
    let a = argon2id(b"hunter2", b"saltAsaltAsaltAA", p).unwrap();
    let b = argon2id(b"hunter2", b"saltBsaltBsaltBB", p).unwrap();
    assert_ne!(a, b);
}

#[test]
fn argon2id_rejects_zero_tag_len() {
    let p = Argon2Params {
        m_kib: 64,
        iterations: 1,
        parallelism: 1,
        tag_len: 0,
    };
    assert_eq!(
        argon2id(b"x", b"y", p),
        Err(Argon2Error::InvalidTagLen { tag_len: 0 })
    );
}

#[test]
fn argon2id_rejects_zero_iterations() {
    let p = Argon2Params {
        m_kib: 64,
        iterations: 0,
        parallelism: 1,
        tag_len: 32,
    };
    assert_eq!(argon2id(b"x", b"y", p), Err(Argon2Error::IterationsZero));
}

#[test]
fn argon2id_rejects_zero_parallelism() {
    let p = Argon2Params {
        m_kib: 64,
        iterations: 1,
        parallelism: 0,
        tag_len: 32,
    };
    assert_eq!(argon2id(b"x", b"y", p), Err(Argon2Error::ParallelismZero));
}

#[test]
fn argon2id_rejects_too_small_memory() {
    let p = Argon2Params {
        m_kib: 4, // requires ≥ 8 * p = 16
        iterations: 1,
        parallelism: 2,
        tag_len: 32,
    };
    assert_eq!(
        argon2id(b"x", b"y", p),
        Err(Argon2Error::MemoryTooSmall {
            m_kib: 4,
            parallelism: 2
        })
    );
}

#[test]
fn argon2id_default_params_produce_32_byte_key() {
    // Production parameters: 64 MiB / t=3 / p=4. Slow (~250ms). We still
    // run it once in the test suite to ensure the configured parameters
    // work end-to-end, not just the toy reduction.
    let p = Argon2Params::DEFAULT;
    let mut salt = [0u8; 16];
    for (i, b) in salt.iter_mut().enumerate() {
        *b = i as u8;
    }
    let key = argon2id(b"correct horse battery staple", &salt, p).unwrap();
    assert_eq!(key.len(), 32);
    // Determinism with the production params, too.
    let key2 = argon2id(b"correct horse battery staple", &salt, p).unwrap();
    assert_eq!(key, key2);
}
