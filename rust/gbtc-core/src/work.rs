use crate::backend::{BatchResult, WorkBatch};
use crate::sha256::{compress, init_state, sha256};

fn put_be32(dst: &mut [u8], value: u32) {
    dst[0] = (value >> 24) as u8;
    dst[1] = (value >> 16) as u8;
    dst[2] = (value >> 8) as u8;
    dst[3] = value as u8;
}

fn read_be32(src: &[u8]) -> u32 {
    ((src[0] as u32) << 24) | ((src[1] as u32) << 16) | ((src[2] as u32) << 8) | (src[3] as u32)
}

pub fn build_header_midstate_into(
    midstate: &mut [u32; 8],
    tail3: &mut [u32; 3],
    header: &[u8],
) -> Result<(), String> {
    if header.len() != 80 {
        return Err("header must be exactly 80 bytes".to_string());
    }
    init_state(midstate);
    let mut block = [0u8; 64];
    block.copy_from_slice(&header[..64]);
    compress(midstate, &block);
    for i in 0..3 {
        tail3[i] = read_be32(&header[64 + 4 * i..]);
    }
    Ok(())
}

pub fn build_header_midstate(header: &[u8]) -> Result<([u32; 8], [u32; 3]), String> {
    let mut midstate = [0u32; 8];
    let mut tail3 = [0u32; 3];
    build_header_midstate_into(&mut midstate, &mut tail3, header)?;
    Ok((midstate, tail3))
}

pub fn work_hash(work: &WorkBatch, nonce: u32) -> [u8; 32] {
    let mut out = [0u8; 32];
    work_hash_into(&mut out, work, nonce);
    out
}

pub fn work_hash_into(out: &mut [u8; 32], work: &WorkBatch, nonce: u32) {
    let mut block = [0u8; 64];
    put_be32(&mut block[0..4], work.tail3[0]);
    put_be32(&mut block[4..8], work.tail3[1]);
    put_be32(&mut block[8..12], work.tail3[2]);
    put_be32(&mut block[12..16], nonce);
    block[16] = 0x80;
    // bytes 17..60 zero
    put_be32(&mut block[60..64], 640u32);

    let mut state = work.midstate;
    compress(&mut state, &block);

    let mut first = [0u8; 32];
    for i in 0..8 {
        put_be32(&mut first[4 * i..4 * i + 4], state[i]);
    }

    let h = sha256(&first);
    out.copy_from_slice(&h);
}

// Aliases
pub fn gbtc_work_hash(out: &mut [u8; 32], work: &WorkBatch, nonce: u32) {
    work_hash_into(out, work, nonce);
}

pub fn reference_final_word7(work: &WorkBatch, nonce: u32) -> u32 {
    let h = work_hash(work, nonce);
    ((h[28] as u32) << 24) | ((h[29] as u32) << 16) | ((h[30] as u32) << 8) | (h[31] as u32)
}

pub fn gbtc_reference_final_word7(work: &WorkBatch, nonce: u32) -> u32 {
    reference_final_word7(work, nonce)
}

pub fn reference_scan_word7(work: &WorkBatch, target_word7: u32) -> Vec<u32> {
    let mut out = Vec::new();
    for i in 0..work.nonce_count {
        let nonce = work.nonce_base.wrapping_add(i);
        if reference_final_word7(work, nonce) == target_word7 {
            out.push(nonce);
        }
    }
    out
}

pub fn reference_scan_word7_count(work: &WorkBatch, target_word7: u32) -> u32 {
    reference_scan_word7(work, target_word7).len() as u32
}

pub fn reference_scan_word7_into(work: &WorkBatch, target_word7: u32, nonces: &mut [u32]) -> u32 {
    let all = reference_scan_word7(work, target_word7);
    let n = all.len().min(nonces.len());
    nonces[..n].copy_from_slice(&all[..n]);
    all.len() as u32
}

pub fn reference_scan(work: &WorkBatch, target_word7: u32) -> BatchResult {
    let candidates = reference_scan_word7(work, target_word7);
    BatchResult {
        candidates,
        unique_hashes: work.nonce_count as u64,
        overflow_rescans: 0,
    }
}

pub fn gbtc_reference_scan_word7(
    work: &WorkBatch,
    target_word7: u32,
    nonces: &mut [u32],
    max_nonces: usize,
) -> u32 {
    // C-style: fill up to max_nonces, return total count
    let _ = max_nonces;
    let mut buf = vec![0u32; nonces.len().max(max_nonces)];
    let total = reference_scan_word7_into(work, target_word7, &mut buf);
    let n = (total as usize).min(nonces.len());
    if n > 0 && !buf.is_empty() {
        nonces[..n].copy_from_slice(&buf[..n]);
    }
    total
}

// Target conversion port.
pub fn target_from_difficulty(difficulty: f64) -> Result<[u32; 8], String> {
    if !difficulty.is_finite() || difficulty < 1.0 || difficulty > 1e30 {
        return Err("difficulty must be finite and from 1 to 1e30".to_string());
    }

    const BIG_LIMBS: usize = 12;
    type Limbs = [u32; BIG_LIMBS];

    fn shift_left(limbs: &mut Limbs, bits: u32) {
        let words = (bits / 32) as usize;
        let rem = bits % 32;
        if words > 0 {
            for i in (0..BIG_LIMBS).rev() {
                limbs[i] = if i >= words { limbs[i - words] } else { 0 };
            }
        }
        if rem > 0 {
            let mut carry = 0u32;
            for limb in limbs.iter_mut() {
                let next = *limb >> (32 - rem);
                *limb = (*limb << rem) | carry;
                carry = next;
            }
        }
    }

    fn shift_right(limbs: &mut Limbs, bits: u32) {
        let words = (bits / 32) as usize;
        let rem = bits % 32;
        if words > 0 {
            for i in 0..BIG_LIMBS {
                limbs[i] = if i + words < BIG_LIMBS {
                    limbs[i + words]
                } else {
                    0
                };
            }
        }
        if rem > 0 {
            let mut carry = 0u32;
            for i in (0..BIG_LIMBS).rev() {
                let next = limbs[i] << (32 - rem);
                limbs[i] = (limbs[i] >> rem) | carry;
                carry = next;
            }
        }
    }

    fn divide_u64(limbs: &mut Limbs, divisor: u64) {
        let mut remainder: u64 = 0;
        for limb in limbs.iter_mut().rev() {
            let current: u128 = ((remainder as u128) << 32) | (*limb as u128);
            *limb = (current / divisor as u128) as u32;
            remainder = (current % divisor as u128) as u64;
        }
    }

    let encoded = difficulty.to_bits();
    let exponent = ((encoded >> 52) & 0x7ff) as u32;
    let mantissa = (1u64 << 52) | (encoded & 0x000fffffffffffff);
    let shift = 1075i32 - exponent as i32;

    let mut value = [0u32; BIG_LIMBS];
    value[6] = 0xffff0000u32;
    if shift >= 0 {
        shift_left(&mut value, shift as u32);
    }
    divide_u64(&mut value, mantissa);
    if shift < 0 {
        shift_right(&mut value, (-shift) as u32);
    }

    if value[8..].iter().any(|&v| v != 0) {
        return Err("difficulty produced an out-of-range target".to_string());
    }
    let mut target = [0u32; 8];
    let mut nonzero = false;
    for (i, &v) in value[..8].iter().enumerate() {
        target[7 - i] = v;
        nonzero = nonzero || v != 0;
    }
    if !nonzero {
        return Err("difficulty produced a zero target".to_string());
    }
    Ok(target)
}

pub fn target_from_difficulty_into(difficulty: f64, target: &mut [u32; 8]) -> Result<(), String> {
    match target_from_difficulty(difficulty) {
        Ok(t) => {
            target.copy_from_slice(&t);
            Ok(())
        }
        Err(e) => Err(e),
    }
}

pub fn target_to_raw_hash(target: &[u32; 8]) -> [u8; 32] {
    let mut raw = [0u8; 32];
    target_to_raw_hash_into(target, &mut raw);
    raw
}

pub fn target_to_raw_hash_into(target: &[u32; 8], raw_hash: &mut [u8; 32]) {
    let mut display = [0u8; 32];
    for i in 0..8 {
        display[4 * i] = (target[i] >> 24) as u8;
        display[4 * i + 1] = (target[i] >> 16) as u8;
        display[4 * i + 2] = (target[i] >> 8) as u8;
        display[4 * i + 3] = target[i] as u8;
    }
    for i in 0..32 {
        raw_hash[i] = display[31 - i];
    }
}

pub fn raw_hash_meets_target(raw_hash: &[u8; 32], target: &[u32; 8]) -> bool {
    for (i, &t) in target.iter().enumerate() {
        let p = 31 - 4 * i;
        let word = ((raw_hash[p] as u32) << 24)
            | ((raw_hash[p - 1] as u32) << 16)
            | ((raw_hash[p - 2] as u32) << 8)
            | (raw_hash[p - 3] as u32);
        if word < t {
            return true;
        }
        if word > t {
            return false;
        }
    }
    true
}

// C-style aliases returning int
pub fn gbtc_target_from_difficulty(difficulty: f64, target: &mut [u32; 8]) -> i32 {
    match target_from_difficulty_into(difficulty, target) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

pub fn gbtc_target_to_raw_hash(target: &[u32; 8], raw_hash: &mut [u8; 32]) {
    target_to_raw_hash_into(target, raw_hash);
}

pub fn gbtc_raw_hash_meets_target(raw_hash: &[u8; 32], target: &[u32; 8]) -> bool {
    raw_hash_meets_target(raw_hash, target)
}

// JobSnapshot
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobSnapshot {
    pub connection_generation: u64,
    pub job_generation: u64,
    pub target_generation: u64,
    pub job_id: String,
    pub extranonce2: Vec<u8>,
    pub ntime: u32,
    pub target: [u32; 8],
    pub header: [u8; 80],
    pub valid: bool,
}

// Range/cursor types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NonceCursor {
    pub value: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NonceRange {
    pub base: u32,
    pub count: u32,
}

impl NonceRange {
    pub fn validate(&self) -> bool {
        crate::backend::nonce_range_valid(self.base, self.count)
    }
    pub fn end(&self) -> Option<u64> {
        let e = self.base as u64 + self.count as u64;
        if e <= (1u64 << 32) {
            Some(e)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkRange {
    pub cursor: u64,
    pub batch_count: u32,
}

impl WorkRange {
    pub fn validate(&self) -> bool {
        self.cursor + self.batch_count as u64 <= (1u64 << 32)
    }
}
pub fn make_synthetic_work(work: &mut WorkBatch, nonce_count: u32) {
    let mut header = [0u8; 80];
    for (i, byte) in header.iter_mut().enumerate() {
        *byte = ((i * 37 + 11) & 0xff) as u8;
    }
    header[76] = 0;
    header[77] = 0;
    header[78] = 0;
    header[79] = 0;

    let (ms, t3) = build_header_midstate(&header).unwrap();
    work.midstate = ms;
    work.tail3 = t3;
    work.nonce_base = 0;
    work.nonce_count = nonce_count;
    work.target = [0u32; 8];
    work.target[0] = 0x00000000;
    work.target[1] = 0xffff0000;
}

pub fn gbtc_make_synthetic_work(work: &mut WorkBatch, nonce_count: u32) {
    make_synthetic_work(work, nonce_count);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::WorkBatch;

    fn hex_decode(s: &str) -> Vec<u8> {
        let mut out = Vec::with_capacity(s.len() / 2);
        let bytes = s.as_bytes();
        for i in (0..bytes.len()).step_by(2) {
            out.push(
                u8::from_str_radix(std::str::from_utf8(&bytes[i..i + 2]).unwrap(), 16).unwrap(),
            );
        }
        out
    }

    #[test]
    fn target_difficulty_vectors() {
        let d1 = target_from_difficulty(1.0).unwrap();
        assert_eq!(d1, [0x00000000, 0xffff0000, 0, 0, 0, 0, 0, 0]);
        let d2 = target_from_difficulty(2.0).unwrap();
        assert_eq!(d2, [0x00000000, 0x7fff8000, 0, 0, 0, 0, 0, 0]);
        let d3 = target_from_difficulty(3.0).unwrap();
        assert_eq!(d3, [0x00000000, 0x55550000, 0, 0, 0, 0, 0, 0]);
        assert!(target_from_difficulty(0.0).is_err());
        assert!(target_from_difficulty(f64::NAN).is_err());
        assert!(target_from_difficulty(1e100).is_err());
    }

    #[test]
    fn target_roundtrip_compare() {
        let diff1 = [0x00000000u32, 0xffff0000, 0, 0, 0, 0, 0, 0];
        let mut raw = [0u8; 32];
        target_to_raw_hash_into(&diff1, &mut raw);
        assert!(raw_hash_meets_target(&raw, &diff1));
        // lower
        let mut lower = diff1;
        lower[1] -= 1;
        let mut rawl = [0u8; 32];
        target_to_raw_hash_into(&lower, &mut rawl);
        assert!(raw_hash_meets_target(&rawl, &diff1));
        // higher
        let mut higher = diff1;
        higher[1] += 1;
        let mut rawh = [0u8; 32];
        target_to_raw_hash_into(&higher, &mut rawh);
        assert!(!raw_hash_meets_target(&rawh, &diff1));
    }

    #[test]
    fn genesis_hash_meets() {
        let diff1 = [0x00000000u32, 0xffff0000, 0, 0, 0, 0, 0, 0];
        let mut gh = [0u8; 32];
        let s = "6fe28c0ab6f1b372c1a6a246ae63f74f931e8365e15a089c68d6190000000000";
        let b = hex_decode(s);
        gh.copy_from_slice(&b);
        assert!(raw_hash_meets_target(&gh, &diff1));
    }

    #[test]
    fn work_hash_word7() {
        let mut work = WorkBatch {
            midstate: [0; 8],
            tail3: [0; 3],
            target: [0; 8],
            nonce_base: 0,
            nonce_count: 256,
        };
        // use synthetic via make? just basic
        // build from header pattern
        let mut header = [0u8; 80];
        for (i, byte) in header.iter_mut().enumerate() {
            *byte = ((i * 37 + 11) & 0xff) as u8;
        }
        header[76] = 0;
        header[77] = 0;
        header[78] = 0;
        header[79] = 0;
        let (ms, t3) = build_header_midstate(&header).unwrap();
        work.midstate = ms;
        work.tail3 = t3;
        work.target[0] = 0;
        work.target[1] = 0xffff0000;
        let nonce = 42u32;
        let h = work_hash(&work, nonce);
        let w7 = ((h[28] as u32) << 24)
            | ((h[29] as u32) << 16)
            | ((h[30] as u32) << 8)
            | (h[31] as u32);
        assert_eq!(w7, reference_final_word7(&work, nonce));
    }

    #[test]
    fn scan_complete() {
        // synthetic work like C make_synthetic_work
        let mut work = WorkBatch {
            midstate: [0; 8],
            tail3: [0; 3],
            target: [0; 8],
            nonce_base: 0,
            nonce_count: 256,
        };
        let mut header = [0u8; 80];
        for (i, byte) in header.iter_mut().enumerate() {
            *byte = ((i * 37 + 11) & 0xff) as u8;
        }
        header[76] = 0;
        header[77] = 0;
        header[78] = 0;
        header[79] = 0;
        let (ms, t3) = build_header_midstate(&header).unwrap();
        work.midstate = ms;
        work.tail3 = t3;
        work.target[0] = 0;
        work.target[1] = 0xffff0000;
        let even_nonce = 42u32;
        let even_word = reference_final_word7(&work, even_nonce);
        let found = reference_scan_word7(&work, even_word);
        assert!(found.contains(&even_nonce));
    }

    #[test]
    fn full_header_equivalence_random() {
        // independent sha256d vs work_hash for random headers
        // rand may not be available; use simple deterministic
        for seed in 0u32..64 {
            let mut header = [0u8; 80];
            for (i, byte) in header.iter_mut().enumerate() {
                *byte = ((seed.wrapping_mul(37).wrapping_add(i as u32 * 13)) & 0xff) as u8;
            }
            header[76] = 0;
            header[77] = 0;
            header[78] = 0;
            header[79] = 0;
            let nonce = seed.wrapping_mul(97);
            // set nonce bytes? work_hash uses nonce param, header placeholder zero; independent full header must include nonce
            let (ms, t3) = build_header_midstate(&header).unwrap();
            let work = WorkBatch {
                midstate: ms,
                tail3: t3,
                target: [0; 8],
                nonce_base: 0,
                nonce_count: 1,
            };
            // independent: build full 80-byte header with nonce in last 4 bytes big-endian
            let mut full = header;
            full[76] = (nonce >> 24) as u8;
            full[77] = (nonce >> 16) as u8;
            full[78] = (nonce >> 8) as u8;
            full[79] = nonce as u8;
            // compute midstate from first 64 (without nonce) matches header first 64 (nonce only last block)
            let full_hash = crate::sha256::sha256d(&full);
            let wh = work_hash(&work, nonce);
            assert_eq!(full_hash, wh, "mismatch seed {}", seed);
            let _ = (work, full);
        }
    }
}
