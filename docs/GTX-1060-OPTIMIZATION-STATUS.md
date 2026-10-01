# GTX 1060 optimization campaign — live status

- Campaign start (UTC): 2026-10-01T18:30:00Z — deadline: 2026-10-03T18:30:00Z.
- Branch: `gtx-1060-6gb`, HEAD `4b590a6`, origin `https://github.com/levi-cm/Gamble-BTC.git`
  (local == remote at start; working tree clean).
- Baseline/rollback image: `gamble-btc:latest`
  `sha256:43ba044b973653875a3200e57189ea311a87e44671a8e79cd5736437fd2eef82`
  (OpenCL unrolled, local 64, batch 2^24, poll 1000us). Sidecar:
  `sha256:f45e1eed4ebea51f52fc3343e43c3607549e56ac242a77f63f9a3e521c5e4b92`.
- Live baseline at start: ~608 MH/s (status avg 609.08, 300 samples), GPU 72 C,
  1835 MHz, 97% util, ~104 W / 120 W limit, no thermal-slowdown flags.
- Thermal guard: `scripts/thermal-guard.sh`, state in
  `bench-results/campaign-20261001/`, 2 s NVML samples, soft >75 Cx3, hard 78 C
  or any SW/HW thermal-slowdown flag, cooldown <=70 C for 60 s. Acts ONLY on the
  workload named in `gpu-workload.lock`. Guard PID in `guard.pid`.
- Campaign lock: this agent owns the campaign. GPU lock: empty = no owned
  experiment workload; production `gamble-btc` service untouched.
- Current phase: 4–16 h (Path 1). Next action: commit+push M2 (CUDA backend),
  deploy cuda/256 to live mining as E06 validation (45 min: submits, CPU,
  thermals), then SASS-level arithmetic work + 60-min confirmation runs.

## Experiment ledger

| ID | Hypothesis | Result |
| --- | --- | --- |
| E01 | Baseline gates: probe, conformance (5 variants), 60 s unrolled/64 control | PASS. Probe OK (NVIDIA CUDA / GTX 1060 / OpenCL 3.0). All conformance passed. Baseline 609.77 MH/s, 54→70 C. Queue mode `default-no-throttle-ext` (no cl_khr_throttle_hints on NVIDIA). Device max_wg=256 → local 512 illegal here. |
| E02 | Variant/dispatch screen, 30 s runs, serialized | unrolled/64: 611.39/609.71. pre3: 612.36 (INCONCLUSIVE, in noise). looped: 113.84 REJECT. dual: 571.63 REJECT. ls32: 594.16 REJECT. ls128: 609.21 REJECT (no gain). ls256: 629.31/630.08 KEEP candidate (+3.2%, needs 30–60 min confirmation). cpu_steady ~1.15–1.35% in bench loop (includes no stratum/UI). |
| E03 | Profiler capability check (nvprof on screening cuda binary) | GPU kernel/memcpy trace WORKS (111 us kernel for 65536 nonces; ~1 us transfers; ~50 us inter-kernel gap → gaps NOT the bottleneck at 2^24 batches). Hardware counters BLOCKED (CUPTI error 4212, admin policy) — recorded, will not retry; use ptxas/SASS + timing instead. |
| E04 | ABBA cuda-native vs ocl-256 vs ocl-64, 30 s, mirrored order | cuda-native: 625.81/625.19, host cpu 0.67/0.66%. ocl-256: 629.28/630.23. ocl-64: 608.84/608.89. OpenCL-256 wins by ~0.7%. CUDA API alone is not the gain; continue Path 1 for lower CPU waits, CUDA block-size tuning and SASS control. |
| E05 | CUDA backend in miner (Driver API, blocking sync, sm_61 cubin) vs ocl-256, 30 s mirrored | cuda-256: 631.98/631.89 @ cpu_steady 0.65/0.69%. cuda-64: 625.13/624.50 @ 0.69%. ocl-256: 631.51/630.54 @ 1.18%. KEEP cuda-256: throughput parity (+0.2%, noise) with ~45% lower host CPU and no JIT build cost (cpu_full 0.82 vs 3.7). Gates: new shared conformance core passes for cuda blocks 64+256 and all 5 OpenCL variants; full `make test` exit 0; ASan host tests exit 0 but GPU inits skip under ASan (ASan+libcuda incompatibility, tooling limit). New image `gamble-btc:latest` sha256:0760c907...; rollback tagged `rollback-vpn-20261001` (sha256:43ba044b...). |
