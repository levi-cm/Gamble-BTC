# GTX 1060 optimization — sanitized milestone evidence

## Campaign

- Start: 2026-10-01T18:30:00Z. Deadline: 2026-10-03T18:30:00Z.
- Branch `gtx-1060-6gb`, start HEAD `4b590a6`, end HEAD `2b021ab` (+9 commits).
- Baseline image `gamble-btc:latest`
  `sha256:43ba044b973653875a3200e57189ea311a87e44671a8e79cd5736437fd2eef82`
  (also tagged `gamble-btc:rollback-vpn-20261001`, retained).
- Final deployed image `gamble-btc:latest` `sha256:10282bf0...` (cuda/512,
  batch 2^26, reconnect fix, overflow recovery, guard tolerance).
- **Continuity note: no unattended 48 h run is claimed.** The stack sat
  Stopped for ~23 h (Oct 2 ~17:15Z → Oct 3 16:19Z) after a sidecar
  tailscaled watchdog panic and an undiagnosed 12-minute miner throughput
  decay (617 → 0.09 MH/s, no Xid errors, no OOM evidence). Thermal telemetry
  covers the whole campaign; mining evidence is fragmented by design
  (experiment pauses), route flaps, and the outage. All rates below are
  sustained warm measurements, never cold peaks.

## Baseline (live miner, start of campaign)

- ~608 MH/s sustained (status avg 609.08 over 300 samples, uptime 409 s).
- GPU 72 C, 1835 MHz SM, 97% util, ~104 W of 120 W, throttle reasons inactive.
- Config: backend opencl, kernel unrolled, local 64, batch 16777216, poll 1000 us.

## Final (production, cuda/512 + batch 2^26)

- ~662 MH/s sustained warm (E09f median 635.26 at /256; E09g/final median
  ~662.4 at /512; cold peaks to ~671 excluded from claims).
- Fraction of 1 GH/s goal: **~66%**. Gain over baseline: **+8.9%**.
- Miner CPU 0.28% of one logical CPU over 60 s live (bench-loop 0.21%);
  stretch target (≤0.5%) met. Sidecar 1.38%, thermal guard 0.15%,
  watch sampler negligible — reported separately, not divided by cores.
- GPU 73–75 C, 1835 MHz SM, 99–100% util, ~108–110 W of 120 W, throttle
  reasons zero across all telemetry. No overclocking, power/fan changes.
- Correctness: OpenCL conformance (5 variants) + CUDA conformance (blocks
  64/256/512) + overflow-recovery test pass on the GTX 1060; full
  `make test` exit 0; mock pool 15/15 and 16/16 submits verified, 0 bad;
  1 live share submitted (pool response parsed as malformed — E16 open).
- Restarts: orderly route-guard restarts during primary-exit flapping
  (failover + failback verified end to end, Mullvad re-verified each time);
  hysteresis fixes absorbed single transients with zero restarts in the
  final validation windows. Separate 23 h outage documented above.

## Bottleneck analysis (why ~660 MH/s is the measured limit)

- SASS of the deployed kernel: funnel shifts (SHF), LOP3 CH/MAJ/sigma with
  correct truth tables, IADD3 chains, hoisted LDG, K-table in cmem —
  ~18 instructions per round, 40 regs/thread, zero spills/local memory.
- 128 rounds × ~18 SASS ≈ 2300 integer instructions per nonce test.
  At 662 MH/s ≈ 1.52×10^12 instr/s vs GP106 peak INT32
  (10 SM × 128 cores × 1.835 GHz ≈ 2.35×10^12/s) ≈ **65% of peak**.
- 1 GH/s would need ~98% of theoretical peak with strictly serial
  round dependencies — infeasible. The interleaved 2-nonce experiment
  (E07, 606 MH/s at 50% occupancy vs 632 at 75%) proved the kernel is
  **issue-bound, not latency-bound**, so wider ILP cannot recover the gap.
- Round-pruning is exhausted: midstate removes the first block; pre3 saves
  3/128 rounds but measures 0 on both APIs (issue-bound); the second SHA's
  last-3-round cut is already done by the compiler (identical SASS with and
  without explicit pruning). Transfers are ~1 µs and inter-kernel gaps
  ~50 µs per 100 ms batch (<0.1%). No memory, occupancy, power, thermal,
  or CPU-wait bottleneck remains addressable within the two-path limit.

## Experiment ledger (KEEP / REJECT / status)

- E01 baseline gates: PASS (probe, 5-variant conformance, 609.77 MH/s).
- E02 dispatch screen: KEEP local-256 (+3.2%); REJECT looped (113.8),
  dual (571.6), ls32, ls128.
- E03 profiler: trace works; hardware counters blocked (CUPTI 4212).
- E04/E05 CUDA promotion: KEEP cuda-256 (parity + ~45% less CPU).
- E06/E09/E09f/E09g confirmations: 635 (256) → 662 (512) sustained.
- E07 dual-interleave: REJECT (issue-bound proof).
- E08 batches: KEEP 2^26 (635.2 @ 0.21%); REJECT 2^22, cuda-128.
- E10 overflow recovery: IMPLEMENTED + tested.
- E11 mock submit: PASS (15/15). E14 reconnect: PASS (16/16).
- E12/E13 guard hysteresis: IMPLEMENTED, flaps absorbed, suites pass.
- E15 cuda-512: KEEP (+3.9%). pre3: REJECT both APIs.
- E16 ckpool response shape: OPEN (1 live submit, response rejected as
  malformed; shape-matrix probe ready, needs a submit event or mock run).
- E17 23 h outage: documented above; no data fabricated across the gap.
- E18 revival + final soak: mining ~663 MH/s on primary, verified exit.

## Images, config, rollback, URLs

- Running: `gamble-btc:latest` (`10282bf0`) + sidecar `6792e77b`,
  `GBTC_BACKEND=cuda GBTC_CUDA_BLOCK=512 GBTC_BATCH_NONCES=67108864`
  (shell-env override; `.env` keeps 2^24 default until the operator adopts).
- Rollback: `docker tag gamble-btc:rollback-vpn-20261001 gamble-btc:latest`
  + `GBTC_BACKEND=opencl ... docker compose up -d miner` (OpenCL path
  untouched and conformance-green). Previous sidecar images superseded
  (mock-tested replacements); prep image `pre-socks-20261001` retained.
- Status UI: `http://localhost:41174/` (loopback) and via the tailnet
  sidecar identity; no public/LAN exposure.
- Accepted-share evidence: mock pools only (15/15, 16/16). One live share
  submitted at pool diff 10000; pool response did not parse (E16).
  No block found (expected at this hashrate).

## Cleanup inventory

- Campaign artifacts under ignored `bench-results/campaign-20261001/`
  (~1.2 MB) + prep `bench-results/hypothesis-20261001/` (~1.5 MB): retained
  as experiment records. No obsolete images beyond the retention set
  (baseline, winner, prep image); no prunes of shared caches/volumes.
- Disk: ~50 GB free throughout; no space pressure created.

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
- E11 (PASS): mock pool at diff 1.0 — 15 submits, 15 verified, 0 bad, both
  jobs; clean-job switch proven. Mock target now matches repo vector.
- E14 (PASS, deployed in `10282bf0`): dead-socket reconnect with backoff;
  kill/restart test gives 16/16 verified, 0 bad, no spin.
- E15: cuda-512 KEEP (+3.9% repeatable, 657-658 vs 632; conformance passes).
  pre3 REJECT on both APIs (629.7, no gain).
- E09g (running): production cuda/512 + batch 2^26, ~662 MH/s, miner CPU
  0.281% (stretch target met), sidecar 1.38% separately, 74-75 C.
