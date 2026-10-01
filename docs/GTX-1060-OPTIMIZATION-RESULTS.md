# GTX 1060 optimization — sanitized milestone evidence

## Campaign

- Start: 2026-10-01T18:30:00Z. Deadline: 2026-10-03T18:30:00Z.
- Branch `gtx-1060-6gb`, start HEAD `4b590a6`.
- Baseline image `gamble-btc:latest`
  `sha256:43ba044b973653875a3200e57189ea311a87e44671a8e79cd5736437fd2eef82`.

## Baseline (live miner, start of campaign)

- ~608 MH/s sustained (status avg 609.08 over 300 samples, uptime 409 s).
- GPU 72 C, 1835 MHz SM, 97% util, ~104 W of 120 W, throttle reasons inactive.
- Config: backend opencl, kernel unrolled, local 64, batch 16777216, poll 1000 us.

## Milestones

- M1 (pushed `4c3bb6e`): NVIDIA harness + thermal guard + baseline correctness gates.
- M2 (pushed `c78a054`): Pascal sm_61 CUDA backend (Driver API, blocking sync).
  ABBA: cuda-256 631.98/631.89 @ 0.65–0.69% CPU vs ocl-256 631.51/630.54 @
  1.18%. Shared conformance core passes for both backends. Full `make test`
  exit 0. Rollback image tagged `gamble-btc:rollback-vpn-20261001`.
- E06 live (running): cuda/256 in production, ~630–632 MH/s, miner CPU 0.750%
  of one core over 60 s (sidecar 1.404% separately), GPU 73–74 C, 1835 MHz,
  ~108 W, no throttle flags. Submits: none yet (expected ~1 per 19 h at
  pool diff 10000; mock-Stratum easy-target test still to do).
- E07 (rejected): interleaved 2-nonce kernel, 606.2 MH/s — issue-bound,
  not latency-bound. Record: `scripts/gen-cuda-dual.py`, `bench-results/`
  cubin/logs, 9/9 hashlib oracle checks.
- E08: batch 2^26 KEEP (cuda-256: 635.2 @ 0.21% CPU; ocl-256: 633.6 @ 0.88%).
  Small batches REJECT (2^22: 619.6 @ 2.35%). cuda-128 REJECT (623.8).
- E09 (running): 60-min production confirmation, cuda-256 + batch 2^26.
- E10 (implemented, image `535d9b83` built, pending deploy): overflow flag +
  subrange rescan recovery; GPU-gated test passes (raw=25 fully recovered).
