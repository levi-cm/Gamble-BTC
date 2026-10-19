#!/usr/bin/env python3
"""Serial, wallet-free container measurements with immutable image IDs and telemetry."""
import argparse
import json
import os
import pathlib
import subprocess
import time
import uuid


def docker(*args, **kwargs):
    return subprocess.check_output(['docker', *args], text=True, **kwargs).strip()


def read_number(path):
    try:
        return int(path.read_text().strip())
    except (OSError, ValueError):
        return None


def process_counters(pid):
    result = {}
    try:
        for line in pathlib.Path('/proc', str(pid), 'status').read_text().splitlines():
            if line.startswith(('voluntary_ctxt_switches:', 'nonvoluntary_ctxt_switches:')):
                key, value = line.split(':', 1)
                result[key] = int(value)
        sched = pathlib.Path('/proc', str(pid), 'schedstat').read_text().split()
        result.update(runtime_ns=int(sched[0]), runqueue_ns=int(sched[1]))
    except (OSError, ValueError, IndexError):
        pass
    return result


def energy_counters():
    result = {}
    for path in pathlib.Path('/sys/class/powercap').glob('intel-rapl*/**/energy_uj'):
        try:
            name = path.parent.joinpath('name').read_text().strip()
            result[str(path.parent.name)] = dict(name=name, energy_uj=read_number(path),
                max_energy_range_uj=read_number(path.parent / 'max_energy_range_uj'))
        except OSError:
            pass
    return result


def run(image, backend, kernel, local, seconds, warmup, output, poll=1000, batch=16777216,
        wait_mode=None, wait_us=None, queue_ms=None):
    image_id = docker('image', 'inspect', '--format', '{{.Id}}', image)
    name = 'gbtc-measure-' + uuid.uuid4().hex[:12]
    settings = dict(GBTC_BENCH_ONLY='1', GBTC_BACKEND=backend,
                    GBTC_DEVICE='/dev/dri/renderD128', GBTC_BATCH_NONCES=str(batch),
                    GBTC_BENCH_SECONDS=str(seconds), GBTC_BENCH_WARMUP_SECONDS=str(warmup),
                    GBTC_VULKAN_KERNEL=kernel if backend == 'vulkan' else 'unrolled',
                    GBTC_VULKAN_LOCAL_SIZE=str(local), GBTC_VULKAN_POLL_US=str(poll),
                    GBTC_OPENCL_KERNEL=kernel if backend == 'opencl' else 'unrolled',
                    GBTC_OPENCL_LOCAL_SIZE=str(local), GBTC_OPENCL_POLL_US=str(poll),
                    GBTC_OPENCL_SIMD='16')
    for key, value in [('GBTC_VULKAN_WAIT_MODE', wait_mode),
                       ('GBTC_VULKAN_WAIT_US', wait_us), ('GBTC_GPU_QUEUE_MS', queue_ms)]:
        if value is not None:
            settings[key] = str(value)
    cmd = ['run', '-d', '--name', name, '--network', 'none',
           '--device', '/dev/dri/renderD128', '--group-add', str(os.stat('/dev/dri/renderD128').st_gid)]
    for key, val in settings.items():
        cmd += ['-e', f'{key}={val}']
    cmd += [image_id]
    competing = subprocess.check_output(['ps', '-eo', 'comm,pcpu', '--sort=-pcpu'], text=True).splitlines()[1:9]
    start = time.monotonic()
    samples = []
    last_progress = start
    try:
        docker(*cmd)
        info = json.loads(docker('inspect', name))[0]
        pid = info['State']['Pid']
        cgroup = pathlib.Path('/proc', str(pid), 'cgroup').read_text().strip().split('::', 1)[1]
        cpu_stat = pathlib.Path('/sys/fs/cgroup' + cgroup, 'cpu.stat')
        drm = pathlib.Path('/sys/class/drm/card1')
        while True:
            state = json.loads(docker('inspect', '--format', '{{json .State}}', name))
            if not state['Running']:
                break
            if time.monotonic() - start > seconds + warmup + 120:
                raise RuntimeError('benchmark deadline exceeded')
            try:
                cpu = dict(line.split() for line in cpu_stat.read_text().splitlines())
                usage = int(cpu['usage_usec'])
            except (OSError, KeyError):
                usage = None
            samples.append(dict(elapsed=time.monotonic()-start, cpu_usec=usage,
                                scheduler=process_counters(pid), rapl=energy_counters(),
                                gt_mhz=read_number(drm / 'gt_cur_freq_mhz'),
                                actual_mhz=read_number(drm / 'gt_act_freq_mhz'),
                                pl1=read_number(drm / 'gt/gt0/throttle_reason_pl1'),
                                pl2=read_number(drm / 'gt/gt0/throttle_reason_pl2'),
                                thermal=read_number(drm / 'gt/gt0/throttle_reason_thermal'),
                                ac=read_number(pathlib.Path('/sys/class/power_supply/ACAD/online'))))
            if time.monotonic() - last_progress >= 50:
                print(json.dumps(dict(progress=output.name, elapsed=round(time.monotonic()-start),
                                      actual_mhz=samples[-1]['actual_mhz'])), flush=True)
                last_progress = time.monotonic()
            time.sleep(1)
        log = docker('logs', name, stderr=subprocess.STDOUT)
        output.with_suffix('.log').write_text(log + '\n')
        reports = [json.loads(line) for line in log.splitlines() if line.startswith('{')]
        if state['ExitCode'] != 0 or not reports or not reports[-1].get('ok'):
            raise RuntimeError(f'benchmark failed: exit {state["ExitCode"]}; see {output.with_suffix(".log")}')
        report = reports[-1]
        measured = [s for s in samples if s['elapsed'] >= warmup + 2 and s['cpu_usec'] is not None]
        cpu_cores = None
        if len(measured) >= 2:
            first, last = measured[0], measured[-1]
            cpu_cores = (last['cpu_usec']-first['cpu_usec'])/1e6/(last['elapsed']-first['elapsed'])
        result = dict(image=image_id, settings=settings, competing_processes=competing, report=report, samples=samples,
                      cpu_cores=cpu_cores, cpu_interval='warmup+2s to last sample; startup excluded',
                      wall_seconds=time.monotonic()-start)
        output.with_suffix('.json').write_text(json.dumps(result, indent=2)+'\n')
        print(json.dumps(dict(output=str(output.with_suffix('.json')), backend=backend,
                              kernel=kernel, local=local, mh_s=report['mh_s'], cpu_cores=cpu_cores)), flush=True)
        return result
    finally:
        subprocess.run(['docker', 'rm', '-f', name], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--image', default='gamble-btc:rust-vulkan-candidate')
    p.add_argument('--backend', choices=['vulkan', 'opencl'], default='vulkan')
    p.add_argument('--kernel', default='pre3')
    p.add_argument('--local', type=int, default=64)
    p.add_argument('--seconds', type=int, default=60)
    p.add_argument('--warmup', type=int, default=30)
    p.add_argument('--poll', type=int, default=1000)
    p.add_argument('--batch', type=int, default=16777216)
    p.add_argument('--wait-mode', choices=['fence', 'poll'])
    p.add_argument('--wait-us', type=int)
    p.add_argument('--queue-ms', type=int)
    p.add_argument('--output', default='bench-results/rust-measurement')
    a = p.parse_args()
    if a.seconds < 1 or a.warmup < 1:
        p.error('durations must be positive')
    output = pathlib.Path(a.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    if output.with_suffix('.json').exists():
        p.error('output already exists; choose another path')
    run(a.image, a.backend, a.kernel, a.local, a.seconds, a.warmup, output,
        a.poll, a.batch, a.wait_mode, a.wait_us, a.queue_ms)


if __name__ == '__main__':
    main()
