# Hardware Matrix

Support tiers:

- Tier A: verified on real hardware with command output.
- Tier B: expected from Linux driver/API support, not yet verified here.
- Tier C: not targeted for iGPU mining in this project.
- Tier D: unsupported or known broken.

| Tier | Hardware | Backend | Status | Evidence |
| --- | --- | --- | --- | --- |
| A | Intel HD Graphics 4600 (Haswell GT2) with Mesa crocus | GLES 3.2 compute | Verified on the current host-specific Compose setup | `GBTC_PROBE_ONLY=1 docker compose run --rm miner` reported `gles available Intel / Mesa Intel(R) HD Graphics 4600 (HSW GT2) / OpenGL ES 3.2 Mesa 25.0.7-2`. |
| A | Intel Iris Xe / Xe-LP on Mesa iris | GLES 3.2 compute | Verified on the CachyOS laptop Docker runtime | `docker run --rm --network host --env-file .env -e GBTC_PROBE_ONLY=1 --device /dev/dri/card1:/dev/dri/card1 --device /dev/dri/renderD128:/dev/dri/renderD128 --group-add 983 --group-add 987 gamble-btc:latest` reported `gles available Intel / Mesa Intel(R) Iris(R) Xe Graphics (TGL GT2) / OpenGL ES 3.2 Mesa 25.0.7-2`. |
| B | AMD GCN/Vega/RDNA APUs on Mesa | Vulkan or GLES | Expected, unverified | Needs real-host probe and mining smoke output before any Tier A claim. |
| B | New Intel Xe/Arc, Meteor/Lunar/Panther Lake class iGPUs | Vulkan | Expected by design, unverified | Do not claim verified support until tested on real hardware. |
| B | AMD RDNA3/RDNA3.5 Ryzen AI class iGPUs | Vulkan | Expected by design, unverified | Do not claim verified support until tested on real hardware. |
| C | Pre-Sandy Bridge Intel and pre-GCN AMD iGPUs | none | Not targeted | No CPU mining backend is provided. This project is iGPU-only; use existing CPU miners if CPU mining is desired. |

Current backend implementation:

- `gles`: implemented and probe-tested on the current HD 4600 and Iris Xe hosts.
- `vulkan`: probe-visible placeholder, not implemented yet.
- `opencl`: probe-visible placeholder, not implemented yet.
- `cpu`: intentionally not implemented; CPU nonce search is out of scope.

Current HD 4600 benchmark note:

- Default `GBTC_GLES_KERNEL=unrolled`, `GBTC_GLES_LOCAL_SIZE=64`, and
  `GBTC_BATCH_NONCES=16777216` measured 14.714 MH/s over a 60.43 second
  uncontended `scripts/bench-current.sh` run on 2026-06-10.
- Tuned `GBTC_GLES_KERNEL=altbool`, `GBTC_GLES_LOCAL_SIZE=16`, and
  `GBTC_BATCH_NONCES=16777216` measured 15.022 MH/s over a 60.31 second
  uncontended matrix run on 2026-06-10 and is the current Compose default.
- Short compile-smoke matrix coverage showed all five GLES kernel variants
  compile across local sizes 8, 16, 32, 64, 128, and 256. Default selection is
  based on the later 60-second full-batch matrix, not the 1-second smoke run.

Current Iris Xe benchmark note:

- Tuned `GBTC_GLES_KERNEL=altbool`, `GBTC_GLES_LOCAL_SIZE=16`, and
  `GBTC_BATCH_NONCES=16777216` measured 104.381 MH/s over a 60.11 second
  confirmation run on 2026-06-11.
- Adding `MESA_NO_ERROR=1` with the same tuned GLES settings measured
  106.473 MH/s over a 60.04 second confirmation run on 2026-06-11 and is the
  current example/Compose default for this Mesa path.
- A 30-case short matrix compiled all five GLES kernel variants across local
  sizes 8, 16, 32, 64, 128, and 256.
- 2026-09-21 (docker image with Mesa 26.1.2 backports, `RUSTICL_ENABLE=iris`,
  persistent shader cache, mem 4g/cpus 4.0): `altbool/16/16M` measured
  126.670 MH/s over 60.13 s; `unrolled/16/16M` 125.126 MH/s; autotune sweep
  8..256 all within 122.3-124.6 MH/s (local size no longer significant).
  Batch 33M measured 124.521 MH/s (larger batches do not help: both paths are
  compute-saturated, sync overhead negligible). New `opencl` backend (Mesa
  Rusticl, `ocl-unrolled`, local 64/128/256) measured ~120.5 MH/s sustained
  and passes CPU-rechecked conformance on host Intel NEO too.
  Winner and current default stays `gles/altbool/16/16M`.
- 2026-09-21 part 2 (Intel NEO 26.35.39758.10 + IGC 2.41.5 fetched as pinned
  upstream .debs; backend prefers NEO over Rusticl): `opencl/ocl-unrolled`
  local 64 measured **182.978 MH/s**, local 128 178.801/183.078 MH/s over
  60 s runs — the 160 MH/s target is exceeded and `opencl` is the new
  default on this laptop.
- NEO's completion wait spin-burns a CPU core (`clFinish` poll loop, upstream
  issue intel/compute-runtime#363; `CL_QUEUE_THROTTLE_LOW_KHR` queue
  creation is rejected by NEO 26.35). Fixed application-side: the backend
  chains each batch behind events and sleep-polls (`nanosleep`,
  `GBTC_OPENCL_POLL_US` default 1000) instead of blocking waits. Result:
  **184.022 MH/s at 1.78% container CPU** (was ~100%). Event API absence
  falls back to blocking waits; `GBTC_OPENCL_DEBUG=1` traces submission.

Push toward 240 MH/s (2026-09-30) — why it is not reachable in software here:

- ISA dump tooling added (`scripts/igc-dump.sh`, IGC `ShaderDumpEnable`). For
  the default kernel IGC picks `simd_size=16`, `grf_count=128`, and
  `eu_thread_count=7`, which is the maximum threads-per-EU on TGLLP. Occupancy
  is therefore already saturated; it was the leading hypothesis and it is
  disproven.
- Forcing `GBTC_OPENCL_SIMD=32` selects `simd_size=32` while keeping
  `eu_thread_count=7`, but the instruction count doubles (4029 -> 8056) and
  spills appear, i.e. IGC splits the wide work-item back down. Measured
  throughput is unchanged (interleaved A/B medians 98.5 auto / 101.7 simd16 /
  99.3 simd32).
- Kernel variants added (`GBTC_OPENCL_KERNEL=unrolled|looped|dual`), all
  CPU-recheck conformance-tested on NEO and Rusticl. `looped` is much slower
  (IGC does not unroll it well) and `dual` (two nonces per item) is also
  slower, so `unrolled` stays the default.
- Opcode census of the GenISA: 1056 `rol`, 1105 `add`, 1055 `xor`, 487 `and`,
  122 `or`, and **zero `lop3`**. Rewriting CH/MAJ into the classic
  LOP3-friendly forms did not make IGC fuse them (4045 vs 4029 instructions),
  so the multi-instruction CH/MAJ cost is compiler-bound, not source-bound.
- Conclusion: the OpenCL kernel is at its practical ceiling for IGC on this
  part. The 184 MH/s figure stands. Reaching 240 MH/s would need a
  structurally different approach (hand-written GenISA / Xe-assembly kernel)
  or more GPU power headroom, not more compiler flags.
- Environment caveat: this laptop shares a 35 W package budget
  (`throttle_reason_pl1=1`, `thermal=0`). Under concurrent CPU load the iGPU
  drops to 300-850 MHz and throughput collapses (52-138 MH/s, high variance).
  Meaningful benchmarks require a quiet machine; `scripts/bench-ab.sh`
  interleaves configs and reports medians to survive residual drift.
- Host-level (explicitly approved, AC-only): `/etc/tlp.d/10-igpu-miner.conf`
  pins `INTEL_GPU_MIN/MAX/BOOST_FREQ_ON_AC=1300` so sustained compute holds
  max clocks; `_ON_BAT`/`_ON_SAV` restore the full 100-1300 range. Verified
  both directions via `tlp bat`/`tlp ac`: min floor reads 100 on battery
  profile, 1300 on AC. TLP auto-switches on physical plug/unplug, so the pin
  can never leak into unplugged operation. (TLP validates min+max+boost as a
  triple — all three must be set per profile or the write is skipped.)
