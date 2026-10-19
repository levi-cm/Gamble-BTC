#!/usr/bin/env python3
"""Compare immutable BTC images alongside four offline RandomX workers.

Uses existing huge pages if available; never provisions them, accesses MSRs,
changes priority/affinity, contacts a pool or stops an existing miner.
"""
import argparse
import datetime
import importlib.util
import json
import pathlib
import re
import signal
import subprocess
import threading
import time


def randomx_config(log_path):
    return {
        'autosave': False, 'background': False, 'colors': False, 'title': False,
        'dmi': False, 'http': {'enabled': False}, 'print-time': 30,
        'log-file': str(log_path), 'donate-level': 0, 'pools': [],
        'randomx': {'init': 4, 'mode': 'fast', '1gb-pages': False, 'rdmsr': False,
                    'wrmsr': False, 'cache_qos': False, 'numa': False},
        'cpu': {'enabled': True, 'rx': [-1] * 4, 'huge-pages': True,
                'huge-pages-jit': False, 'yield': True, 'priority': None},
        'opencl': {'enabled': False}, 'cuda': {'enabled': False},
        'benchmark': {'size': '10M', 'algo': 'rx/0', 'submit': False},
    }


def parse_progress(text):
    pattern = re.compile(r'^\[([^]]+)\].*?\bbench\s+\S+%\s+(\d+)/(\d+)\s+\(([\d.]+)s\)')
    rows = []
    for line in text.splitlines():
        match = pattern.search(line)
        if match:
            stamp, count, total, elapsed = match.groups()
            rows.append({'wall_time': datetime.datetime.fromisoformat(stamp).timestamp(),
                         'hashes': int(count), 'total': int(total), 'elapsed': float(elapsed)})
    return rows


def progress_rate(rows, start, end):
    chosen = [row for row in rows if start <= row['wall_time'] <= end]
    result = {'h_s': None, 'covered_seconds': None, 'samples': len(chosen)}
    if len(chosen) >= 2:
        elapsed = chosen[-1]['elapsed'] - chosen[0]['elapsed']
        count = chosen[-1]['hashes'] - chosen[0]['hashes']
        if elapsed > 0 and count >= 0:
            result.update(h_s=count / elapsed, covered_seconds=elapsed)
    return result


def validate_cases(cases):
    if not isinstance(cases, list) or not cases:
        raise ValueError('cases must be a nonempty JSON array')
    labels = set()
    allowed = {'label', 'image', 'backend', 'kernel', 'local', 'poll', 'batch',
               'wait_mode', 'wait_us', 'queue_ms'}
    for case in cases:
        if not isinstance(case, dict) or set(case) - allowed:
            raise ValueError('unknown case fields')
        label = case.get('label', '')
        if not re.fullmatch(r'[a-z0-9][a-z0-9-]*', label) or label in labels:
            raise ValueError('case labels must be unique safe lowercase names')
        if not isinstance(case.get('image'), str) or not case['image']:
            raise ValueError('each case requires an image')
        if case.get('wait_mode', 'fence') not in ('fence', 'poll'):
            raise ValueError('invalid wait mode')
        if case.get('backend', 'vulkan') not in ('vulkan', 'opencl'):
            raise ValueError('invalid backend')
        labels.add(label)
    return cases


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--xmrig', required=True, type=pathlib.Path)
    parser.add_argument('--cases', required=True, type=pathlib.Path,
                        help='JSON array: label,image,backend,kernel,local,wait_mode,wait_us,queue_ms')
    parser.add_argument('--order', default='baseline,candidate,candidate,baseline')
    parser.add_argument('--seconds', default=300, type=int)
    parser.add_argument('--warmup', default=30, type=int)
    parser.add_argument('--control-seconds', default=300, type=int)
    parser.add_argument('--alone', default='', help='comma-separated case labels for BTC-only trials before XMRig')
    parser.add_argument('--runtime-image', help='optional image for a 300s local Stratum/HTTP coexistence check')
    parser.add_argument('--output', required=True, type=pathlib.Path)
    args = parser.parse_args()
    if args.seconds < 300 or args.control_seconds < 300 or args.warmup < 30:
        parser.error('use at least 300 measured/control seconds and 30 warmup seconds')
    if not args.xmrig.is_file():
        parser.error('XMRig executable is missing')
    try:
        cases = {c['label']: dict(c) for c in validate_cases(json.loads(args.cases.read_text()))}
        order = args.order.split(',')
        alone = args.alone.split(',') if args.alone else []
        if any(label not in cases for label in alone):
            raise ValueError('alone contains an unknown label')
        if not order or any(label not in cases for label in order):
            raise ValueError('order contains an unknown label')
    except (ValueError, OSError) as error:
        parser.error(str(error))
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    spec = importlib.util.spec_from_file_location('bench_rust', pathlib.Path(__file__).with_name('bench-rust.py'))
    bench = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(bench)
    # Resolve every tag before starting the CPU workload.
    for case in cases.values():
        case['image'] = bench.docker('image', 'inspect', '--format', '{{.Id}}', case['image'])
    runtime_image = bench.docker('image', 'inspect', '--format', '{{.Id}}', args.runtime_image) if args.runtime_image else None
    (output / 'cases.json').write_text(json.dumps(cases, indent=2) + '\n')
    for label in alone:
        case = cases[label]
        bench.run(case['image'], case.get('backend', 'vulkan'),
                  case.get('kernel', 'pre3'), case.get('local', 64),
                  args.seconds, args.warmup, output / (label + '-alone'),
                  case.get('poll', 1000), case.get('batch', 16777216),
                  case.get('wait_mode'), case.get('wait_us'), case.get('queue_ms'))
    config = output / 'xmrig-offline.json'
    log_path = output / 'xmrig.log'
    config.write_text(json.dumps(randomx_config(log_path), indent=2) + '\n')
    process = subprocess.Popen([str(args.xmrig.resolve()), '-c', str(config)],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                               stdin=subprocess.DEVNULL, cwd='/tmp')
    done = threading.Event()
    stage = ['initializing']

    def heartbeat():
        while not done.wait(50):
            print(json.dumps({'progress': stage[0], 'xmrig_alive': process.poll() is None}), flush=True)

    thread = threading.Thread(target=heartbeat, daemon=True)
    thread.start()
    reports = []
    runtime_process = None
    try:
        deadline = time.monotonic() + 120
        while True:
            if process.poll() is not None:
                raise RuntimeError('XMRig exited before readiness; inspect xmrig.log')
            if log_path.exists() and 'READY threads 4/4' in log_path.read_text():
                break
            if time.monotonic() >= deadline:
                raise RuntimeError('XMRig startup deadline exceeded')
            time.sleep(.25)
        stage[0] = 'xmrig-alone-control'
        start = time.time()
        control_samples = []
        until = time.monotonic() + args.control_seconds
        while time.monotonic() < until:
            if process.poll() is not None:
                raise RuntimeError('XMRig exited during control')
            control_samples.append(dict(elapsed=time.time() - start, rapl=bench.energy_counters()))
            time.sleep(1)
        (output / 'control-metrics.json').write_text(json.dumps(control_samples, indent=2) + '\n')
        control = progress_rate(parse_progress(log_path.read_text()), start, time.time())
        (output / 'control.json').write_text(json.dumps(control, indent=2) + '\n')
        for index, label in enumerate(order, 1):
            case = cases[label]
            stage[0] = f'{index}-{label}'
            start = time.time()
            report = bench.run(case['image'], case.get('backend', 'vulkan'),
                               case.get('kernel', 'pre3'), case.get('local', 64),
                               args.seconds, args.warmup, output / stage[0],
                               case.get('poll', 1000), case.get('batch', 16777216),
                               case.get('wait_mode'), case.get('wait_us'), case.get('queue_ms'))
            if process.poll() is not None:
                raise RuntimeError('XMRig did not remain active throughout BTC run')
            xmrig = progress_rate(parse_progress(log_path.read_text()), start + args.warmup + 2, time.time())
            reports.append({'label': label, 'report': report['report'],
                            'cpu_cores': report['cpu_cores'], 'xmrig': xmrig})
            (output / 'summary.json').write_text(json.dumps(reports, indent=2) + '\n')
        if runtime_image:
            stage[0] = 'local-stratum-http'
            start = time.time()
            runtime_process = subprocess.Popen(['python3', str(pathlib.Path(__file__).with_name('qualify-rust.py')),
                            '--image', runtime_image, '--seconds', '330', '--require-shares',
                            '--output', str(output / 'runtime')])
            if runtime_process.wait(timeout=360) != 0:
                raise RuntimeError('local runtime coexistence check failed')
            if process.poll() is not None:
                raise RuntimeError('XMRig exited during runtime coexistence check')
            runtime_rate = progress_rate(parse_progress(log_path.read_text()), start + 30, time.time())
            (output / 'runtime-xmrig.json').write_text(json.dumps(runtime_rate, indent=2) + '\n')
        print(json.dumps({'complete': True, 'output': str(output), 'control': control}), flush=True)
    finally:
        if runtime_process is not None and runtime_process.poll() is None:
            runtime_process.send_signal(signal.SIGINT)
            try:
                runtime_process.wait(timeout=12)
            except subprocess.TimeoutExpired:
                runtime_process.terminate()
                runtime_process.wait(timeout=5)
        # Popen owns exactly this child; no broad process matching or exit-node/system changes.
        if process.poll() is None:
            process.send_signal(signal.SIGINT)
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.wait(timeout=10)
        done.set()
        thread.join(timeout=2)


if __name__ == '__main__':
    main()
