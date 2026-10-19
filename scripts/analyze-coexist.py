#!/usr/bin/env python3
"""Summarize bench-coexist artifacts without attributing package energy to a GPU."""
import argparse
import json
import pathlib
import statistics


def mean(values):
    values = [value for value in values if value is not None]
    return statistics.mean(values) if values else None


def summarize(data):
    report = data['report']
    settings = data['settings']
    measured = [row for row in data['samples']
                if row['elapsed'] >= int(settings['GBTC_BENCH_WARMUP_SECONDS']) + 2]
    result = dict(mh_s=report['mh_s'], seconds=report['seconds'],
                  cpu_percent_one_core=data['cpu_cores'] * 100 if data['cpu_cores'] is not None else None,
                  gt_mhz_mean=mean([row['actual_mhz'] for row in measured]),
                  pl1_fraction=mean([row['pl1'] for row in measured]),
                  thermal_fraction=mean([row['thermal'] for row in measured]),
                  image=data['image'], kernel=report['kernel'], local_size=report['local_size'])
    if len(measured) >= 2:
        first, last = measured[0], measured[-1]
        elapsed = last['elapsed'] - first['elapsed']
        for key in ['voluntary_ctxt_switches', 'nonvoluntary_ctxt_switches', 'runqueue_ns']:
            if key in first.get('scheduler', {}) and key in last.get('scheduler', {}):
                result[key + '_per_second'] = (last['scheduler'][key] - first['scheduler'][key]) / elapsed
        for domain in ['intel-rapl:0', 'intel-rapl:0:0', 'intel-rapl:0:1', 'intel-rapl:1']:
            a, b = first.get('rapl', {}).get(domain), last.get('rapl', {}).get(domain)
            if a and b and all(x is not None for x in [a['energy_uj'], b['energy_uj'], b['max_energy_range_uj']]):
                joules = ((b['energy_uj'] - a['energy_uj']) % b['max_energy_range_uj']) / 1e6
                result[domain + '_watts'] = joules / elapsed
    telemetry = report.get('backend_telemetry_delta', {})
    if telemetry.get('gpu_ns'):
        result['gpu_execution_fraction'] = telemetry['gpu_ns'] / 1e9 / report['seconds']
        result['gpu_execution_mh_s'] = report['hashes'] / telemetry['gpu_ns'] * 1000
    for key in ['poll_checks', 'fence_waits', 'submissions']:
        if key in telemetry:
            result[key + '_per_second'] = telemetry[key] / report['seconds']
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=pathlib.Path)
    args = parser.parse_args()
    rows = []
    summary_path = args.directory / 'summary.json'
    loaded = json.loads(summary_path.read_text()) if summary_path.exists() else []
    for path in sorted(args.directory.glob('*.json')):
        data = json.loads(path.read_text())
        if not isinstance(data, dict) or not {'report', 'samples', 'settings'} <= data.keys():
            continue
        result = summarize(data)
        result['trial'] = path.stem
        parts = path.stem.split('-', 1)
        if parts[0].isdigit():
            index = int(parts[0]) - 1
            if index < len(loaded):
                result['xmrig'] = loaded[index]['xmrig']
        rows.append(result)
    output = {'trials': rows}
    control = args.directory / 'control.json'
    if control.exists():
        output['xmrig_alone'] = json.loads(control.read_text())
    (args.directory / 'analysis.json').write_text(json.dumps(output, indent=2) + '\n')
    print(json.dumps(output, indent=2))


if __name__ == '__main__':
    main()
