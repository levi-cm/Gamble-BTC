use gbtc_core::job::{decode_hex, encode_hex, MiningJob};
use gbtc_core::work::{target_from_difficulty, work_hash};
use sha2::{Digest, Sha256};

fn hex_decode(s: &str) -> Vec<u8> {
    decode_hex(s).unwrap()
}

#[test]
fn decode_encode_hex_roundtrip() {
    let bytes = hex_decode("0102aabb");
    assert_eq!(bytes, vec![1, 2, 0xaa, 0xbb]);
    assert_eq!(encode_hex(&bytes), "0102aabb");
    assert!(decode_hex("abc").is_err());
    assert!(decode_hex("gg").is_err());
}

#[test]
fn header_coinbase_merkle_bytes() {
    let job = MiningJob {
        connection_generation: 1,
        job_generation: 2,
        target_generation: 3,
        job_id: "job-1".to_string(),
        prevhash: [0u8; 32],
        coinb1: hex_decode("0102"),
        coinb2: hex_decode("0304"),
        merkle: vec![],
        version: 0x20000000,
        nbits: 0x1d00ffff,
        ntime: 0x495fab29,
        target: target_from_difficulty(1.0).unwrap(),
        difficulty: 1.0,
        clean: false,
    };
    let en1 = hex_decode("a1b2c3d4");
    let en2 = vec![0u8, 0u8, 0u8, 0u8];
    let hdr = job.header(&en1, &en2).unwrap();
    assert_eq!(hdr.len(), 80);
    // version LE
    assert_eq!(&hdr[0..4], &[0x00, 0x00, 0x00, 0x20]);
    // ntime LE
    assert_eq!(&hdr[68..72], &[0x29, 0xab, 0x5f, 0x49]);
    // nbits LE
    assert_eq!(&hdr[72..76], &[0xff, 0xff, 0x00, 0x1d]);
    // nonce placeholder
    assert_eq!(&hdr[76..80], &[0, 0, 0, 0]);
}

#[test]
fn header_merkle_fold() {
    let branch = [0x55u8; 32];
    let job = MiningJob {
        connection_generation: 1,
        job_generation: 1,
        target_generation: 1,
        job_id: "j".to_string(),
        prevhash: [0u8; 32],
        coinb1: hex_decode("01"),
        coinb2: hex_decode("02"),
        merkle: vec![branch],
        version: 1,
        nbits: 0x1d00ffff,
        ntime: 1,
        target: target_from_difficulty(1.0).unwrap(),
        difficulty: 1.0,
        clean: false,
    };
    let hdr = job.header(&[], &[]).unwrap();
    // independent: coinbase hash then fold branch
    let mut cb = vec![1u8];
    // extranonces empty
    cb.push(2u8);
    // wait coinb1 then en1 empty en2 empty coinb2
    let mut coinbase = vec![1u8];
    coinbase.extend_from_slice(&[] as &[u8]);
    coinbase.extend_from_slice(&[] as &[u8]);
    coinbase.push(2u8);
    let mut mr: [u8; 32] = Sha256::digest(Sha256::digest(&coinbase)).into();
    let mut cat = [0u8; 64];
    cat[..32].copy_from_slice(&mr);
    cat[32..].copy_from_slice(&branch);
    mr = Sha256::digest(Sha256::digest(cat)).into();
    assert_eq!(&hdr[36..68], &mr);
}

#[test]
fn work_builds_batch() {
    let job = MiningJob {
        connection_generation: 1,
        job_generation: 1,
        target_generation: 1,
        job_id: "j".to_string(),
        prevhash: [0u8; 32],
        coinb1: hex_decode("01"),
        coinb2: hex_decode("02"),
        merkle: vec![],
        version: 1,
        nbits: 0x1d00ffff,
        ntime: 1,
        target: target_from_difficulty(1.0).unwrap(),
        difficulty: 1.0,
        clean: false,
    };
    let work = job.work(&[], &[], 0, 256).unwrap();
    assert_eq!(work.nonce_base, 0);
    assert_eq!(work.nonce_count, 256);
    assert_eq!(work.target, job.target);
    // independent hash check
    let nonce = 42u32;
    let h = work_hash(&work, nonce);
    let mut header = job.header(&[], &[]).unwrap();
    header[76..80].copy_from_slice(&nonce.to_be_bytes());
    let expected: [u8; 32] = Sha256::digest(Sha256::digest(header)).into();
    assert_eq!(h, expected);
}

#[test]
fn nonce_submit_swap_convention() {
    let raw = 0x01020304u32;
    assert_eq!(format!("{:08x}", raw.swap_bytes()), "04030201");
}

#[test]
fn invalid_difficulty_rejected() {
    let mut job = MiningJob {
        connection_generation: 1,
        job_generation: 1,
        target_generation: 1,
        job_id: "j".to_string(),
        prevhash: [0u8; 32],
        coinb1: hex_decode("01"),
        coinb2: hex_decode("02"),
        merkle: vec![],
        version: 1,
        nbits: 0x1d00ffff,
        ntime: 1,
        target: target_from_difficulty(1.0).unwrap(),
        difficulty: f64::NAN,
        clean: false,
    };
    assert!(job.work(&[], &[], 0, 1).is_err());
    job.difficulty = 0.0;
    assert!(job.work(&[], &[], 0, 1).is_err());
}

#[test]
fn bounds_validation() {
    let job = MiningJob {
        connection_generation: 1,
        job_generation: 1,
        target_generation: 1,
        job_id: "j".to_string(),
        prevhash: [0u8; 32],
        coinb1: vec![0u8; 512],
        coinb2: hex_decode("02"),
        merkle: vec![],
        version: 1,
        nbits: 0x1d00ffff,
        ntime: 1,
        target: target_from_difficulty(1.0).unwrap(),
        difficulty: 1.0,
        clean: false,
    };
    assert!(job.header(&[], &[]).is_err());
    // extranonce2 too long
    let job2 = MiningJob {
        coinb1: hex_decode("01"),
        ..job.clone()
    };
    // fix job2 coinb1 already? just test extranonce len
    let long_en2 = vec![0u8; 9];
    assert!(job2.header(&[], &long_en2).is_err());
    // merkle >32
    let mut job3 = job2.clone();
    job3.merkle = vec![[0u8; 32]; 33];
    assert!(job3.header(&[], &[]).is_err());
    // nonce range
    let job4 = job2.clone();
    assert!(job4.work(&[], &[], 0, 0).is_err());
    assert!(job4.work(&[], &[], u32::MAX, 2).is_err());
}
