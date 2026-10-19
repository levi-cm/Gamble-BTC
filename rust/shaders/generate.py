#!/usr/bin/env python3
import argparse
from pathlib import Path

K = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]

def header(pre3=False):
    lines = []
    lines.append("#version 450")
    lines.append("layout(constant_id = 0) const uint LOCAL_SIZE = 64;")
    lines.append("layout(local_size_x_id = 0, local_size_y = 1, local_size_z = 1) in;")
    lines.append("")
    lines.append("const uint CAPACITY = 1024u;")
    lines.append("")
    lines.append("layout(push_constant) uniform PushConstants {")
    lines.append("    uint midstate[8];")
    lines.append("    uint tail3[3];")
    lines.append("    uint nonce_base;")
    lines.append("    uint target_word0;")
    lines.append("    uint nonce_count;")
    if pre3:
        lines.append("    uint prefix[8];")
    lines.append("} pc;")
    lines.append("")
    lines.append("layout(set = 0, binding = 0) buffer OutputBuffer {")
    lines.append("    uint count;")
    lines.append("    uint candidates[CAPACITY];")
    lines.append("} outbuf;")
    lines.append("")
    lines.append("const uint K[64] = uint[64](")
    for i in range(0, 64, 4):
        vals = ", ".join(f"0x{K[i+j]:08x}u" for j in range(4))
        suffix = "," if i + 4 < 64 else ""
        lines.append(f"    {vals}{suffix}")
    lines.append(");")
    lines.append("")
    return lines

def funcs(boolean=False):
    lines = []
    lines.append("uint rotr(uint x, uint n) {")
    lines.append("    return (x >> n) | (x << (32u - n));")
    lines.append("}")
    lines.append("")
    lines.append("uint bigsig0(uint x) { return rotr(x, 2u) ^ rotr(x, 13u) ^ rotr(x, 22u); }")
    lines.append("uint bigsig1(uint x) { return rotr(x, 6u) ^ rotr(x, 11u) ^ rotr(x, 25u); }")
    lines.append("uint smallsig0(uint x) { return rotr(x, 7u) ^ rotr(x, 18u) ^ (x >> 3u); }")
    lines.append("uint smallsig1(uint x) { return rotr(x, 17u) ^ rotr(x, 19u) ^ (x >> 10u); }")
    lines.append("")
    if boolean:
        lines.append("uint ch(uint x, uint y, uint z) { return z ^ (x & (y ^ z)); }")
        lines.append("uint maj(uint x, uint y, uint z) { return (x & y) | (z & (x | y)); }")
    else:
        lines.append("uint ch(uint x, uint y, uint z) { return (x & y) ^ (~x & z); }")
        lines.append("uint maj(uint x, uint y, uint z) { return (x & y) ^ (x & z) ^ (y & z); }")
    lines.append("")
    return lines

def gen_rounds(lines, t_start, t_count, slots, w, boolean=False):
    # slots: list of 8 names (mutated)
    for t in range(t_start, t_start + t_count):
        s = t & 15
        if t >= 16:
            s15 = (t - 15) & 15
            s7 = (t - 7) & 15
            s2 = (t - 2) & 15
            lines.append(f"    {w[s]} = {w[s]} + smallsig0({w[s15]}) + {w[s7]} + smallsig1({w[s2]});")
        lines.append(f"    t1 = {slots[7]} + bigsig1({slots[4]}) + ch({slots[4]}, {slots[5]}, {slots[6]}) + 0x{K[t]:08x}u + {w[s]};")
        lines.append(f"    t2 = bigsig0({slots[0]}) + maj({slots[0]}, {slots[1]}, {slots[2]});")
        lines.append(f"    {slots[3]} = {slots[3]} + t1; {slots[7]} = t1 + t2;")
        # rotate
        new_slots = [slots[7], slots[0], slots[1], slots[2], slots[3], slots[4], slots[5], slots[6]]
        slots[:] = new_slots

def unrolled_shader(boolean=False):
    lines = header(False)
    lines += funcs(boolean)
    lines.append("void main() {")
    lines.append("    uint idx = gl_GlobalInvocationID.x;")
    lines.append("    if (idx >= pc.nonce_count) return;")
    lines.append("    uint nonce = pc.nonce_base + idx;")
    lines.append("")
    lines.append("    uint a = pc.midstate[0], b = pc.midstate[1], c = pc.midstate[2], d = pc.midstate[3];")
    lines.append("    uint e = pc.midstate[4], f = pc.midstate[5], g = pc.midstate[6], h = pc.midstate[7];")
    lines.append("")
    lines.append("    uint w0 = pc.tail3[0], w1 = pc.tail3[1], w2 = pc.tail3[2], w3 = nonce;")
    lines.append("    uint w4 = 0x80000000u, w5 = 0u, w6 = 0u, w7 = 0u;")
    lines.append("    uint w8 = 0u, w9 = 0u, w10 = 0u, w11 = 0u;")
    lines.append("    uint w12 = 0u, w13 = 0u, w14 = 0u, w15 = 640u;")
    lines.append("")
    lines.append("    uint t1, t2;")
    # We'll manage slots as Python list of names, not GLSL.
    slots = ["a", "b", "c", "d", "e", "f", "g", "h"]
    w = [f"w{i}" for i in range(16)]
    gen_rounds(lines, 0, 64, slots, w, boolean)
    lines.append("")
    # fold first hash
    # slots identity after 64
    lines.append("    w0 = pc.midstate[0] + a; w1 = pc.midstate[1] + b; w2 = pc.midstate[2] + c; w3 = pc.midstate[3] + d;")
    lines.append("    w4 = pc.midstate[4] + e; w5 = pc.midstate[5] + f; w6 = pc.midstate[6] + g; w7 = pc.midstate[7] + h;")
    lines.append("    w8 = 0x80000000u; w9 = 0u; w10 = 0u; w11 = 0u;")
    lines.append("    w12 = 0u; w13 = 0u; w14 = 0u; w15 = 256u;")
    lines.append("    a = 0x6a09e667u; b = 0xbb67ae85u; c = 0x3c6ef372u; d = 0xa54ff53au;")
    lines.append("    e = 0x510e527fu; f = 0x9b05688cu; g = 0x1f83d9abu; h = 0x5be0cd19u;")
    lines.append("")
    slots = ["a", "b", "c", "d", "e", "f", "g", "h"]
    gen_rounds(lines, 0, 64, slots, w, boolean)
    lines.append("")
    lines.append("    uint final_word7 = 0x5be0cd19u + h;")
    lines.append("    uint bswapped = (final_word7 >> 24) | ((final_word7 >> 8) & 0x0000ff00u) |")
    lines.append("                    ((final_word7 << 8) & 0x00ff0000u) | (final_word7 << 24);")
    lines.append("")
    lines.append("    if (bswapped <= pc.target_word0) {")
    lines.append("        uint slot = atomicAdd(outbuf.count, 1u);")
    lines.append("        if (slot < CAPACITY) {")
    lines.append("            outbuf.candidates[slot] = nonce;")
    lines.append("        }")
    lines.append("    }")
    lines.append("}")
    lines.append("")
    return "\n".join(lines)

def pruned_shader(boolean=False):
    # first compression full, second compression 61 rounds
    lines = header(False)
    lines += funcs(boolean)
    lines.append("void main() {")
    lines.append("    uint idx = gl_GlobalInvocationID.x;")
    lines.append("    if (idx >= pc.nonce_count) return;")
    lines.append("    uint nonce = pc.nonce_base + idx;")
    lines.append("")
    lines.append("    uint a = pc.midstate[0], b = pc.midstate[1], c = pc.midstate[2], d = pc.midstate[3];")
    lines.append("    uint e = pc.midstate[4], f = pc.midstate[5], g = pc.midstate[6], h = pc.midstate[7];")
    lines.append("")
    lines.append("    uint w0 = pc.tail3[0], w1 = pc.tail3[1], w2 = pc.tail3[2], w3 = nonce;")
    lines.append("    uint w4 = 0x80000000u, w5 = 0u, w6 = 0u, w7 = 0u;")
    lines.append("    uint w8 = 0u, w9 = 0u, w10 = 0u, w11 = 0u;")
    lines.append("    uint w12 = 0u, w13 = 0u, w14 = 0u, w15 = 640u;")
    lines.append("")
    lines.append("    uint t1, t2;")
    slots = ["a", "b", "c", "d", "e", "f", "g", "h"]
    w = [f"w{i}" for i in range(16)]
    gen_rounds(lines, 0, 64, slots, w, boolean)
    lines.append("")
    lines.append("    w0 = pc.midstate[0] + a; w1 = pc.midstate[1] + b; w2 = pc.midstate[2] + c; w3 = pc.midstate[3] + d;")
    lines.append("    w4 = pc.midstate[4] + e; w5 = pc.midstate[5] + f; w6 = pc.midstate[6] + g; w7 = pc.midstate[7] + h;")
    lines.append("    w8 = 0x80000000u; w9 = 0u; w10 = 0u; w11 = 0u;")
    lines.append("    w12 = 0u; w13 = 0u; w14 = 0u; w15 = 256u;")
    lines.append("    a = 0x6a09e667u; b = 0xbb67ae85u; c = 0x3c6ef372u; d = 0xa54ff53au;")
    lines.append("    e = 0x510e527fu; f = 0x9b05688cu; g = 0x1f83d9abu; h = 0x5be0cd19u;")
    lines.append("")
    slots = ["a", "b", "c", "d", "e", "f", "g", "h"]
    gen_rounds(lines, 0, 61, slots, w, boolean)
    lines.append("")
    lines.append("    uint final_word7 = 0x5be0cd19u + h;")
    lines.append("    uint bswapped = (final_word7 >> 24) | ((final_word7 >> 8) & 0x0000ff00u) |")
    lines.append("                    ((final_word7 << 8) & 0x00ff0000u) | (final_word7 << 24);")
    lines.append("")
    lines.append("    if (bswapped <= pc.target_word0) {")
    lines.append("        uint slot = atomicAdd(outbuf.count, 1u);")
    lines.append("        if (slot < CAPACITY) {")
    lines.append("            outbuf.candidates[slot] = nonce;")
    lines.append("        }")
    lines.append("    }")
    lines.append("}")
    lines.append("")
    return "\n".join(lines)

def pre3_shader():
    lines = header(True)
    lines += funcs(False)
    lines.append("void main() {")
    lines.append("    uint idx = gl_GlobalInvocationID.x;")
    lines.append("    if (idx >= pc.nonce_count) return;")
    lines.append("    uint nonce = pc.nonce_base + idx;")
    lines.append("")
    lines.append("    uint a = pc.prefix[0], b = pc.prefix[1], c = pc.prefix[2], d = pc.prefix[3];")
    lines.append("    uint e = pc.prefix[4], f = pc.prefix[5], g = pc.prefix[6], h = pc.prefix[7];")
    lines.append("")
    lines.append("    uint w0 = pc.tail3[0], w1 = pc.tail3[1], w2 = pc.tail3[2], w3 = nonce;")
    lines.append("    uint w4 = 0x80000000u, w5 = 0u, w6 = 0u, w7 = 0u;")
    lines.append("    uint w8 = 0u, w9 = 0u, w10 = 0u, w11 = 0u;")
    lines.append("    uint w12 = 0u, w13 = 0u, w14 = 0u, w15 = 640u;")
    lines.append("")
    lines.append("    uint t1, t2;")
    slots = ["a", "b", "c", "d", "e", "f", "g", "h"]
    w = [f"w{i}" for i in range(16)]
    gen_rounds(lines, 3, 61, slots, w, False)
    lines.append("")
    # feed-forward using slots mapping
    for i in range(8):
        lines.append(f"    w{i} = pc.midstate[{i}] + {slots[i]};")
    lines.append("    w8 = 0x80000000u; w9 = 0u; w10 = 0u; w11 = 0u;")
    lines.append("    w12 = 0u; w13 = 0u; w14 = 0u; w15 = 256u;")
    lines.append("    a = 0x6a09e667u; b = 0xbb67ae85u; c = 0x3c6ef372u; d = 0xa54ff53au;")
    lines.append("    e = 0x510e527fu; f = 0x9b05688cu; g = 0x1f83d9abu; h = 0x5be0cd19u;")
    lines.append("")
    slots = ["a", "b", "c", "d", "e", "f", "g", "h"]
    gen_rounds(lines, 0, 64, slots, w, False)
    lines.append("")
    lines.append("    uint final_word7 = 0x5be0cd19u + h;")
    lines.append("    uint bswapped = (final_word7 >> 24) | ((final_word7 >> 8) & 0x0000ff00u) |")
    lines.append("                    ((final_word7 << 8) & 0x00ff0000u) | (final_word7 << 24);")
    lines.append("")
    lines.append("    if (bswapped <= pc.target_word0) {")
    lines.append("        uint slot = atomicAdd(outbuf.count, 1u);")
    lines.append("        if (slot < CAPACITY) {")
    lines.append("            outbuf.candidates[slot] = nonce;")
    lines.append("        }")
    lines.append("    }")
    lines.append("}")
    lines.append("")
    return "\n".join(lines)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out-dir", default=str(Path(__file__).resolve().parent))
    args = parser.parse_args()
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    variants = {
        "unrolled": unrolled_shader(False),
        "boolean": pruned_shader(True),
    }
    variants["pruned"] = pruned_shader(False)
    variants["pre3"] = pre3_shader()
    for name, src in variants.items():
        path = out_dir / f"sha256d-generated-{name}.comp"
        path.write_text(src)

if __name__ == "__main__":
    main()
