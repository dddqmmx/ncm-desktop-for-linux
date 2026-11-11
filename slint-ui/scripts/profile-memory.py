#!/usr/bin/env python3
"""Read Linux process-tree memory without attaching a debugger or reading credentials."""
import argparse
import datetime
import json
from pathlib import Path
import subprocess
import time


def process_tree(root):
    parents = {}
    for directory in Path('/proc').iterdir():
        if not directory.name.isdigit():
            continue
        try:
            # comm may contain spaces or parentheses; fields after the last ')' are stable.
            fields = (directory / 'stat').read_text().rsplit(')', 1)[1].split()
            parents[int(directory.name)] = int(fields[1])
        except (OSError, IndexError, ValueError):
            continue
    found = {root}
    while True:
        expanded = found | {pid for pid, parent in parents.items() if parent in found}
        if expanded == found:
            return sorted(found)
        found = expanded


def memory(pid):
    directory = Path('/proc') / str(pid)
    values = {}
    for line in (directory / 'smaps_rollup').read_text().splitlines()[1:]:
        key, value = line.split(':', 1)
        values[key] = int(value.split()[0])
    mappings = {}
    name = '[anonymous]'
    for line in (directory / 'smaps').read_text().splitlines():
        fields = line.split()
        if '-' in fields[0] and ':' not in fields[0]:
            name = ' '.join(fields[5:]) or '[anonymous]'
        elif fields[0] == 'Pss:':
            mappings[name] = mappings.get(name, 0) + int(fields[1])
    return {
        'pid': pid,
        'name': (directory / 'comm').read_text().strip(),
        'rss_kib': values['Rss'],
        'pss_kib': values['Pss'],
        'uss_kib': values['Private_Clean'] + values['Private_Dirty'],
        'swap_pss_kib': values.get('SwapPss', 0),
        'rollup_kib': values,
        'mapping_pss_kib': dict(sorted(mappings.items(), key=lambda pair: -pair[1])),
    }


def snapshot(root):
    processes, errors = [], []
    for pid in process_tree(root):
        try:
            processes.append(memory(pid))
        except (OSError, ValueError) as error:
            errors.append({'pid': pid, 'error': str(error)})
    gpu, gpu_status = {}, 'unavailable'
    try:
        result = subprocess.run(
            ['nvidia-smi', '--query-compute-apps=pid,used_memory', '--format=csv,noheader,nounits'],
            capture_output=True, text=True, timeout=5, check=True,
        )
        pids = {p['pid'] for p in processes}
        for line in result.stdout.splitlines():
            pid, used = line.split(',')
            if int(pid) in pids and used.strip().isdigit():
                gpu[int(pid)] = int(used)
        # An absent process is NOT zero VRAM: this query may omit graphics-only clients.
        gpu_status = 'compute-apps query; graphics-only clients may be absent'
    except (OSError, ValueError, subprocess.SubprocessError):
        pass
    return {
        'time': datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'root_pid': root, 'processes': processes, 'errors': errors,
        'total': {key: sum(p[key] for p in processes)
                  for key in ('rss_kib', 'pss_kib', 'uss_kib', 'swap_pss_kib')},
        'nvidia_reported_mib': gpu, 'nvidia_query_status': gpu_status,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('pid', type=int)
    parser.add_argument('--samples', type=int, default=1)
    parser.add_argument('--interval', type=float, default=1)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if args.samples < 1 or args.interval < 0:
        parser.error('samples must be positive and interval nonnegative')
    samples = []
    for i in range(args.samples):
        if i:
            time.sleep(args.interval)
        item = snapshot(args.pid)
        samples.append(item)
        print(json.dumps(item, ensure_ascii=False), flush=True)
        if args.output:
            args.output.write_text(json.dumps(samples, ensure_ascii=False, indent=2) + '\n')
    if not samples[-1]['processes']:
        raise SystemExit('No process could be measured; see errors above.')


if __name__ == '__main__':
    main()
