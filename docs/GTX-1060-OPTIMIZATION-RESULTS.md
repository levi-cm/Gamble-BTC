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

- M1 (pending): NVIDIA harness + thermal guard + baseline correctness gates.
