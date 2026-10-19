use gbtc_backend_vulkan::{device, probe, VulkanContext};
use gbtc_core::backend::ComputeBackend;
use gbtc_core::work::build_header_midstate;
use gbtc_core::WorkBatch;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

fn require_gpu() -> bool {
    std::env::var("GBTC_REQUIRE_GPU_TESTS").ok().as_deref() == Some("1")
}

fn gpu_available() -> bool {
    probe().is_ok()
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
fn embedded_shader_decodes() {
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/sha256d.spv"));
    let words = gbtc_backend_vulkan::decode_spirv(bytes).expect("decode");
    assert!(!words.is_empty());
    assert_eq!(words[0], 0x07230203);
}

#[test]
fn limits_validation_rejects_too_small() {
    let mut limits = device::Limits {
        max_compute_work_group_invocations: 32,
        max_compute_work_group_size: [32, 1, 1],
        max_compute_work_group_count: [1, 1, 1],
        max_storage_buffer_range: 1 << 20,
        max_push_constants_size: 88,
        min_storage_buffer_offset_alignment: 4,
        non_coherent_atom_size: 4,
    };
    assert!(device::validate_limits(&limits).is_err());
    limits.max_compute_work_group_invocations = 64;
    limits.max_compute_work_group_size = [64, 1, 1];
    assert!(device::validate_limits(&limits).is_ok());
}

#[test]
fn unsupported_limits_reject() {
    let limits = device::Limits {
        max_compute_work_group_invocations: 64,
        max_compute_work_group_size: [64, 1, 1],
        max_compute_work_group_count: [1, 1, 1],
        max_storage_buffer_range: 1024,
        max_push_constants_size: 88,
        min_storage_buffer_offset_alignment: 4,
        non_coherent_atom_size: 4,
    };
    assert!(device::validate_limits(&limits).is_err());
}

#[test]
fn wrong_device_fails() {
    let res = VulkanContext::new(Some("definitely-not-a-real-device-xyz"));
    assert!(res.is_err(), "expected failure for wrong device");
}

#[test]
fn probe_or_dispatch() {
    if !gpu_available() {
        if require_gpu() {
            panic!("GPU required but unavailable");
        }
        eprintln!("SKIP: no Vulkan GPU available");
        return;
    }
    let info = probe().expect("probe");
    assert_eq!(info.backend, "vulkan");
    assert!(!info.device.is_empty());
    assert!(!info.driver.is_empty());
}

#[test]
fn small_known_header() {
    if !gpu_available() {
        if require_gpu() {
            panic!("GPU required but unavailable");
        }
        eprintln!("SKIP: no Vulkan GPU available");
        return;
    }
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
    let mut ctx = VulkanContext::new(None).expect("context");
    let res = ctx.run_batch(&work).expect("run_batch");
    assert_eq!(res.unique_hashes, work.nonce_count as u64);

    let got: BTreeSet<u32> = res.candidates.iter().copied().collect();
    let exp: BTreeSet<u32> = expected.iter().copied().collect();
    assert_eq!(got, exp, "candidate set mismatch");
}

#[test]
fn complete_set_easy_target() {
    if !gpu_available() {
        if require_gpu() {
            panic!("GPU required but unavailable");
        }
        eprintln!("SKIP: no Vulkan GPU available");
        return;
    }
    let prefix = [0u8; 76];
    let count = 512u32;
    let work = make_work(&prefix, 0, count, u32::MAX);
    let expected = independent_candidates(&prefix, 0, count, u32::MAX);

    let mut ctx = VulkanContext::new(None).expect("context");
    let res = ctx.run_batch(&work).expect("run_batch");
    assert_eq!(res.candidates.len(), count as usize);
    assert_eq!(res.unique_hashes, count as u64);
    let got: BTreeSet<u32> = res.candidates.iter().copied().collect();
    let exp: BTreeSet<u32> = expected.iter().copied().collect();
    assert_eq!(got, exp);
}

#[test]
fn overflow_dedup_complete() {
    if !gpu_available() {
        if require_gpu() {
            panic!("GPU required but unavailable");
        }
        eprintln!("SKIP: no Vulkan GPU available");
        return;
    }
    let prefix = [7u8; 76];
    let base = 1000u32;
    let count = 2000u32;
    let work = make_work(&prefix, base, count, u32::MAX);
    let expected = independent_candidates(&prefix, base, count, u32::MAX);

    let mut ctx = VulkanContext::new(None).expect("context");
    let res = ctx.run_batch(&work).expect("run_batch");
    assert!(res.overflow_rescans > 0, "expected overflow rescans");
    assert_eq!(res.unique_hashes, count as u64);

    let got: BTreeSet<u32> = res.candidates.iter().copied().collect();
    let exp: BTreeSet<u32> = expected.iter().copied().collect();
    assert_eq!(got, exp, "overflow candidate set mismatch");
    assert_eq!(res.candidates.len(), got.len());
}

#[test]
fn repeated_dispatch_stable() {
    if !gpu_available() {
        if require_gpu() {
            panic!("GPU required but unavailable");
        }
        eprintln!("SKIP: no Vulkan GPU available");
        return;
    }
    let mut prefix = [0u8; 76];
    for (i, b) in prefix.iter_mut().enumerate() {
        *b = ((i * 13 + 5) & 0xff) as u8;
    }
    let work = make_work(&prefix, 0, 256, 0x0000ffff);

    let mut ctx = VulkanContext::new(None).expect("context");
    let r1 = ctx.run_batch(&work).expect("first");
    let r2 = ctx.run_batch(&work).expect("second");
    assert_eq!(r1.candidates, r2.candidates);
    assert_eq!(r1.unique_hashes, r2.unique_hashes);
}

#[test]
fn partial_init_failure_cleanup() {
    let res = VulkanContext::new(Some("no-such-device-for-partial-init"));
    assert!(res.is_err());
}

#[test]
fn range_edge_last_nonce_equality() {
    if !gpu_available() {
        if require_gpu() {
            panic!("GPU required but unavailable");
        }
        eprintln!("SKIP: no Vulkan GPU available");
        return;
    }
    let mut prefix = [0u8; 76];
    for (i, b) in prefix.iter_mut().enumerate() {
        *b = ((i * 31 + 7) & 0xff) as u8;
    }
    let base = 0xffffff80u32;
    let count = 128u32;
    // target equality for the final nonce (no skip)
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

    let mut ctx = VulkanContext::new(None).expect("context");
    let res = ctx.run_batch(&work).expect("run_batch");
    let got: BTreeSet<u32> = res.candidates.iter().copied().collect();
    let exp: BTreeSet<u32> = expected.iter().copied().collect();
    assert_eq!(got, exp);
    assert!(res.candidates.contains(&last_nonce));
}

#[test]
fn odd_counts_partial_groups() {
    if !gpu_available() {
        if require_gpu() {
            panic!("GPU required but unavailable");
        }
        eprintln!("SKIP: no Vulkan GPU available");
        return;
    }
    let mut prefix = [0u8; 76];
    for (i, b) in prefix.iter_mut().enumerate() {
        *b = ((i * 19 + 3) & 0xff) as u8;
    }
    for &count in &[1u32, 7, 31, 63, 65, 100, 1000] {
        let target = 0x0000ffffu32;
        let work = make_work(&prefix, 0, count, target);
        let expected = independent_candidates(&prefix, 0, count, target);
        let mut ctx = VulkanContext::new(None).expect("context");
        let res = ctx.run_batch(&work).expect("run_batch");
        assert_eq!(res.unique_hashes, count as u64);
        let got: BTreeSet<u32> = res.candidates.iter().copied().collect();
        let exp: BTreeSet<u32> = expected.iter().copied().collect();
        assert_eq!(got, exp, "mismatch for count {}", count);
    }
}

#[test]
fn random_seeds_full_target_false_positives() {
    if !gpu_available() {
        if require_gpu() {
            panic!("GPU required but unavailable");
        }
        eprintln!("SKIP: no Vulkan GPU available");
        return;
    }
    for seed in 0u32..8 {
        let mut prefix = [0u8; 76];
        for (i, b) in prefix.iter_mut().enumerate() {
            *b = ((seed.wrapping_mul(37).wrapping_add(i as u32 * 13)) & 0xff) as u8;
        }
        let count = 256u32 + (seed % 5) * 17;
        let target = 0x0003ffffu32;
        let work = make_work(&prefix, seed.wrapping_mul(1000), count, target);
        let expected = independent_candidates(&prefix, seed.wrapping_mul(1000), count, target);
        let mut ctx = VulkanContext::new(None).expect("context");
        let res = ctx.run_batch(&work).expect("run_batch");
        assert_eq!(res.unique_hashes, count as u64);
        let got: BTreeSet<u32> = res.candidates.iter().copied().collect();
        let exp: BTreeSet<u32> = expected.iter().copied().collect();
        assert_eq!(got, exp, "seed {} mismatch", seed);
    }
}

#[test]
fn range_invalid_rejected() {
    if !gpu_available() {
        if require_gpu() {
            panic!("GPU required but unavailable");
        }
        eprintln!("SKIP: no Vulkan GPU available");
        return;
    }
    let prefix = [0u8; 76];
    let mut work = make_work(&prefix, 0, 0, 1);
    // zero count
    let mut ctx = VulkanContext::new(None).expect("context");
    assert!(ctx.run_batch(&work).is_err());

    // overflowing range
    work = make_work(&prefix, 2, u32::MAX, 1);
    assert!(ctx.run_batch(&work).is_err());
}

#[test]
fn reset_between_batches() {
    if !gpu_available() {
        if require_gpu() {
            panic!("GPU required but unavailable");
        }
        eprintln!("SKIP: no Vulkan GPU available");
        return;
    }
    let mut prefix = [0u8; 76];
    for (i, b) in prefix.iter_mut().enumerate() {
        *b = ((i * 37 + 11) & 0xff) as u8;
    }
    let work = make_work(&prefix, 0, 256, 0x0000ffff);

    let mut ctx = VulkanContext::new(None).expect("context");
    let r1 = ctx.run_batch(&work).expect("first");
    let r2 = ctx.run_batch(&work).expect("second");
    assert_eq!(r1.candidates, r2.candidates);
    assert_eq!(r1.unique_hashes, r2.unique_hashes);
}

#[test]
fn zero_target_no_match() {
    if !gpu_available() {
        if require_gpu() {
            panic!("GPU required but unavailable");
        }
        eprintln!("SKIP: no Vulkan GPU available");
        return;
    }
    let mut prefix = [0u8; 76];
    for (i, b) in prefix.iter_mut().enumerate() {
        *b = ((i * 37 + 11) & 0xff) as u8;
    }
    let work = make_work(&prefix, 0, 256, 0u32);
    let mut ctx = VulkanContext::new(None).expect("context");
    let res = ctx.run_batch(&work).expect("run_batch");
    assert!(res.candidates.is_empty());
    assert_eq!(res.unique_hashes, 256);
}

#[test]
fn cancellation_stops() {
    if !gpu_available() {
        if require_gpu() {
            panic!("GPU required but unavailable");
        }
        eprintln!("SKIP: no Vulkan GPU available");
        return;
    }
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let prefix = [0u8; 76];
    let work = make_work(&prefix, 0, 100000, 0xffffffff);
    let mut ctx = VulkanContext::new(None).expect("context");
    ctx.set_stop(stop);
    let res = ctx.run_batch(&work);
    assert!(res.is_err(), "expected cancellation error");
}

#[test]
fn queued_two_outstanding_tickets() {
    if !gpu_available() {
        if require_gpu() {
            panic!("GPU required but unavailable");
        }
        eprintln!("SKIP: no Vulkan GPU available");
        return;
    }
    let mut prefix = [0u8; 76];
    for (i, b) in prefix.iter_mut().enumerate() {
        *b = ((i * 37 + 11) & 0xff) as u8;
    }
    let work1 = make_work(&prefix, 0, 256, 0x0000ffff);
    let work2 = make_work(&prefix, 256, 256, 0x0000ffff);

    let mut ctx = VulkanContext::new(None).expect("context");
    let t1 = ctx.submit_batch(&work1).expect("submit1");
    let t2 = ctx.submit_batch(&work2).expect("submit2");
    assert_ne!(t1, t2);

    let r1 = ctx.retire_batch(t1).expect("retire1");
    let r2 = ctx.retire_batch(t2).expect("retire2");
    assert_eq!(r1.unique_hashes, work1.nonce_count as u64);
    assert_eq!(r2.unique_hashes, work2.nonce_count as u64);
}

#[test]
fn queued_independent_output() {
    if !gpu_available() {
        if require_gpu() {
            panic!("GPU required but unavailable");
        }
        eprintln!("SKIP: no Vulkan GPU available");
        return;
    }
    let mut prefix = [0u8; 76];
    for (i, b) in prefix.iter_mut().enumerate() {
        *b = ((i * 13 + 5) & 0xff) as u8;
    }
    let work1 = make_work(&prefix, 0, 512, 0xffffffff);
    let work2 = make_work(&prefix, 0, 512, 0x00000000);

    let mut ctx = VulkanContext::new(None).expect("context");
    let t1 = ctx.submit_batch(&work1).expect("submit1");
    let t2 = ctx.submit_batch(&work2).expect("submit2");
    let r1 = ctx.retire_batch(t1).expect("retire1");
    let r2 = ctx.retire_batch(t2).expect("retire2");
    // target 0 means no candidate can pass (unless hash word <=0), so r2 empty
    assert!(r2.candidates.is_empty());
    assert_eq!(r1.unique_hashes, 512);
}

#[test]
fn queued_invalid_and_double_ticket() {
    if !gpu_available() {
        if require_gpu() {
            panic!("GPU required but unavailable");
        }
        eprintln!("SKIP: no Vulkan GPU available");
        return;
    }
    let prefix = [0u8; 76];
    let work = make_work(&prefix, 0, 64, 0x00ffffff);
    let mut ctx = VulkanContext::new(None).expect("context");
    let t = ctx.submit_batch(&work).expect("submit");
    let _ = ctx.retire_batch(t).expect("retire");
    // double retirement
    assert!(ctx.retire_batch(t).is_err());
    // unknown ticket
    assert!(ctx
        .retire_batch(gbtc_core::backend::BatchTicket(999999))
        .is_err());
}

#[test]
fn queued_overflow() {
    if !gpu_available() {
        if require_gpu() {
            panic!("GPU required but unavailable");
        }
        eprintln!("SKIP: no Vulkan GPU available");
        return;
    }
    let prefix = [7u8; 76];
    let base = 1000u32;
    let count = 2000u32;
    let work = make_work(&prefix, base, count, u32::MAX);
    let expected = independent_candidates(&prefix, base, count, u32::MAX);

    let mut ctx = VulkanContext::new(None).expect("context");
    let t = ctx.submit_batch(&work).expect("submit");
    let res = ctx.retire_batch(t).expect("retire");
    assert!(res.overflow_rescans > 0);
    assert_eq!(res.unique_hashes, count as u64);
    let got: BTreeSet<u32> = res.candidates.iter().copied().collect();
    let exp: BTreeSet<u32> = expected.iter().copied().collect();
    assert_eq!(got, exp);
}

#[test]
fn queued_cancellation() {
    if !gpu_available() {
        if require_gpu() {
            panic!("GPU required but unavailable");
        }
        eprintln!("SKIP: no Vulkan GPU available");
        return;
    }
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let prefix = [0u8; 76];
    let work = make_work(&prefix, 0, 100000, 0xffffffff);
    let mut ctx = VulkanContext::new(None).expect("context");
    ctx.set_stop(stop);
    let res = ctx.submit_batch(&work);
    assert!(res.is_err(), "expected cancellation error");
}

#[test]
fn synchronous_adapter_preserves_pending_tickets() {
    if !gpu_available() {
        assert!(!require_gpu(), "GPU required but unavailable");
        return;
    }
    let prefix = [19u8; 76];
    let a = make_work(&prefix, 100, 600, u32::MAX);
    let b = make_work(&prefix, 900, 700, u32::MAX);
    let mut ctx = VulkanContext::new(None).unwrap();
    let ta = ctx.submit_batch(&a).unwrap();
    let tb = ctx.submit_batch(&b).unwrap();
    assert!(
        ctx.run_batch(&a).is_err(),
        "full queue must reject synchronous overwrite"
    );
    assert_eq!(
        ctx.retire_batch(ta).unwrap().candidates,
        independent_candidates(&prefix, 100, 600, u32::MAX)
    );
    assert_eq!(
        ctx.retire_batch(tb).unwrap().candidates,
        independent_candidates(&prefix, 900, 700, u32::MAX)
    );
}

#[test]
fn cancellation_during_retirement_poisons_context() {
    if !gpu_available() {
        assert!(!require_gpu(), "GPU required but unavailable");
        return;
    }
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let work = make_work(&[3; 76], 0, 2000, 0);
    let mut ctx = VulkanContext::new(None).unwrap();
    ctx.set_stop(stop.clone());
    let ticket = ctx.submit_batch(&work).unwrap();
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(ctx.retire_batch(ticket).is_err());
    assert!(ctx.is_poisoned());
    ctx.clear_stop();
    assert!(ctx.submit_batch(&work).is_err());
}
