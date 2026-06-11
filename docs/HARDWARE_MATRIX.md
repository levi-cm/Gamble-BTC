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
