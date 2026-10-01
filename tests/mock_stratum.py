#!/usr/bin/env python3
"""Local mock Stratum pool for submit-path verification (campaign E11).

Speaks just enough Stratum: subscribe/authorize/difficulty/notify, verifies
every mining.submit by recomputing the block header and checking the full
256-bit target, and answers true/false. Also issues a second clean job
mid-run so the miner's job-switch path is exercised.

Usage: mock_stratum.py <port> <seconds>   (logs JSONL verdicts to stdout)
"""
import hashlib
import json
import socket
import sys
import threading
import time

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 43333
SECONDS = int(sys.argv[2]) if len(sys.argv) > 2 else 180

DIFF = 1.0
# diff-1 target as 8 big-endian words, target[0] is most significant
TARGET = [0x00000000, 0x00000000, 0x00000000, 0x00000000,
          0x00000000, 0x00000000, 0x0000ffff, 0x00000000]
EN1 = "00112233"
JOB1 = {"id": "mockjob1", "prev": "00" * 32,
        "cb1": "01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff0704ffff001d0101",
        "cb2": "ffffffff0100f2052a010000001976a914000000000000000000000000000000000000000088ac00000000",
        "branches": [], "version": "20000000", "nbits": "1d00ffff",
        "ntime": format(int(time.time()), "08x"), "clean": True}
JOB2 = dict(JOB1)
JOB2["id"] = "mockjob2"
JOB2["clean"] = True

STATS = {"submits": 0, "verified": 0, "bad": 0, "jobs_seen": set()}


def sha256d(b):
    return hashlib.sha256(hashlib.sha256(b).digest()).digest()


def meets_target(h, target_words):
    # Same word order as gbtc_raw_hash_meets_target in src/stratum_protocol.c.
    for i in range(8):
        p = 31 - 4 * i
        w = (h[p] << 24) | (h[p - 1] << 16) | (h[p - 2] << 8) | h[p - 3]
        if w < target_words[i]:
            return True
        if w > target_words[i]:
            return False
    return True


def handle(conn):
    f = conn.makefile("r")
    en2sizes = {}
    try:
        # subscribe
        line = f.readline()
        req = json.loads(line)
        sid = req.get("id", 1)
        conn.sendall((json.dumps({"id": sid, "result": [[["mining.set_difficulty", "s1"],
                                                        ["mining.notify", "s1"]], EN1, 4],
                                  "error": None}) + "\n").encode())
        # authorize
        line = f.readline()
        req = json.loads(line)
        conn.sendall((json.dumps({"id": req.get("id", 2), "result": True,
                                  "error": None}) + "\n").encode())
        # difficulty + first job
        conn.sendall((json.dumps({"id": None, "method": "mining.set_difficulty",
                                  "params": [DIFF]}) + "\n").encode())
        j = JOB1
        conn.sendall((json.dumps({"id": None, "method": "mining.notify",
                                  "params": [j["id"], j["prev"], j["cb1"], j["cb2"],
                                             j["branches"], j["version"], j["nbits"],
                                             j["ntime"], j["clean"]]}) + "\n").encode())
        t_end = time.time() + SECONDS
        job2_sent = False
        conn.settimeout(1.0)
        buf = ""
        while time.time() < t_end:
            try:
                chunk = conn.recv(4096).decode()
            except socket.timeout:
                chunk = ""
            if not chunk:
                if not job2_sent and time.time() > t_end - SECONDS * 0.5:
                    j = JOB2
                    conn.sendall((json.dumps({"id": None, "method": "mining.notify",
                                              "params": [j["id"], j["prev"], j["cb1"], j["cb2"],
                                                         j["branches"], j["version"], j["nbits"],
                                                         j["ntime"], True]}) + "\n").encode())
                    job2_sent = True
                continue
            buf += chunk
            while "\n" in buf:
                line, buf = buf.split("\n", 1)
                if not line.strip():
                    continue
                msg = json.loads(line)
                if msg.get("method") == "mining.submit":
                    p = msg["params"]
                    user, job_id, en2hex, ntimehex, noncehex = p[0], p[1], p[2], p[3], p[4]
                    STATS["submits"] += 1
                    STATS["jobs_seen"].add(job_id)
                    ok = verify(job_id, en2hex, ntimehex, noncehex)
                    STATS["verified" if ok else "bad"] += 1
                    conn.sendall((json.dumps({"id": msg["id"], "result": ok,
                                              "error": None}) + "\n").encode())
    except (ConnectionError, ValueError):
        pass
    finally:
        try:
            conn.close()
        except OSError:
            pass


def verify(job_id, en2hex, ntimehex, noncehex):
    j = JOB2 if job_id == JOB2["id"] else JOB1
    try:
        en1 = bytes.fromhex(EN1)
        en2 = bytes.fromhex(en2hex)
        coinbase = bytes.fromhex(j["cb1"]) + en1 + en2 + bytes.fromhex(j["cb2"])
        root = sha256d(coinbase)
        hdr = bytes.fromhex(j["version"])[::-1]
        prev = bytes.fromhex(j["prev"])
        prev_swapped = b"".join(prev[i:i + 4][::-1] for i in range(0, 32, 4))
        hdr += prev_swapped + root
        hdr += bytes.fromhex(ntimehex)[::-1] + bytes.fromhex(j["nbits"])[::-1]
        # nonce bytes are big-endian in the header (put_be32), no reversal.
        hdr += bytes.fromhex(noncehex)
        assert len(hdr) == 80
        return meets_target(sha256d(hdr), TARGET)
    except (ValueError, AssertionError):
        return False


srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(("127.0.0.1", PORT))
srv.listen(4)
srv.settimeout(1.0)
t_end = time.time() + SECONDS + 15
print(json.dumps({"mock": "listening", "port": PORT, "seconds": SECONDS}), flush=True)
while time.time() < t_end:
    try:
        conn, _ = srv.accept()
    except socket.timeout:
        continue
    threading.Thread(target=handle, args=(conn,), daemon=True).start()
print(json.dumps({"mock": "done", "submits": STATS["submits"],
                  "verified": STATS["verified"], "bad": STATS["bad"],
                  "jobs": sorted(STATS["jobs_seen"])}), flush=True)
sys.exit(0 if STATS["verified"] >= 3 and STATS["bad"] == 0 else 1)
