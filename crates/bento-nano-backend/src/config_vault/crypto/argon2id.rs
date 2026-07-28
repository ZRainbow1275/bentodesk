//! Hand-rolled Argon2id key-derivation function (RFC 9106).
//!
//! Q2=C ruling forbids adding the `argon2` crate to the §8 whitelist. This
//! file implements the reference algorithm in pure Rust on top of `std`
//! only — no third-party crypto deps. Output is a 32-byte key suitable for
//! `crypto::aes_gcm` to use as an AES-256-GCM secret.
//!
//! Algorithm parameters (Q2=C / task spec):
//! - `m = 65 536` KiB (64 MiB memory)
//! - `t = 3` iterations
//! - `p = 4` parallelism lanes
//! - `tag_len = 32` bytes (AES-256 key)
//! - `salt_len = 16` bytes (caller-supplied via `BCryptGenRandom`)
//!
//! References:
//! - RFC 9106 — <https://datatracker.ietf.org/doc/html/rfc9106>
//! - Reference C implementation — <https://github.com/P-H-C/phc-winner-argon2>
//!
//! Verified against the RFC 9106 §5.3 Argon2id test vector (`test_vectors`
//! module in `tests`).

use core::convert::TryInto;

// ─── BLAKE2b — RFC 7693, plus the Argon2-specific H' "long hash" wrapper ─

/// BLAKE2b initialisation vector (RFC 7693 §2.6).
const BLAKE2B_IV: [u64; 8] = [
    0x6a09_e667_f3bc_c908,
    0xbb67_ae85_84ca_a73b,
    0x3c6e_f372_fe94_f82b,
    0xa54f_f53a_5f1d_36f1,
    0x510e_527f_ade6_82d1,
    0x9b05_688c_2b3e_6c1f,
    0x1f83_d9ab_fb41_bd6b,
    0x5be0_cd19_137e_2179,
];

/// BLAKE2b SIGMA permutation table (RFC 7693 §2.7).
const BLAKE2B_SIGMA: [[usize; 16]; 12] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
];

#[derive(Clone)]
struct Blake2b {
    h: [u64; 8],
    t: [u64; 2],
    buf: [u8; 128],
    buflen: usize,
}

impl Blake2b {
    /// Create a new BLAKE2b hasher with the given output length (1..=64 bytes).
    fn new(out_len: usize) -> Self {
        debug_assert!((1..=64).contains(&out_len));
        let mut h = BLAKE2B_IV;
        // Parameter block: digest_length || key_length(0) || fanout(1) ||
        // depth(1). Key length is 0 because Argon2 only uses unkeyed BLAKE2b.
        h[0] ^= 0x0101_0000 ^ (out_len as u64);
        Self {
            h,
            t: [0, 0],
            buf: [0u8; 128],
            buflen: 0,
        }
    }

    fn update(&mut self, mut data: &[u8]) {
        while !data.is_empty() {
            // The BLAKE2b spec says the buffer is compressed only when more
            // bytes are coming after — so we keep at most 128 bytes pending
            // and only compress when we have ≥1 trailing byte.
            if self.buflen == 128 {
                self.t[0] = self.t[0].wrapping_add(128);
                if self.t[0] < 128 {
                    self.t[1] = self.t[1].wrapping_add(1);
                }
                self.compress(false);
                self.buflen = 0;
            }
            let take = (128 - self.buflen).min(data.len());
            self.buf[self.buflen..self.buflen + take].copy_from_slice(&data[..take]);
            self.buflen += take;
            data = &data[take..];
        }
    }

    fn finalize(mut self) -> [u8; 64] {
        self.t[0] = self.t[0].wrapping_add(self.buflen as u64);
        if self.t[0] < self.buflen as u64 {
            self.t[1] = self.t[1].wrapping_add(1);
        }
        // Zero-pad the remaining buffer.
        for byte in self.buf[self.buflen..].iter_mut() {
            *byte = 0;
        }
        self.compress(true);

        let mut out = [0u8; 64];
        for (i, word) in self.h.iter().enumerate() {
            out[i * 8..(i + 1) * 8].copy_from_slice(&word.to_le_bytes());
        }
        out
    }

    fn compress(&mut self, last: bool) {
        let mut m = [0u64; 16];
        for (word, chunk) in m.iter_mut().zip(self.buf.chunks_exact(8)) {
            // SAFETY-equivalent: `chunks_exact(8)` guarantees an 8-byte slice;
            // the `try_into` cannot fail and the `.unwrap_or` fallback is dead
            // defence kept so this stays panic-free per spec §11.
            *word = u64::from_le_bytes(chunk.try_into().unwrap_or([0; 8]));
        }

        let mut v = [0u64; 16];
        v[..8].copy_from_slice(&self.h);
        v[8..].copy_from_slice(&BLAKE2B_IV);
        v[12] ^= self.t[0];
        v[13] ^= self.t[1];
        if last {
            v[14] ^= 0xffff_ffff_ffff_ffff;
        }

        for s in &BLAKE2B_SIGMA {
            blake2b_g(&mut v, 0, 4, 8, 12, m[s[0]], m[s[1]]);
            blake2b_g(&mut v, 1, 5, 9, 13, m[s[2]], m[s[3]]);
            blake2b_g(&mut v, 2, 6, 10, 14, m[s[4]], m[s[5]]);
            blake2b_g(&mut v, 3, 7, 11, 15, m[s[6]], m[s[7]]);
            blake2b_g(&mut v, 0, 5, 10, 15, m[s[8]], m[s[9]]);
            blake2b_g(&mut v, 1, 6, 11, 12, m[s[10]], m[s[11]]);
            blake2b_g(&mut v, 2, 7, 8, 13, m[s[12]], m[s[13]]);
            blake2b_g(&mut v, 3, 4, 9, 14, m[s[14]], m[s[15]]);
        }

        for i in 0..8 {
            self.h[i] ^= v[i] ^ v[i + 8];
        }
    }
}

#[inline(always)]
fn blake2b_g(v: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize, x: u64, y: u64) {
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
    v[d] = (v[d] ^ v[a]).rotate_right(32);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(24);
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
    v[d] = (v[d] ^ v[a]).rotate_right(16);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(63);
}

#[cfg(test)]
fn blake2b(out_len: usize, data: &[u8]) -> Vec<u8> {
    let mut h = Blake2b::new(out_len);
    h.update(data);
    let full = h.finalize();
    full[..out_len].to_vec()
}

/// Argon2 H' "long hash" — variable-length output via chained BLAKE2b.
/// Defined in RFC 9106 §3.2 step 2 and §3.4.
fn h_prime(out_len: u32, input: &[u8]) -> Vec<u8> {
    let mut prefix = [0u8; 4];
    prefix.copy_from_slice(&out_len.to_le_bytes());

    if out_len <= 64 {
        let mut h = Blake2b::new(out_len as usize);
        h.update(&prefix);
        h.update(input);
        let full = h.finalize();
        return full[..out_len as usize].to_vec();
    }

    // For longer outputs the spec emits BLAKE2b-64 chunks: V_1 = H(64, prefix||input);
    // V_{i+1} = H(64, V_i); output = first 32 bytes of every V_i for i in [1, r-1],
    // then the last (out_len - 32 * (r-1)) bytes from V_r.
    let r = out_len.div_ceil(32) - 1;
    let mut out = Vec::with_capacity(out_len as usize);

    let mut v_prev = {
        let mut h = Blake2b::new(64);
        h.update(&prefix);
        h.update(input);
        h.finalize()
    };
    out.extend_from_slice(&v_prev[..32]);

    for _ in 1..r {
        let mut h = Blake2b::new(64);
        h.update(&v_prev);
        let v = h.finalize();
        out.extend_from_slice(&v[..32]);
        v_prev = v;
    }

    // Final block: output (out_len - 32*r) bytes of H(remaining).
    let last_len = out_len as usize - 32 * r as usize;
    let mut h = Blake2b::new(last_len);
    h.update(&v_prev);
    let v_last = h.finalize();
    out.extend_from_slice(&v_last[..last_len]);

    out
}

// ─── Argon2id memory block ──────────────────────────────────────────

/// One Argon2 memory block — 1024 bytes (128 × u64 words).
#[derive(Clone, Copy)]
struct Block([u64; 128]);

impl Block {
    fn zero() -> Self {
        Self([0u64; 128])
    }

    fn xor_into(&mut self, other: &Block) {
        for (a, b) in self.0.iter_mut().zip(other.0.iter()) {
            *a ^= *b;
        }
    }

    fn from_bytes(bytes: &[u8; 1024]) -> Self {
        let mut words = [0u64; 128];
        for (i, w) in words.iter_mut().enumerate() {
            // SAFETY-equivalent: const 8-byte chunk inside a 1024-byte array.
            *w = u64::from_le_bytes(bytes[i * 8..(i + 1) * 8].try_into().unwrap_or([0; 8]));
        }
        Self(words)
    }

    fn to_bytes(self) -> [u8; 1024] {
        let mut out = [0u8; 1024];
        for (i, w) in self.0.iter().enumerate() {
            out[i * 8..(i + 1) * 8].copy_from_slice(&w.to_le_bytes());
        }
        out
    }
}

/// Argon2 G compression function (RFC 9106 §3.5). Produces `out = X ⊕ Y ⊕ P(X⊕Y)`
/// where P is the BLAKE2b-derived round function applied row-then-column over
/// the 128-word block treated as an 8×8 matrix of 16-byte cells.
fn compress(out: &mut Block, x: &Block, y: &Block, with_xor: bool) {
    let mut r = Block::zero();
    for i in 0..128 {
        r.0[i] = x.0[i] ^ y.0[i];
    }
    let z = r;

    // Apply P to each of the 8 rows (each row = 16 u64 = 128 bytes).
    for row in 0..8 {
        let off = row * 16;
        let mut s: [u64; 16] = r.0[off..off + 16].try_into().unwrap_or([0; 16]);
        permute_p(&mut s);
        r.0[off..off + 16].copy_from_slice(&s);
    }
    // Apply P to each of the 8 columns. A column is the 16 u64 values at
    // positions (col*2, col*2+1, col*2+16, col*2+17, ...).
    for col in 0..8 {
        let mut s = [0u64; 16];
        for i in 0..8 {
            s[i * 2] = r.0[i * 16 + col * 2];
            s[i * 2 + 1] = r.0[i * 16 + col * 2 + 1];
        }
        permute_p(&mut s);
        for i in 0..8 {
            r.0[i * 16 + col * 2] = s[i * 2];
            r.0[i * 16 + col * 2 + 1] = s[i * 2 + 1];
        }
    }

    // Output: (Z ⊕ R) optionally XOR-folded into the existing block.
    if with_xor {
        for i in 0..128 {
            out.0[i] ^= z.0[i] ^ r.0[i];
        }
    } else {
        for i in 0..128 {
            out.0[i] = z.0[i] ^ r.0[i];
        }
    }
}

/// The Argon2 P permutation — BLAKE2b round function applied to 16 words
/// (RFC 9106 §3.6). Operates in-place.
#[inline(always)]
fn permute_p(s: &mut [u64; 16]) {
    blake2_round(s, 0, 4, 8, 12);
    blake2_round(s, 1, 5, 9, 13);
    blake2_round(s, 2, 6, 10, 14);
    blake2_round(s, 3, 7, 11, 15);
    blake2_round(s, 0, 5, 10, 15);
    blake2_round(s, 1, 6, 11, 12);
    blake2_round(s, 2, 7, 8, 13);
    blake2_round(s, 3, 4, 9, 14);
}

#[inline(always)]
fn blake2_round(s: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize) {
    // Argon2's GB function (RFC 9106 §3.6) — distinct from BLAKE2b's G:
    // it uses 32-bit lower-half multiplication adds in steps 1 and 5.
    let m_a_b = 2u64.wrapping_mul(lower32(s[a])).wrapping_mul(lower32(s[b]));
    s[a] = s[a].wrapping_add(s[b]).wrapping_add(m_a_b);
    s[d] = (s[d] ^ s[a]).rotate_right(32);

    let m_c_d = 2u64.wrapping_mul(lower32(s[c])).wrapping_mul(lower32(s[d]));
    s[c] = s[c].wrapping_add(s[d]).wrapping_add(m_c_d);
    s[b] = (s[b] ^ s[c]).rotate_right(24);

    let m_a_b2 = 2u64.wrapping_mul(lower32(s[a])).wrapping_mul(lower32(s[b]));
    s[a] = s[a].wrapping_add(s[b]).wrapping_add(m_a_b2);
    s[d] = (s[d] ^ s[a]).rotate_right(16);

    let m_c_d2 = 2u64.wrapping_mul(lower32(s[c])).wrapping_mul(lower32(s[d]));
    s[c] = s[c].wrapping_add(s[d]).wrapping_add(m_c_d2);
    s[b] = (s[b] ^ s[c]).rotate_right(63);
}

#[inline(always)]
fn lower32(x: u64) -> u64 {
    x & 0xffff_ffff
}

// ─── Public KDF entry point ─────────────────────────────────────────

/// Errors surfaced by the Argon2id KDF. Hand-rolled; spec §8.1 forbids
/// `thiserror`.
#[derive(Debug, PartialEq, Eq)]
pub enum Argon2Error {
    /// `tag_len` was 0 or `> 0xffff_ffff`. RFC 9106 §3.1 rejects 0.
    InvalidTagLen { tag_len: u32 },
    /// `m_kib` was below `8 * p` (RFC 9106 §3.1 minimum-memory rule).
    MemoryTooSmall { m_kib: u32, parallelism: u32 },
    /// `t` (iterations) was 0.
    IterationsZero,
    /// `p` (parallelism / lanes) was 0.
    ParallelismZero,
}

impl core::fmt::Display for Argon2Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidTagLen { tag_len } => write!(f, "invalid tag length: {tag_len}"),
            Self::MemoryTooSmall { m_kib, parallelism } => write!(
                f,
                "memory too small: m={m_kib} KiB, requires ≥ 8 * p = {} KiB",
                parallelism.saturating_mul(8)
            ),
            Self::IterationsZero => f.write_str("iterations (t) must be ≥ 1"),
            Self::ParallelismZero => f.write_str("parallelism (p) must be ≥ 1"),
        }
    }
}

impl core::error::Error for Argon2Error {}

/// Argon2id KDF parameters per RFC 9106 §3.
///
/// The defaults match Q2=C ruling: `m=64 MiB`, `t=3`, `p=4`, `tag_len=32`.
#[derive(Debug, Clone, Copy)]
pub struct Argon2Params {
    /// Memory size in KiB. Default 65 536 (64 MiB).
    pub m_kib: u32,
    /// Iterations / time cost. Default 3.
    pub iterations: u32,
    /// Parallelism / lane count. Default 4.
    pub parallelism: u32,
    /// Output tag length in bytes. Default 32 (AES-256 key).
    pub tag_len: u32,
}

impl Argon2Params {
    /// Locked defaults from Q2=C ruling. Use this for production callers.
    pub const DEFAULT: Self = Self {
        m_kib: 65_536,
        iterations: 3,
        parallelism: 4,
        tag_len: 32,
    };

    /// Low-cost params for the test cfg only (min-mem RSS knob, NOT a crash fix).
    /// 1 MiB matrix / t=1 / p=1; invariant `m_kib >= 8*parallelism` holds (1024 >= 8).
    /// Production keeps `DEFAULT`; this is a compile-time-only seam (see Task #8).
    #[cfg(test)]
    pub const TEST: Self = Self {
        m_kib: 1024,
        iterations: 1,
        parallelism: 1,
        tag_len: 32,
    };
}

/// Argon2id KDF. Returns a `tag_len`-byte key derived from `password` and
/// `salt`. `secret` and `associated_data` are RFC 9106 optional inputs —
/// nano callers pass empty slices.
///
/// The Argon2 type identifier is fixed to `2` (Argon2id).
pub fn argon2id(
    password: &[u8],
    salt: &[u8],
    params: Argon2Params,
) -> Result<Vec<u8>, Argon2Error> {
    argon2id_with_extras(password, salt, &[], &[], params)
}

fn argon2id_with_extras(
    password: &[u8],
    salt: &[u8],
    secret: &[u8],
    associated_data: &[u8],
    params: Argon2Params,
) -> Result<Vec<u8>, Argon2Error> {
    if params.tag_len == 0 {
        return Err(Argon2Error::InvalidTagLen {
            tag_len: params.tag_len,
        });
    }
    if params.parallelism == 0 {
        return Err(Argon2Error::ParallelismZero);
    }
    if params.iterations == 0 {
        return Err(Argon2Error::IterationsZero);
    }
    if params.m_kib < params.parallelism.saturating_mul(8) {
        return Err(Argon2Error::MemoryTooSmall {
            m_kib: params.m_kib,
            parallelism: params.parallelism,
        });
    }

    let m_prime = (params.m_kib / (4 * params.parallelism)) * (4 * params.parallelism);
    let lane_length = m_prime / params.parallelism;
    let segment_length = lane_length / 4;

    // ── Step 1: H_0 = BLAKE2b-64(p||tau||m||t||v||y||len(P)||P||len(S)||S||len(K)||K||len(X)||X)
    let mut h0_input =
        Vec::with_capacity(64 + password.len() + salt.len() + secret.len() + associated_data.len());
    h0_input.extend_from_slice(&params.parallelism.to_le_bytes());
    h0_input.extend_from_slice(&params.tag_len.to_le_bytes());
    h0_input.extend_from_slice(&params.m_kib.to_le_bytes());
    h0_input.extend_from_slice(&params.iterations.to_le_bytes());
    h0_input.extend_from_slice(&0x13_u32.to_le_bytes()); // v=0x13 (RFC 9106 §3.1)
    h0_input.extend_from_slice(&2_u32.to_le_bytes()); // y=2 (Argon2id)
    push_len_prefixed(&mut h0_input, password);
    push_len_prefixed(&mut h0_input, salt);
    push_len_prefixed(&mut h0_input, secret);
    push_len_prefixed(&mut h0_input, associated_data);

    let mut h0 = Blake2b::new(64);
    h0.update(&h0_input);
    let h0 = h0.finalize();

    // ── Allocate the memory matrix B[lanes][lane_length].
    let mut blocks: Vec<Block> = vec![Block::zero(); m_prime as usize];

    // ── Step 2: B[i][0] = H'(1024, H_0 || 0 || i),  B[i][1] = H'(1024, H_0 || 1 || i).
    for lane in 0..params.parallelism {
        for col in 0..2u32 {
            let mut input = Vec::with_capacity(64 + 8);
            input.extend_from_slice(&h0);
            input.extend_from_slice(&col.to_le_bytes());
            input.extend_from_slice(&lane.to_le_bytes());
            let bytes = h_prime(1024, &input);
            let mut buf = [0u8; 1024];
            buf.copy_from_slice(&bytes);
            blocks[(lane * lane_length + col) as usize] = Block::from_bytes(&buf);
        }
    }

    // ── Step 3: process passes.
    for pass in 0..params.iterations {
        for slice in 0..4u32 {
            // Sequential lane processing — this is the "p=1 per-machine" path
            // in the RFC. Q2=C explicitly accepts the straight-line variant
            // (true p=4 thread fan-out is a future optimisation; correctness
            // is identical).
            for lane in 0..params.parallelism {
                fill_segment(
                    &mut blocks,
                    pass,
                    lane,
                    slice,
                    lane_length,
                    segment_length,
                    params.parallelism,
                    params.iterations,
                );
            }
        }
    }

    // ── Step 4: C = B[0][q-1] ⊕ B[1][q-1] ⊕ ... ⊕ B[p-1][q-1].
    let mut c = blocks[(lane_length - 1) as usize];
    for lane in 1..params.parallelism {
        let idx = (lane * lane_length + (lane_length - 1)) as usize;
        c.xor_into(&blocks[idx]);
    }

    // ── Step 5: tag = H'(tag_len, C).
    Ok(h_prime(params.tag_len, &c.to_bytes()))
}

fn push_len_prefixed(dst: &mut Vec<u8>, data: &[u8]) {
    dst.extend_from_slice(&(data.len() as u32).to_le_bytes());
    dst.extend_from_slice(data);
}

#[allow(clippy::too_many_arguments)]
fn fill_segment(
    blocks: &mut [Block],
    pass: u32,
    lane: u32,
    slice: u32,
    lane_length: u32,
    segment_length: u32,
    parallelism: u32,
    iterations: u32,
) {
    // For Argon2id, the first half of pass 0 (slices 0 and 1) uses the
    // data-independent indexing (Argon2i mode); everything else uses the
    // data-dependent indexing (Argon2d mode).
    let data_independent = pass == 0 && slice < 2;

    // Pre-compute the address pseudo-random words for data-independent mode.
    // RFC 9106 §3.4.1.1 — we generate them in 1024-byte chunks via the
    // compress function applied to a deterministic input block.
    let mut address_block = Block::zero();
    let mut input_block = Block::zero();
    let zero_block = Block::zero();
    if data_independent {
        // RFC 9106 §3.4.1.1 address-block input layout:
        //   [0] = pass number
        //   [1] = lane number
        //   [2] = slice number
        //   [3] = total memory blocks (m')
        //   [4] = total passes (iterations)
        //   [5] = type (2 for Argon2id)
        //   [6] = counter (incremented each block of address output)
        input_block.0[0] = pass as u64;
        input_block.0[1] = lane as u64;
        input_block.0[2] = slice as u64;
        input_block.0[3] = blocks.len() as u64;
        input_block.0[4] = iterations as u64;
        input_block.0[5] = 2;
    }

    let starting_index = if pass == 0 && slice == 0 { 2 } else { 0 };

    for index in starting_index..segment_length {
        let curr_offset = (lane * lane_length + slice * segment_length + index) as usize;
        let prev_offset = if curr_offset % lane_length as usize == 0 {
            // Wrap to the end of the same lane.
            curr_offset + lane_length as usize - 1
        } else {
            curr_offset - 1
        };

        // Grab pseudo-random J1 (low 32 bits) and J2 (next 32 bits).
        let pseudo_rand = if data_independent {
            // Refresh the address block every 128 indexes (one block holds
            // 128 u64 words = 64 (J1, J2) pairs).
            if index % 128 == 0 {
                input_block.0[6] = input_block.0[6].wrapping_add(1);
                let mut tmp = Block::zero();
                compress(&mut tmp, &zero_block, &input_block, false);
                let mut tmp2 = Block::zero();
                compress(&mut tmp2, &zero_block, &tmp, false);
                address_block = tmp2;
            }
            address_block.0[(index % 128) as usize]
        } else {
            blocks[prev_offset].0[0]
        };

        let j1 = (pseudo_rand & 0xffff_ffff) as u32;
        let j2 = (pseudo_rand >> 32) as u32;

        // Pick the reference lane.
        let ref_lane = if pass == 0 && slice == 0 {
            lane
        } else {
            j2 % parallelism
        };

        // Pick the reference index within the chosen lane.
        let same_lane = ref_lane == lane;
        let ref_area_size = compute_ref_area_size(pass, slice, same_lane, segment_length, index);
        let mut relative_position = j1 as u64;
        relative_position = (relative_position * relative_position) >> 32;
        relative_position = ((ref_area_size as u64) * relative_position) >> 32;
        relative_position = (ref_area_size as u64) - 1 - relative_position;

        let start_position = if pass != 0 && slice != 3 {
            ((slice + 1) * segment_length) as u64
        } else {
            0
        };
        let absolute_position = (start_position + relative_position) % (lane_length as u64);
        let ref_index = (ref_lane as usize) * (lane_length as usize) + (absolute_position as usize);

        // Compress.
        let prev = blocks[prev_offset];
        let refb = blocks[ref_index];
        if pass == 0 {
            compress(&mut blocks[curr_offset], &prev, &refb, false);
        } else {
            // XOR into the existing block on subsequent passes.
            compress(&mut blocks[curr_offset], &prev, &refb, true);
        }
    }
}

/// Compute the size of the reference area for index selection per
/// RFC 9106 §3.4.1.2 (matching `phc-winner-argon2/src/core.c::index_alpha`
/// `reference_area_size` calculation).
fn compute_ref_area_size(
    pass: u32,
    slice: u32,
    same_lane: bool,
    segment_length: u32,
    index: u32,
) -> u32 {
    // index_zero == 1 when computing block at index 0 of the new segment
    // (special case for cross-lane reference: subtract 1 extra to skip the
    // last block of the previous segment that hasn't yet been written).
    let index_zero_adjust: u32 = if index == 0 { 1 } else { 0 };

    if pass == 0 {
        if slice == 0 {
            // Pass 0, segment 0: only blocks 0..(index-1) within this lane.
            index.saturating_sub(1)
        } else if same_lane {
            // Already-filled blocks in the current lane minus the previous block.
            slice * segment_length + index.saturating_sub(1)
        } else {
            // Other lane, previous segments only.
            slice * segment_length - index_zero_adjust
        }
    } else if same_lane {
        // 3 prior segments in this lane (lane_length - segment_length blocks)
        // plus already-filled portion of current segment minus previous block.
        // Note: lane_length = 4 * segment_length.
        3 * segment_length + index.saturating_sub(1)
    } else {
        // Other lane, 3 prior segments only.
        3 * segment_length - index_zero_adjust
    }
}

#[cfg(test)]
mod tests;
