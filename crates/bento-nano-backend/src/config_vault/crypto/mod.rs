//! Cryptographic primitives backing `config_vault`.
//!
//! All three submodules satisfy the Q2=C ruling: zero new crates added to
//! the §8 whitelist; everything rides on `windows-sys` BCrypt / DPAPI bindings
//! plus a hand-rolled RFC 9106 Argon2id KDF.

pub mod aes_gcm;
pub mod argon2id;
pub mod dpapi;
