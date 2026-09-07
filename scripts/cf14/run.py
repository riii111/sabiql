#!/usr/bin/env python3
"""Bounded adapter measurements; output must be a new persistent directory."""
import argparse
import json
import math
import os
import signal
from pathlib import Path
import platform
import statistics
import subprocess
import tempfile
import time

OPERATIONS = ['startup', 'db_selection', 'metadata_reload', 'table_selection', 'inspector', 'preview', 'page', 'completion', 'er_metadata']


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--smoke', action='store_true')
    parser.add_argument('--operations', nargs='+', choices=OPERATIONS, default=OPERATIONS)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    binary = args.binary.resolve()
    root = Path(__file__).resolve().parents[2]
    manifest = dict(base=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
                    platform=platform.platform(), python=platform.python_version(), repetitions=1 if args.smoke else 5,
                    delay_ms=[0] if args.smoke else [0, 50, 200], tables=[10] if args.smoke else [10, 100, 1000],
                    operations=args.operations, delay_location='before each nonempty XML response; SET has no delay',
                    phase_definition='first and second operation in the same fresh harness; no app/DB cache',
                    p95='nearest rank; n=5 means maximum', deadline_seconds=1200, byte_limit=20*1024*1024)
    (args.output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    deadline = time.monotonic() + 1200
    samples = []
    with tempfile.TemporaryDirectory(prefix='cf14-') as scratch:
        for delay in manifest['delay_ms']:
            for tables in manifest['tables']:
                for operation in args.operations:
                    for repetition in range(manifest['repetitions']):
                        if time.monotonic() >= deadline or sum(p.stat().st_size for p in args.output.iterdir()) > manifest['byte_limit']:
                            raise RuntimeError('measurement budget reached; partial raw retained')
                        key = f'{delay}-{tables}-{operation}-{repetition}'
                        log = args.output / (key + '.jsonl')
                        env = dict(os.environ, PATH=str(root / 'scripts/cf14') + os.pathsep + os.environ['PATH'],
                                   CF14_LOG=str(log.resolve()), CF14_TABLES=str(tables), CF14_DELAY_MS=str(delay),
                                   TMPDIR=scratch)
                        env.pop('CF14_FAULT_PORT', None)
                        with subprocess.Popen([str(binary), operation], env=env, stdout=subprocess.PIPE,
                                              stderr=subprocess.PIPE, text=True, start_new_session=True) as child:
                            try:
                                stdout, stderr = child.communicate(timeout=min(80, max(1, deadline-time.monotonic())))
                            except BaseException:
                                os.killpg(child.pid, signal.SIGKILL)
                                child.wait()
                                raise
                            run = subprocess.CompletedProcess(child.args, child.returncode, stdout, stderr)
                        if run.returncode:
                            raise RuntimeError(run.stderr)
                        events = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
                        for measurement in map(json.loads, run.stdout.splitlines()):
                            selected = [event for event in events if measurement['start_ns'] <= event['ns'] <= measurement['end_ns']]
                            remaining = []
                            for pid in {e['pid'] for e in selected if e['kind'] == 'start'}:
                                try:
                                    os.kill(pid, 0)
                                    remaining.append(pid)
                                except ProcessLookupError:
                                    pass
                            sample = dict(operation=operation, delay_ms=delay, tables=tables, repetition=repetition,
                                          **measurement, processes=sum(e['kind']=='start' for e in selected),
                                          sql_count=sum(e['kind']=='sql' for e in selected),
                                          sql_text_bytes=sum(e.get('input_bytes', 0) for e in selected),
                                          xml_bytes=sum(e.get('output_bytes', 0) for e in selected),
                                          max_fake_processes=None, residual_pids=remaining, db_connections=None,
                                          rtt_ms=None, render_ms=None, app_cache=None, timeout_stage=None)
                            samples.append(sample)
                            with (args.output / 'samples.jsonl').open('a') as stream:
                                stream.write(json.dumps(sample) + '\n')
                            if sample['error'] or remaining:
                                raise RuntimeError(f'{key}: {sample}')
                        print(key, flush=True)
    groups = {}
    for sample in samples:
        key = (sample['operation'], sample['delay_ms'], sample['tables'], sample['phase'])
        groups.setdefault(key, []).append(sample['elapsed_ms'])
    summary = [dict(operation=key[0], delay_ms=key[1], tables=key[2], phase=key[3], n=len(values),
                    median_ms=statistics.median(values), p95_ms=sorted(values)[math.ceil(.95*len(values))-1],
                    min_ms=min(values), max_ms=max(values)) for key, values in groups.items()]
    (args.output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')


if __name__ == '__main__':
    main()
