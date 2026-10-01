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
- Current phase: 0–4 h. Next action: commit M1 (guard + harness + E01/E02);
  then Path 1 CUDA prototype promotion analysis (compile-only SASS/PTX
  inspection in toolbox, no GPU load), then 30–60 min ls256 confirmation.

## Experiment ledger

| ID | Hypothesis | Result |
| --- | --- | --- |
| E01 | Baseline gates: probe, conformance (5 variants), 60 s unrolled/64 control | PASS. Probe OK (NVIDIA CUDA / GTX 1060 / OpenCL 3.0). All conformance passed. Baseline 609.77 MH/s, 54→70 C. Queue mode `default-no-throttle-ext` (no cl_khr_throttle_hints on NVIDIA). Device max_wg=256 → local 512 illegal here. |
| E02 | Variant/dispatch screen, 30 s runs, serialized | unrolled/64: 611.39/609.71. pre3: 612.36 (INCONCLUSIVE, in noise). looped: 113.84 REJECT. dual: 571.63 REJECT. ls32: 594.16 REJECT. ls128: 609.21 REJECT (no gain). ls256: 629.31/630.08 KEEP candidate (+3.2%, needs 30–60 min confirmation). cpu_steady ~1.15–1.35% in bench loop (includes no stratum/UI). |
