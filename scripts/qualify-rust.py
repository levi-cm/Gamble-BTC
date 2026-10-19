#!/usr/bin/env python3
"""Exercise a Rust candidate with a synthetic Stratum pool and real HTTP/SSE.

No wallet, public pool, existing container or host configuration is used.
"""
import argparse
import hashlib
import json
import os
import pathlib
import socket
import subprocess
import threading
import time
import urllib.request
import uuid

DIFF1 = 0xFFFF << 208


def sha256d(data):
    return hashlib.sha256(hashlib.sha256(data).digest()).digest()


# Public Bitcoin genesis coinbase: the inserted extranonces occupy eight bytes
# of the zero previous-output hash. This gives the GPU a known valid share while
# the mock pool still independently constructs and hashes its submitted header.
GENESIS_CB = bytes.fromhex(
    '0100000001' + '00' * 32 +
    'ffffffff4d04ffff001d0104455468652054696d65732030332f4a616e2f3230303920'
    '4368616e63656c6c6f72206f6e206272696e6b206f66207365636f6e64206261696c'
    '6f757420666f722062616e6b73ffffffff0100f2052a01000000434104678afdb0fe55'
    '48271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c'
    '4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000')
assert sha256d(GENESIS_CB).hex() == '3ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a'


class MockPool:
    def __init__(self):
        self.socket = socket.socket()
        self.socket.bind(('127.0.0.1', 0))
        self.socket.listen(4)
        self.socket.settimeout(.1)
        self.port = self.socket.getsockname()[1]
        self.stop = threading.Event()
        self.thread = threading.Thread(target=self.run, daemon=True)
        self.counts = dict(connections=0, authorized=0, submitted=0, accepted=0, stale=0,
                           invalid=0, duplicates=0, clean_jobs=0, nonclean_jobs=0)
        self.error = None

    def start(self):
        self.thread.start()

    def close(self):
        self.stop.set()
        self.thread.join(2)
        self.socket.close()

    @staticmethod
    def send(conn, obj):
        conn.sendall(json.dumps(obj).encode() + b'\n')

    def run(self):
        try:
            while not self.stop.is_set():
                try:
                    conn, _ = self.socket.accept()
                except socket.timeout:
                    continue
                self.counts['connections'] += 1
                try:
                    with conn:
                        self.handle(conn)
                except (OSError, ValueError, KeyError, IndexError):
                    # Deliberate client shutdowns/reconnects are normal.
                    continue
        except Exception as error:
            self.error = repr(error)

    def handle(self, conn):
        conn.settimeout(.1)
        jobs = {}
        seen = set()
        en1 = bytes(4)
        next_job = time.monotonic()
        started = next_job
        job_no = 0
        buffer = b''
        authorized = False
        while not self.stop.is_set():
            now = time.monotonic()
            if now - started >= 120:
                return  # Force connection-generation invalidation during soak.
            if authorized and now >= next_job:
                clean = job_no % 3 == 0
                difficulty = 1 + job_no % 2
                ntime = 0x65000000 + job_no + self.counts['connections'] * 1000
                job_id = f'synthetic-{self.counts["connections"]}-{job_no}'
                if clean:
                    jobs.clear()
                    self.counts['clean_jobs'] += 1
                else:
                    self.counts['nonclean_jobs'] += 1
                version, prevhash, cb1, cb2 = 0x20000000, bytes(range(32)), bytes.fromhex('0102'), bytes.fromhex('0304')
                if job_no == 0:
                    ntime, difficulty, version = 1231006505, 1, 1
                    prevhash, cb1, cb2 = bytes(32), GENESIS_CB[:5], GENESIS_CB[13:]
                jobs[job_id] = (ntime, difficulty, version, prevhash, cb1, cb2)
                while len(jobs) > 64:
                    del jobs[next(iter(jobs))]
                self.send(conn, dict(id=None, method='mining.set_difficulty', params=[difficulty]))
                self.send(conn, dict(id=None, method='mining.notify', params=[
                    job_id, prevhash.hex(), cb1.hex(), cb2.hex(), [],
                    f'{version:08x}', '1d00ffff', f'{ntime:08x}', clean]))
                job_no += 1
                next_job = now + (30 if job_no == 1 else 15)
            try:
                data = conn.recv(4096)
            except socket.timeout:
                continue
            if not data:
                return
            buffer += data
            if len(buffer) > 65536:
                raise ValueError('oversize client frame')
            while b'\n' in buffer:
                raw, buffer = buffer.split(b'\n', 1)
                msg = json.loads(raw)
                method = msg.get('method')
                if method == 'mining.subscribe':
                    self.send(conn, dict(id=msg['id'], result=[[], en1.hex(), 4], error=None))
                elif method == 'mining.authorize':
                    authorized = True
                    self.counts['authorized'] += 1
                    self.send(conn, dict(id=msg['id'], result=True, error=None))
                elif method == 'mining.submit':
                    self.counts['submitted'] += 1
                    params = msg['params']
                    valid = False
                    if len(params) == 5 and params[1] in jobs:
                        ntime, difficulty, version, wire_prevhash, cb1, cb2 = jobs[params[1]]
                        en2 = bytes.fromhex(params[2])
                        wire_time = int(params[3], 16)
                        nonce = int(params[4], 16)
                        prevhash = b''.join(wire_prevhash[i:i+4][::-1] for i in range(0, 32, 4))
                        merkle = sha256d(cb1 + en1 + en2 + cb2)
                        header = version.to_bytes(4, 'little') + prevhash + merkle
                        header += wire_time.to_bytes(4, 'little') + (0x1d00ffff).to_bytes(4, 'little') + nonce.to_bytes(4, 'little')
                        valid = (wire_time == ntime and len(en2) == 4 and len(params[3]) == 8
                                 and len(params[4]) == 8 and int.from_bytes(sha256d(header), 'little') <= DIFF1 // difficulty)
                    key = tuple(params)
                    if key in seen:
                        self.counts['duplicates'] += 1
                        valid = False
                    seen.add(key)
                    stale = len(params) == 5 and params[1] not in jobs
                    if valid:
                        self.counts['accepted'] += 1
                    elif stale:
                        # Clean notify may cross an already transmitted share in flight.
                        self.counts['stale'] += 1
                    else:
                        self.counts['invalid'] += 1
                    self.send(conn, dict(id=msg['id'], result=valid,
                                         error=None if valid else [23, 'synthetic-invalid', None]))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', default='rust/target/release/gbtc-miner')
    parser.add_argument('--image')
    parser.add_argument('--backend', choices=['vulkan', 'opencl'], default='vulkan')
    parser.add_argument('--kernel', default='pre3')
    parser.add_argument('--local', type=int, default=64)
    parser.add_argument('--poll', type=int, default=1000)
    parser.add_argument('--wait-mode', choices=['fence', 'poll'], default='fence')
    parser.add_argument('--wait-us', type=int, default=10000)
    parser.add_argument('--queue-ms', type=int, default=200)
    parser.add_argument('--batch', type=int, default=16777216)
    parser.add_argument('--seconds', type=int, default=20)
    parser.add_argument('--device', default='/dev/dri/renderD128')
    parser.add_argument('--validation', action='store_true')
    parser.add_argument('--require-shares', action='store_true')
    parser.add_argument('--output', default='bench-results/rust-qualification')
    args = parser.parse_args()
    if args.seconds < 2:
        parser.error('--seconds must be at least 2')
    output = pathlib.Path(args.output)
    output.mkdir(parents=True, exist_ok=True)
    pool = MockPool()
    pool.start()
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        http_port = sock.getsockname()[1]
    settings = dict(BTC_ADDRESS='synthetic-local-test', WORKER_NAME='x',
                    POOL_URL=f'stratum+tcp://127.0.0.1:{pool.port}',
                    GBTC_BACKEND=args.backend, GBTC_DEVICE=args.device,
                    GBTC_HTTP_BIND='127.0.0.1', GBTC_HTTP_PORT=str(http_port),
                    GBTC_BATCH_NONCES=str(args.batch), GBTC_VULKAN_KERNEL=args.kernel,
                    GBTC_VULKAN_LOCAL_SIZE=str(args.local), GBTC_VULKAN_POLL_US=str(args.poll),
                    GBTC_OPENCL_POLL_US=str(args.poll),
                    GBTC_VULKAN_WAIT_MODE=args.wait_mode, GBTC_VULKAN_WAIT_US=str(args.wait_us),
                    GBTC_GPU_QUEUE_MS=str(args.queue_ms),
                    GBTC_GLES_KERNEL='', GBTC_GLES_LOCAL_SIZE='', GBTC_GLES_AUTOTUNE='')
    if args.validation:
        settings['GBTC_VULKAN_VALIDATION'] = '1'
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith(('GBTC_', 'BTC_ADDRESS', 'POOL_URL', 'WORKER_NAME'))}
    environment.update(settings)
    container = 'gbtc-rust-qual-' + uuid.uuid4().hex[:12]
    if args.image:
        command = ['docker', 'run', '--rm', '--name', container, '--network', 'host',
                   '--device', args.device, '--group-add', str(os.stat(args.device).st_gid)]
        for key, value in settings.items():
            command += ['-e', f'{key}={value}']
        command += [args.image]
    else:
        command = [str(pathlib.Path(args.binary).resolve())]
    started = time.monotonic()
    samples = []
    slow_clients = []
    runtime_metrics = []
    cpu_stat = None
    miner_pid = None
    failure = None
    with (output / 'miner.log').open('wb') as log:
        proc = subprocess.Popen(command, env=environment, stdout=log, stderr=log)
        try:
            while time.monotonic() - started < args.seconds:
                if proc.poll() is not None:
                    raise RuntimeError(f'miner exited early: {proc.returncode}')
                try:
                    with urllib.request.urlopen(f'http://127.0.0.1:{http_port}/status.json', timeout=1) as response:
                        status = json.load(response)
                    for key in ['mh', 'lifetime_hashes', 'submitted', 'accepted', 'rejected', 'backend', 'history']:
                        if key not in status:
                            raise RuntimeError(f'missing status key {key}')
                    if status['backend'] != args.backend:
                        raise RuntimeError('unexpected backend substitution')
                    samples.append(status)
                    metric = {'elapsed': time.monotonic() - started,
                              'lifetime_hashes': status.get('lifetime_hashes', 0)}
                    try:
                        if miner_pid is None:
                            if args.image:
                                miner_pid = int(subprocess.check_output(
                                    ['docker', 'inspect', '--format', '{{.State.Pid}}', container], text=True))
                            else:
                                miner_pid = proc.pid
                            if args.image:
                                group = pathlib.Path('/proc', str(miner_pid), 'cgroup').read_text().strip().split('::', 1)[1]
                                cpu_stat = pathlib.Path('/sys/fs/cgroup' + group, 'cpu.stat')
                        if args.image:
                            usage = dict(line.split() for line in cpu_stat.read_text().splitlines())
                            metric['cpu_usec'] = int(usage['usage_usec'])
                        else:
                            fields = pathlib.Path('/proc', str(miner_pid), 'stat').read_text().rsplit(') ', 1)[1].split()
                            metric['cpu_usec'] = (int(fields[11]) + int(fields[12])) * 1_000_000 / os.sysconf('SC_CLK_TCK')
                        metric['actual_mhz'] = int(pathlib.Path('/sys/class/drm/card1/gt_act_freq_mhz').read_text())
                    except (OSError, ValueError, KeyError):
                        pass
                    runtime_metrics.append(metric)
                    if len(samples) % 15 == 1:
                        stream = socket.create_connection(('127.0.0.1', http_port), timeout=1)
                        stream.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1024)
                        stream.sendall(b'GET /events HTTP/1.1\r\nHost: localhost\r\n\r\n')
                        slow_clients.append(stream)
                        if len(slow_clients) > 2:
                            slow_clients.pop(0).close()
                except (OSError, ValueError):
                    if time.monotonic() - started > 20:
                        raise
                time.sleep(1)
            if not samples or not any(s.get('lifetime_hashes', 0) > 0 for s in samples):
                raise RuntimeError('no completed GPU work/status evidence')
            if args.seconds >= 130 and pool.counts['connections'] < 2:
                raise RuntimeError('forced reconnect not completed')
        except Exception as error:
            failure = str(error)
        finally:
            stop_started = time.monotonic()
            if args.image:
                subprocess.run(['docker', 'stop', '--time', '6', container], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
            else:
                proc.terminate()
            try:
                exit_code = proc.wait(timeout=7)
            except subprocess.TimeoutExpired:
                proc.kill()
                exit_code = proc.wait(timeout=2)
                failure = failure or 'shutdown deadline exceeded'
            shutdown_seconds = time.monotonic() - stop_started
            for stream in slow_clients:
                stream.close()
            pool.close()
    if pool.error:
        failure = failure or pool.error
    if pool.counts['invalid'] or pool.counts['duplicates']:
        failure = failure or 'invalid or duplicate submitted shares'
    if (args.seconds >= 1800 or args.require_shares) and not pool.counts['accepted']:
        failure = failure or 'no independently validated shares observed'
    if exit_code != 0:
        failure = failure or f'nonzero miner exit {exit_code}'
    if shutdown_seconds > 6:
        failure = failure or 'shutdown exceeded 6 seconds'
    if args.validation and 'Vulkan validation error:' in (output / 'miner.log').read_text(errors='replace'):
        failure = failure or 'Vulkan validation errors'
    summary = dict(ok=failure is None, error=failure, backend=args.backend,
                   seconds=time.monotonic() - started, shutdown_seconds=shutdown_seconds,
                   exit_code=exit_code, pool=pool.counts, status_samples=len(samples),
                   observed_share_validation=pool.counts['accepted'] > 0,
                   last_status=samples[-1] if samples else None)
    (output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    (output / 'runtime-metrics.jsonl').write_text(''.join(json.dumps(sample) + '\n' for sample in runtime_metrics))
    (output / 'status.jsonl').write_text(''.join(json.dumps(sample) + '\n' for sample in samples))
    print(json.dumps({key: value for key, value in summary.items() if key != 'last_status'}))
    return 0 if failure is None else 1


if __name__ == '__main__':
    raise SystemExit(main())
