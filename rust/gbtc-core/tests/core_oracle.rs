use gbtc_core::backend::WorkBatch;
use gbtc_core::sha256::{sha256, sha256d};
use gbtc_core::work::{
    build_header_midstate, raw_hash_meets_target, reference_final_word7, target_from_difficulty,
    target_to_raw_hash, work_hash,
};
use sha2::{Digest, Sha256};

fn hex_encode(b: &[u8]) -> String {
    const L: &[u8] = b"0123456789abcdef";
    let mut s = String::with_capacity(b.len() * 2);
    for &x in b {
        s.push(L[(x >> 4) as usize] as char);
        s.push(L[(x & 0x0f) as usize] as char);
    }
    s
}

#[test]
fn sha256_oracle_random() {
    for seed in 0u64..256 {
        let mut data = vec![0u8; (seed as usize % 130) + 1];
        for (i, byte) in data.iter_mut().enumerate() {
            *byte = ((seed as usize * 37 + i * 13 + 5) & 0xff) as u8;
        }
        let got = sha256(&data);
        let want = Sha256::digest(&data);
        assert_eq!(got.as_slice(), want.as_slice(), "seed {}", seed);
    }
}

#[test]
fn sha256d_oracle_random() {
    for seed in 0u64..128 {
        let mut data = vec![0u8; seed as usize % 200];
        for (i, byte) in data.iter_mut().enumerate() {
            *byte = ((seed as usize * 53 + i * 7) & 0xff) as u8;
        }
        let got = sha256d(&data);
        let mid = Sha256::digest(&data);
        let want = Sha256::digest(mid);
        assert_eq!(got.as_slice(), want.as_slice(), "seed {}", seed);
    }
}

#[test]
fn empty_and_abc() {
    assert_eq!(
        hex_encode(&sha256(b"")),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        hex_encode(&sha256(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn full_header_equivalence_many() {
    for seed in 0u32..128 {
        let mut header = [0u8; 80];
        for (i, byte) in header.iter_mut().enumerate() {
            *byte = ((seed.wrapping_mul(37).wrapping_add(i as u32 * 13)) & 0xff) as u8;
        }
        let nonce = seed.wrapping_mul(97).wrapping_add(1);
        // place nonce big-endian in last 4 bytes
        header[76] = (nonce >> 24) as u8;
        header[77] = (nonce >> 16) as u8;
        header[78] = (nonce >> 8) as u8;
        header[79] = nonce as u8;
        // build midstate from first 64 bytes (nonce not in first block)
        let mut h0 = [0u8; 80];
        h0.copy_from_slice(&header);
        h0[76] = 0;
        h0[77] = 0;
        h0[78] = 0;
        h0[79] = 0;
        let (ms, t3) = build_header_midstate(&h0).unwrap();
        let work = WorkBatch {
            midstate: ms,
            tail3: t3,
            target: [0; 8],
            nonce_base: 0,
            nonce_count: 1,
        };
        // independent full hash of header with nonce
        let full = sha256d(&header);
        let wh = work_hash(&work, nonce);
        assert_eq!(full, wh, "seed {}", seed);
        // final word7
        let w7 = reference_final_word7(&work, nonce);
        let fh_w7 = ((full[28] as u32) << 24)
            | ((full[29] as u32) << 16)
            | ((full[30] as u32) << 8)
            | (full[31] as u32);
        assert_eq!(w7, fh_w7);
    }
}

#[test]
fn target_boundary() {
    let diff1 = target_from_difficulty(1.0).unwrap();
    let raw = target_to_raw_hash(&diff1);
    assert!(raw_hash_meets_target(&raw, &diff1));
    // neighbor lower accepted
    let mut lower = diff1;
    lower[1] -= 1;
    let rawl = target_to_raw_hash(&lower);
    assert!(raw_hash_meets_target(&rawl, &diff1));
    // neighbor higher rejected
    let mut higher = diff1;
    higher[1] += 1;
    let rawh = target_to_raw_hash(&higher);
    assert!(!raw_hash_meets_target(&rawh, &diff1));
}

#[test]
fn range_validation() {
    use gbtc_core::backend::nonce_range_valid;
    assert!(!nonce_range_valid(0, 0));
    assert!(nonce_range_valid(0, u32::MAX));
    assert!(!nonce_range_valid(2, u32::MAX));
    assert!(nonce_range_valid(1, u32::MAX));
}

#[test]
fn scan_complete_set() {
    // synthetic work
    let mut header = [0u8; 80];
    for (i, byte) in header.iter_mut().enumerate() {
        *byte = ((i * 37 + 11) & 0xff) as u8;
    }
    header[76] = 0;
    header[77] = 0;
    header[78] = 0;
    header[79] = 0;
    let (ms, t3) = build_header_midstate(&header).unwrap();
    let mut work = WorkBatch {
        midstate: ms,
        tail3: t3,
        target: [0; 8],
        nonce_base: 0,
        nonce_count: 256,
    };
    work.target[0] = 0;
    work.target[1] = 0xffff0000;
    let even_nonce = 42u32;
    let even_word = reference_final_word7(&work, even_nonce);
    // scan all
    let found = gbtc_core::work::reference_scan_word7(&work, even_word);
    assert!(found.contains(&even_nonce));
    // unique_hashes count
    let res = gbtc_core::work::reference_scan(&work, even_word);
    assert_eq!(res.unique_hashes, 256);
}
