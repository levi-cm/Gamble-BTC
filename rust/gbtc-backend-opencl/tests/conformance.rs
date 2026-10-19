use gbtc_backend_opencl::OpenClBackend;
use gbtc_core::backend::ComputeBackend;
use gbtc_core::work::build_header_midstate;
use gbtc_core::WorkBatch;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn require_gpu() -> bool {
    std::env::var("GBTC_REQUIRE_GPU_TESTS").ok().as_deref() == Some("1")
}

fn gpu_available() -> bool {
    OpenClBackend::new(None).is_ok()
}

fn sha256d(data: &[u8]) -> [u8; 32] {
    let first: [u8; 32] = Sha256::digest(data).into();
    Sha256::digest(first).into()
}

fn independent_candidates(prefix: &[u8; 76], base: u32, count: u32, target_word0: u32) -> Vec<u32> {
    let mut out = Vec::new();
    for i in 0..count {
        let nonce = base.wrapping_add(i);
        let mut full = [0u8; 80];
        full[..76].copy_from_slice(prefix);
        full[76] = (nonce >> 24) as u8;
        full[77] = (nonce >> 16) as u8;
        full[78] = (nonce >> 8) as u8;
        full[79] = nonce as u8;
        let h = sha256d(&full);
        let word = u32::from_be_bytes([h[28], h[29], h[30], h[31]]);
        let bswapped = word.swap_bytes();
        if bswapped <= target_word0 {
            out.push(nonce);
        }
    }
    out
}

fn make_work(prefix: &[u8; 76], base: u32, count: u32, target_word0: u32) -> WorkBatch {
    let mut header = [0u8; 80];
    header[..76].copy_from_slice(prefix);
    let (ms, t3) = build_header_midstate(&header).unwrap();
    let mut work = WorkBatch {
        midstate: ms,
        tail3: t3,
        target: [0; 8],
        nonce_base: base,
        nonce_count: count,
    };
    work.target[0] = target_word0;
    work
}

#[test]
fn wrong_device_fails() {
    let res = OpenClBackend::new(Some("definitely-not-a-real-device-xyz"));
    assert!(res.is_err(), "expected failure for wrong device");
}

#[test]
fn invalid_settings_fail() {
    std::env::set_var("GBTC_OPENCL_LOCAL_SIZE", "999");
    assert!(OpenClBackend::new(None).is_err());
    std::env::remove_var("GBTC_OPENCL_LOCAL_SIZE");

    std::env::set_var("GBTC_OPENCL_POLL_US", "50");
    assert!(OpenClBackend::new(None).is_err());
    std::env::remove_var("GBTC_OPENCL_POLL_US");

    std::env::set_var("GBTC_OPENCL_KERNEL", "nope");
    assert!(OpenClBackend::new(None).is_err());
    std::env::remove_var("GBTC_OPENCL_KERNEL");

    std::env::set_var("GBTC_OPENCL_SIMD", "7");
    assert!(OpenClBackend::new(None).is_err());
    std::env::remove_var("GBTC_OPENCL_SIMD");
}

#[test]
fn probe_or_dispatch() {
    if !gpu_available() {
        if require_gpu() {
            panic!("GPU required but unavailable");
        }
        eprintln!("SKIP: no OpenCL GPU available");
        return;
    }
    let info = OpenClBackend::probe(None).expect("probe");
    assert_eq!(info.backend, "opencl");
    assert!(!info.device.is_empty());
    assert!(!info.driver.is_empty());
}

fn run_conformance(kernel: &str, simd: Option<&str>) {
    std::env::set_var("GBTC_OPENCL_KERNEL", kernel);
    if let Some(s) = simd {
        std::env::set_var("GBTC_OPENCL_SIMD", s);
    } else {
        std::env::remove_var("GBTC_OPENCL_SIMD");
    }

    if !gpu_available() {
        if require_gpu() {
            panic!("GPU required but unavailable");
        }
        eprintln!("SKIP: no OpenCL GPU available");
        std::env::remove_var("GBTC_OPENCL_KERNEL");
        return;
    }

    // small known header
    {
        let mut prefix = [0u8; 76];
        for (i, b) in prefix.iter_mut().enumerate() {
            *b = ((i * 37 + 11) & 0xff) as u8;
        }
        let mut chosen_target = 0u32;
        let mut expected = Vec::new();
        for &t in &[
            0x00010000u32,
            0x00020000,
            0x00040000,
            0x00100000,
            0x00200000,
            0x00400000,
            0x00ffffff,
            0xffffffff,
        ] {
            let exp = independent_candidates(&prefix, 0, 256, t);
            if !exp.is_empty() && exp.len() <= 64 {
                chosen_target = t;
                expected = exp;
                break;
            }
        }
        assert!(!expected.is_empty(), "no suitable threshold found");
        let work = make_work(&prefix, 0, 256, chosen_target);
        let mut ctx = OpenClBackend::new(None).expect("context");
        let res = ctx.run_batch(&work).expect("run_batch");
        assert_eq!(res.unique_hashes, work.nonce_count as u64);
        let got: BTreeSet<u32> = res.candidates.iter().copied().collect();
        let exp: BTreeSet<u32> = expected.iter().copied().collect();
        assert_eq!(got, exp, "candidate set mismatch");
    }

    // complete set easy target
    {
        let prefix = [0u8; 76];
        let count = 512u32;
        let work = make_work(&prefix, 0, count, u32::MAX);
        let expected = independent_candidates(&prefix, 0, count, u32::MAX);
        let mut ctx = OpenClBackend::new(None).expect("context");
        let res = ctx.run_batch(&work).expect("run_batch");
        assert_eq!(res.candidates.len(), count as usize);
        assert_eq!(res.unique_hashes, count as u64);
        let got: BTreeSet<u32> = res.candidates.iter().copied().collect();
        let exp: BTreeSet<u32> = expected.iter().copied().collect();
        assert_eq!(got, exp);
    }

    // overflow dedup
    {
        let prefix = [7u8; 76];
        let base = 1000u32;
        let count = 2000u32;
        let work = make_work(&prefix, base, count, u32::MAX);
        let expected = independent_candidates(&prefix, base, count, u32::MAX);
        let mut ctx = OpenClBackend::new(None).expect("context");
        let res = ctx.run_batch(&work).expect("run_batch");
        assert!(res.overflow_rescans > 0, "expected overflow rescans");
        assert_eq!(res.unique_hashes, count as u64);
        let got: BTreeSet<u32> = res.candidates.iter().copied().collect();
        let exp: BTreeSet<u32> = expected.iter().copied().collect();
        assert_eq!(got, exp, "overflow candidate set mismatch");
        assert_eq!(res.candidates.len(), got.len());
    }

    // repeated dispatch
    {
        let mut prefix = [0u8; 76];
        for (i, b) in prefix.iter_mut().enumerate() {
            *b = ((i * 13 + 5) & 0xff) as u8;
        }
        let work = make_work(&prefix, 0, 256, 0x0000ffff);
        let mut ctx = OpenClBackend::new(None).expect("context");
        let r1 = ctx.run_batch(&work).expect("first");
        let r2 = ctx.run_batch(&work).expect("second");
        assert_eq!(r1.candidates, r2.candidates);
        assert_eq!(r1.unique_hashes, r2.unique_hashes);
    }

    // range edge last nonce equality
    {
        let mut prefix = [0u8; 76];
        for (i, b) in prefix.iter_mut().enumerate() {
            *b = ((i * 31 + 7) & 0xff) as u8;
        }
        let base = 0xffffff80u32;
        let count = 128u32;
        let last_nonce = base.wrapping_add(count - 1);
        let mut full = [0u8; 80];
        full[..76].copy_from_slice(&prefix);
        full[76] = (last_nonce >> 24) as u8;
        full[77] = (last_nonce >> 16) as u8;
        full[78] = (last_nonce >> 8) as u8;
        full[79] = last_nonce as u8;
        let h = sha256d(&full);
        let word = u32::from_be_bytes([h[28], h[29], h[30], h[31]]);
        let target_word0 = word.swap_bytes();

        let work = make_work(&prefix, base, count, target_word0);
        let expected = independent_candidates(&prefix, base, count, target_word0);

        let mut ctx = OpenClBackend::new(None).expect("context");
        let res = ctx.run_batch(&work).expect("run_batch");
        let got: BTreeSet<u32> = res.candidates.iter().copied().collect();
        let exp: BTreeSet<u32> = expected.iter().copied().collect();
        assert_eq!(got, exp);
        assert!(res.candidates.contains(&last_nonce));
    }

    // odd counts
    {
        let mut prefix = [0u8; 76];
        for (i, b) in prefix.iter_mut().enumerate() {
            *b = ((i * 19 + 3) & 0xff) as u8;
        }
        for &count in &[1u32, 7, 31, 63, 65, 100, 1000] {
            let target = 0x0000ffffu32;
            let work = make_work(&prefix, 0, count, target);
            let expected = independent_candidates(&prefix, 0, count, target);
            let mut ctx = OpenClBackend::new(None).expect("context");
            let res = ctx.run_batch(&work).expect("run_batch");
            assert_eq!(res.unique_hashes, count as u64);
            let got: BTreeSet<u32> = res.candidates.iter().copied().collect();
            let exp: BTreeSet<u32> = expected.iter().copied().collect();
            assert_eq!(got, exp, "mismatch for count {}", count);
        }
    }

    // random seeds
    {
        for seed in 0u32..8 {
            let mut prefix = [0u8; 76];
            for (i, b) in prefix.iter_mut().enumerate() {
                *b = ((seed.wrapping_mul(37).wrapping_add(i as u32 * 13)) & 0xff) as u8;
            }
            let count = 256u32 + (seed % 5) * 17;
            let base = seed.wrapping_mul(1000);
            let target = 0x0003ffffu32;
            let work = make_work(&prefix, base, count, target);
            let expected = independent_candidates(&prefix, base, count, target);
            let mut ctx = OpenClBackend::new(None).expect("context");
            let res = ctx.run_batch(&work).expect("run_batch");
            assert_eq!(res.unique_hashes, count as u64);
            let got: BTreeSet<u32> = res.candidates.iter().copied().collect();
            let exp: BTreeSet<u32> = expected.iter().copied().collect();
            assert_eq!(got, exp, "seed {} mismatch", seed);
        }
    }

    // invalid range rejected
    {
        let prefix = [0u8; 76];
        let mut work = make_work(&prefix, 0, 0, 1);
        let mut ctx = OpenClBackend::new(None).expect("context");
        assert!(ctx.run_batch(&work).is_err());

        work = make_work(&prefix, 2, u32::MAX, 1);
        assert!(ctx.run_batch(&work).is_err());
    }

    // reset between batches
    {
        let mut prefix = [0u8; 76];
        for (i, b) in prefix.iter_mut().enumerate() {
            *b = ((i * 37 + 11) & 0xff) as u8;
        }
        let work = make_work(&prefix, 0, 256, 0x0000ffff);
        let mut ctx = OpenClBackend::new(None).expect("context");
        let r1 = ctx.run_batch(&work).expect("first");
        let r2 = ctx.run_batch(&work).expect("second");
        assert_eq!(r1.candidates, r2.candidates);
        assert_eq!(r1.unique_hashes, r2.unique_hashes);
    }

    // zero target no match
    {
        let mut prefix = [0u8; 76];
        for (i, b) in prefix.iter_mut().enumerate() {
            *b = ((i * 37 + 11) & 0xff) as u8;
        }
        let work = make_work(&prefix, 0, 256, 0u32);
        let mut ctx = OpenClBackend::new(None).expect("context");
        let res = ctx.run_batch(&work).expect("run_batch");
        assert!(res.candidates.is_empty());
        assert_eq!(res.unique_hashes, 256);
    }

    // cancellation
    {
        let stop = Arc::new(AtomicBool::new(true));
        let prefix = [0u8; 76];
        let work = make_work(&prefix, 0, 100000, u32::MAX);
        let mut ctx = OpenClBackend::new(None).expect("context");
        ctx.set_stop(stop);
        let res = ctx.run_batch(&work);
        assert!(res.is_err(), "expected cancellation error");
    }

    // repeated init
    {
        let c1 = OpenClBackend::new(None).expect("first");
        drop(c1);
        let c2 = OpenClBackend::new(None).expect("second");
        drop(c2);
    }

    // all-match overflow
    {
        let prefix = [0u8; 76];
        let count = 128u32;
        let work = make_work(&prefix, 0, count, u32::MAX);
        let mut ctx = OpenClBackend::new(None).expect("context");
        let res = ctx.run_batch(&work).expect("run_batch");
        assert_eq!(res.unique_hashes, count as u64);
        assert!(res.overflow_rescans > 0);
    }

    std::env::remove_var("GBTC_OPENCL_KERNEL");
    std::env::remove_var("GBTC_OPENCL_SIMD");
}

#[test]
fn conformance_unrolled() {
    run_conformance("unrolled", None);
}

#[test]
fn conformance_looped() {
    run_conformance("looped", None);
}

#[test]
fn conformance_dual() {
    run_conformance("dual", None);
}

#[test]
fn conformance_pre3() {
    run_conformance("pre3", None);
}

#[test]
fn conformance_simd16() {
    run_conformance("unrolled", Some("16"));
}
