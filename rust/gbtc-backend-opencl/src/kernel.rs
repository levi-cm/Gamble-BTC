// SHA round generation uses explicit schedule indices for auditability.
#![allow(clippy::needless_range_loop)]
use crate::BackendError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelVariant {
    Unrolled,
    Looped,
    Dual,
    Pre3,
}

impl KernelVariant {
    pub fn parse(s: &str) -> Result<Self, BackendError> {
        match s {
            "unrolled" => Ok(Self::Unrolled),
            "looped" => Ok(Self::Looped),
            "dual" => Ok(Self::Dual),
            "pre3" => Ok(Self::Pre3),
            _ => Err(BackendError::invalid_input(
                "GBTC_OPENCL_KERNEL must be unrolled, looped, dual, or pre3",
            )),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Unrolled => "ocl-unrolled",
            Self::Looped => "ocl-looped",
            Self::Dual => "ocl-dual",
            Self::Pre3 => "ocl-pre3",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub local_size: u32,
    pub local_size_auto: bool,
    pub poll_us: u32,
    pub simd: u32,
    pub kernel: KernelVariant,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            local_size: 64,
            local_size_auto: false,
            poll_us: 1000,
            simd: 0,
            kernel: KernelVariant::Unrolled,
        }
    }
}

pub fn local_size_allowed(v: u32) -> bool {
    matches!(v, 8 | 16 | 32 | 64 | 128 | 256 | 512)
}

pub fn simd_allowed(v: u32) -> bool {
    v == 0 || v == 8 || v == 16 || v == 32
}

pub fn batch_quantum(cfg: &Config) -> u32 {
    let q = cfg.local_size;
    if cfg.kernel == KernelVariant::Dual {
        q * 2
    } else {
        q
    }
}

const SHA_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn pre_rotr(x: u32, n: u32) -> u32 {
    x.rotate_right(n)
}

pub fn sha_prefix3(out: &mut [u32; 8], mid: &[u32; 8], tail3: &[u32; 3]) {
    let mut a = mid[0];
    let mut b = mid[1];
    let mut c = mid[2];
    let mut d = mid[3];
    let mut e = mid[4];
    let mut f = mid[5];
    let mut g = mid[6];
    let mut h = mid[7];
    for t in 0..3usize {
        let s1 = pre_rotr(e, 6) ^ pre_rotr(e, 11) ^ pre_rotr(e, 25);
        let ch = (e & f) ^ (!e & g);
        let t1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(SHA_K[t])
            .wrapping_add(tail3[t]);
        let s0 = pre_rotr(a, 2) ^ pre_rotr(a, 13) ^ pre_rotr(a, 22);
        let mj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(mj);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    out[0] = a;
    out[1] = b;
    out[2] = c;
    out[3] = d;
    out[4] = e;
    out[5] = f;
    out[6] = g;
    out[7] = h;
}

// We'll port generator using string building. To keep manageable, implement
// emit functions returning String fragments.
pub fn build_kernel_src(cfg: &Config) -> Result<String, BackendError> {
    // Build into a String with capacity.
    let mut sb = String::with_capacity(256 * 1024);
    let push = |sb: &mut String, s: &str| sb.push_str(s);

    // macros
    sb.push_str("#define ROR(x,n) (((x)>>(n))|((x)<<(32u-(n))))\n");
    sb.push_str("#define BIGSIG0(x) (ROR(x,2u)^ROR(x,13u)^ROR(x,22u))\n");
    sb.push_str("#define BIGSIG1(x) (ROR(x,6u)^ROR(x,11u)^ROR(x,25u))\n");
    sb.push_str("#define SMALLSIG0(x) (ROR(x,7u)^ROR(x,18u)^((x)>>3u))\n");
    sb.push_str("#define SMALLSIG1(x) (ROR(x,17u)^ROR(x,19u)^((x)>>10u))\n");
    sb.push_str("#define CH(x,y,z) (((x)&(y))^(~(x)&(z)))\n");
    sb.push_str("#define MAJ(x,y,z) (((x)&(y))^((x)&(z))^((y)&(z)))\n");
    sb.push_str("#define BSWAP32(x) (((x)>>24u)|(((x)>>8u)&0x0000ff00u)|(((x)<<8u)&0x00ff0000u)|((x)<<24u))\n");

    if cfg.simd != 0 {
        sb.push_str(&format!(
            "#define GBTC_SIMD_ATTR __attribute__((intel_reqd_sub_group_size({})))\n",
            cfg.simd
        ));
    } else {
        sb.push_str("#define GBTC_SIMD_ATTR\n");
    }
    if !cfg.local_size_auto {
        sb.push_str(&format!(
            "#define GBTC_WG_ATTR __attribute__((reqd_work_group_size({}, 1, 1)))\n",
            cfg.local_size
        ));
    } else {
        sb.push_str("#define GBTC_WG_ATTR\n");
    }

    sb.push_str("GBTC_SIMD_ATTR\nGBTC_WG_ATTR\n__kernel void gbtc_mine(__global const uint *I, __global uint *O) {\n");
    sb.push_str("  uint gid = get_global_id(0);\n");
    sb.push_str("  uint t1, t2;\n  (void)t1; (void)t2;\n");

    // guard for count: I[13] for normal layouts, I[21] for pre3. We'll insert variant-specific.
    match cfg.kernel {
        KernelVariant::Dual => {
            // guard total count: if gid*2 >= count return
            sb.push_str("  if (gid * 2u >= I[13]) return;\n");
            emit_nonce_unrolled(&mut sb, "I[12] + (gid << 1u)", "", true, 1);
            sb.push_str("  if (gid * 2u + 1u >= I[13]) return;\n");
            emit_nonce_unrolled(&mut sb, "I[12] + (gid << 1u) + 1u", "_lane1", true, 1);
        }
        KernelVariant::Looped => {
            sb.push_str("  if (gid >= I[13]) return;\n");
            emit_nonce_looped(&mut sb, "I[12] + gid");
        }
        KernelVariant::Pre3 => {
            sb.push_str("  if (gid >= I[21]) return;\n");
            emit_nonce_pre3(&mut sb, "I[20] + gid");
        }
        KernelVariant::Unrolled => {
            sb.push_str("  if (gid >= I[13]) return;\n");
            emit_nonce_unrolled(&mut sb, "I[12] + gid", "", false, 0);
        }
    }

    sb.push_str("}\n");

    if sb.len() > 256 * 1024 {
        return Err(BackendError::driver_failure(
            "generated OpenCL source overflowed".to_string(),
        ));
    }
    let _ = push;
    Ok(sb)
}

// emit functions
fn emit_unrolled_rounds(
    sb: &mut String,
    vnames: &[&str; 8],
    wnames: &[&str; 16],
    t_first: usize,
    t_count: usize,
) {
    // We'll use rotating v array indices into vnames? Simpler: use Vec<String> of current names.
    let mut v: Vec<String> = vnames.iter().map(|s| s.to_string()).collect();
    for t in t_first..(t_first + t_count) {
        let wt_idx = t & 15;
        if t >= 16 {
            let s = t & 15;
            let s2 = (t - 2) & 15;
            let s7 = (t - 7) & 15;
            let s15 = (t - 15) & 15;
            sb.push_str(&format!(
                "  {} = {} + SMALLSIG0({}) + {} + SMALLSIG1({});\n",
                wnames[s], wnames[s], wnames[s15], wnames[s7], wnames[s2]
            ));
        }
        let wt = wnames[wt_idx];
        sb.push_str(&format!(
            "  t1 = {} + BIGSIG1({}) + CH({},{},{}) + 0x{:08x}u + {};\n",
            v[7], v[4], v[4], v[5], v[6], SHA_K[t], wt
        ));
        sb.push_str(&format!(
            "  t2 = BIGSIG0({}) + MAJ({},{},{});\n",
            v[0], v[0], v[1], v[2]
        ));
        sb.push_str(&format!(
            "  {} = {} + t1; {} = t1 + t2;\n",
            v[3], v[3], v[7]
        ));
        // rotate
        let na = v[7].clone();
        let ne = v[3].clone();
        v[7] = v[6].clone();
        v[6] = v[5].clone();
        v[5] = v[4].clone();
        v[4] = ne;
        v[3] = v[2].clone();
        v[2] = v[1].clone();
        v[1] = v[0].clone();
        v[0] = na;
    }
    // We don't need slot names for pre3? pre3 uses slots after rounds. We'll handle separately.
    let _ = ();
}

fn emit_nonce_unrolled(sb: &mut String, nonce_expr: &str, suffix: &str, _guard: bool, _x: usize) {
    // declare variables
    let mut vn = Vec::with_capacity(8);
    let mut wn = Vec::with_capacity(16);
    for i in 0..8 {
        vn.push(format!("{}{}", "abcdefgh".chars().nth(i).unwrap(), suffix));
    }
    for i in 0..16 {
        wn.push(format!("w{}{}", i, suffix));
    }
    // w0..w3
    sb.push_str(&format!(
        "  uint {} = I[8], {} = I[9], {} = I[10], {} = {};\n",
        wn[0], wn[1], wn[2], wn[3], nonce_expr
    ));
    sb.push_str(&format!("  uint {} = 0x80000000u", wn[4]));
    for i in 5..15 {
        sb.push_str(&format!(", {} = 0u", wn[i]));
    }
    sb.push_str(&format!(", {} = 640u;\n", wn[15]));
    // a..d, e..h from I[0..7]
    sb.push_str(&format!(
        "  uint {} = I[0], {} = I[1], {} = I[2], {} = I[3];\n",
        vn[0], vn[1], vn[2], vn[3]
    ));
    sb.push_str(&format!(
        "  uint {} = I[4], {} = I[5], {} = I[6], {} = I[7];\n",
        vn[4], vn[5], vn[6], vn[7]
    ));

    // vnames/wnames slices
    let vrefs: [String; 8] = vn.clone().try_into().unwrap();
    let wrefs: [String; 16] = wn.clone().try_into().unwrap();
    let vstr: Vec<&str> = vrefs.iter().map(|s| s.as_str()).collect();
    let wstr: Vec<&str> = wrefs.iter().map(|s| s.as_str()).collect();
    let vfixed: [&str; 8] = [
        vstr[0], vstr[1], vstr[2], vstr[3], vstr[4], vstr[5], vstr[6], vstr[7],
    ];
    let wfixed: [&str; 16] = [
        wstr[0], wstr[1], wstr[2], wstr[3], wstr[4], wstr[5], wstr[6], wstr[7], wstr[8], wstr[9],
        wstr[10], wstr[11], wstr[12], wstr[13], wstr[14], wstr[15],
    ];

    emit_unrolled_rounds(sb, &vfixed, &wfixed, 0, 64);

    // fold midstate
    for i in 0..8 {
        sb.push_str(&format!("  {} = I[{}] + {};\n", wfixed[i], i, vfixed[i]));
    }
    sb.push_str(&format!("  {} = 0x80000000u", wfixed[8]));
    for i in 9..15 {
        sb.push_str(&format!(", {} = 0u", wfixed[i]));
    }
    sb.push_str(&format!(", {} = 256u;\n", wfixed[15]));
    sb.push_str(&format!(
        "  {} = 0x6a09e667u; {} = 0xbb67ae85u; {} = 0x3c6ef372u; {} = 0xa54ff53au;\n",
        vfixed[0], vfixed[1], vfixed[2], vfixed[3]
    ));
    sb.push_str(&format!(
        "  {} = 0x510e527fu; {} = 0x9b05688cu; {} = 0x1f83d9abu; {} = 0x5be0cd19u;\n",
        vfixed[4], vfixed[5], vfixed[6], vfixed[7]
    ));

    emit_unrolled_rounds(sb, &vfixed, &wfixed, 0, 64);

    // match check
    let suffix_var = if suffix.is_empty() { "" } else { suffix };
    sb.push_str(&format!(
        "  if (BSWAP32(0x5be0cd19u + {}) <= I[11]) {{\n    uint idx{} = atomic_inc(O);\n    if (idx{} < 15u) O[idx{} + 1u] = {};\n  }}\n",
        vfixed[7], suffix_var, suffix_var, suffix_var, nonce_expr
    ));
}

fn emit_nonce_looped(sb: &mut String, nonce_expr: &str) {
    sb.push_str("  uint w[16];\n  uint a, b, c, d, e, f, g, h;\n  const uint K[64] = {");
    for i in 0..64 {
        sb.push_str(&format!(
            "{}0x{:08x}u",
            if i == 0 { "" } else { ", " },
            SHA_K[i]
        ));
    }
    sb.push_str("};\n");
    sb.push_str(&format!(
        "  w[0] = I[8]; w[1] = I[9]; w[2] = I[10]; w[3] = {};\n",
        nonce_expr
    ));
    sb.push_str("  w[4] = 0x80000000u; w[5] = 0u; w[6] = 0u; w[7] = 0u;\n");
    sb.push_str("  w[8] = 0u; w[9] = 0u; w[10] = 0u; w[11] = 0u;\n");
    sb.push_str("  w[12] = 0u; w[13] = 0u; w[14] = 0u; w[15] = 640u;\n");
    sb.push_str("  a = I[0]; b = I[1]; c = I[2]; d = I[3];\n");
    sb.push_str("  e = I[4]; f = I[5]; g = I[6]; h = I[7];\n");

    for block in 0..2 {
        if block == 1 {
            sb.push_str("  w[0] = I[0] + a; w[1] = I[1] + b; w[2] = I[2] + c; w[3] = I[3] + d;\n");
            sb.push_str("  w[4] = I[4] + e; w[5] = I[5] + f; w[6] = I[6] + g; w[7] = I[7] + h;\n");
            sb.push_str("  w[8] = 0x80000000u; w[9] = 0u; w[10] = 0u; w[11] = 0u;\n");
            sb.push_str("  w[12] = 0u; w[13] = 0u; w[14] = 0u; w[15] = 256u;\n");
            sb.push_str("  a = 0x6a09e667u; b = 0xbb67ae85u; c = 0x3c6ef372u; d = 0xa54ff53au;\n");
            sb.push_str("  e = 0x510e527fu; f = 0x9b05688cu; g = 0x1f83d9abu; h = 0x5be0cd19u;\n");
        }
        sb.push_str("  for (int i = 0; i < 64; i++) {\n");
        sb.push_str("    uint wt = w[i & 15];\n");
        sb.push_str("    if (i >= 16) {\n");
        sb.push_str("      w[i & 15] = wt + SMALLSIG0(w[(i - 15) & 15]) + w[(i - 7) & 15]\n");
        sb.push_str("                + SMALLSIG1(w[(i - 2) & 15]);\n");
        sb.push_str("      wt = w[i & 15];\n    }\n");
        sb.push_str("    uint lt1 = h + BIGSIG1(e) + CH(e, f, g) + K[i] + wt;\n");
        sb.push_str("    uint lt2 = BIGSIG0(a) + MAJ(a, b, c);\n");
        sb.push_str("    h = g; g = f; f = e; e = d + lt1;\n");
        sb.push_str("    d = c; c = b; b = a; a = lt1 + lt2;\n  }\n");
    }
    sb.push_str("  if (BSWAP32(0x5be0cd19u + h) <= I[11]) {\n    uint idx = atomic_inc(O);\n    if (idx < 15u) O[idx + 1u] = ");
    sb.push_str(nonce_expr);
    sb.push_str(";\n  }\n");
}

fn emit_nonce_pre3(sb: &mut String, nonce_expr: &str) {
    sb.push_str("  uint w0 = I[16], w1 = I[17], w2 = I[18], w3 = ");
    sb.push_str(nonce_expr);
    sb.push_str(";\n");
    sb.push_str("  uint w4 = 0x80000000u");
    for i in 5..15 {
        sb.push_str(&format!(", w{} = 0u", i));
    }
    sb.push_str(", w15 = 640u;\n");
    sb.push_str("  uint a = I[0], b = I[1], c = I[2], d = I[3];\n");
    sb.push_str("  uint e = I[4], f = I[5], g = I[6], h = I[7];\n");

    // emit rounds 3..64 (61 rounds) with rotating names a..h
    // We'll use variable names a..h directly; emit_unrolled_rounds uses vnames/w0..w15.
    // Need slot tracking. We'll inline a custom loop generating code with rotation,
    // tracking final slot mapping. Since emit_unrolled_rounds in C returns slot names.
    // Our emit_unrolled_rounds doesn't track. For pre3, need slot mapping after 61 rounds.
    // Let's implement rounds manually with rotation and record slot mapping.
    let vnames = ["a", "b", "c", "d", "e", "f", "g", "h"];
    let wnames = [
        "w0", "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "w10", "w11", "w12", "w13",
        "w14", "w15",
    ];
    // We need current mapping from logical slot to variable name. Start identity.
    let mut slot: Vec<String> = (0..8).map(|i| vnames[i].to_string()).collect();
    // w array already declared as scalars w0..w15; message expansion uses wnames.
    for t in 3..64usize {
        let s = t & 15;
        if t >= 16 {
            sb.push_str(&format!(
                "  {} = {} + SMALLSIG0({}) + {} + SMALLSIG1({});\n",
                wnames[s],
                wnames[s],
                wnames[(t - 15) & 15],
                wnames[(t - 7) & 15],
                wnames[(t - 2) & 15]
            ));
        }
        // round
        sb.push_str(&format!(
            "  t1 = {} + BIGSIG1({}) + CH({},{},{}) + 0x{:08x}u + {};\n",
            slot[7], slot[4], slot[4], slot[5], slot[6], SHA_K[t], wnames[s]
        ));
        sb.push_str(&format!(
            "  t2 = BIGSIG0({}) + MAJ({},{},{});\n",
            slot[0], slot[0], slot[1], slot[2]
        ));
        sb.push_str(&format!(
            "  {} = {} + t1; {} = t1 + t2;\n",
            slot[3], slot[3], slot[7]
        ));
        // rotate slot mapping
        let na = slot[7].clone();
        let ne = slot[3].clone();
        slot[7] = slot[6].clone();
        slot[6] = slot[5].clone();
        slot[5] = slot[4].clone();
        slot[4] = ne;
        slot[3] = slot[2].clone();
        slot[2] = slot[1].clone();
        slot[1] = slot[0].clone();
        slot[0] = na;
    }
    // After 61 rounds, slot[i] holds variable name for state word i.
    // Feed-forward uses original midstate I[8..15].
    for i in 0..8 {
        sb.push_str(&format!("  w{} = I[{}] + {};\n", i, 8 + i, slot[i]));
    }
    sb.push_str("  w8 = 0x80000000u");
    for i in 9..15 {
        sb.push_str(&format!(", w{} = 0u", i));
    }
    sb.push_str(", w15 = 256u;\n");
    sb.push_str("  a = 0x6a09e667u; b = 0xbb67ae85u; c = 0x3c6ef372u; d = 0xa54ff53au;\n");
    sb.push_str("  e = 0x510e527fu; f = 0x9b05688cu; g = 0x1f83d9abu; h = 0x5be0cd19u;\n");

    // second block unrolled
    // use vnames a..h directly
    let vfixed: [&str; 8] = ["a", "b", "c", "d", "e", "f", "g", "h"];
    let wfixed: [&str; 16] = [
        "w0", "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "w10", "w11", "w12", "w13",
        "w14", "w15",
    ];
    emit_unrolled_rounds(sb, &vfixed, &wfixed, 0, 64);

    sb.push_str("  if (BSWAP32(0x5be0cd19u + h) <= I[19]) {\n    uint idx = atomic_inc(O);\n    if (idx < 15u) O[idx + 1u] = ");
    sb.push_str(nonce_expr);
    sb.push_str(";\n  }\n");
}
