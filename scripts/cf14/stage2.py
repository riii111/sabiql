#!/usr/bin/env python3
"""Bounded CSV and reducer/EffectRunner measurements using a root test binary."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import statistics
import subprocess
import sys
import tempfile
import time

TESTS = {'csv': 'tests::cf14_measurements::csv_export_records_rows_bytes_and_failure_cleanup',
         'connection': 'tests::cf14_measurements::connection_switch_records_metadata_and_testbackend_draw'}


def reclaim_process_group(group):
    try:
        os.killpg(group, signal.SIGKILL)
    except ProcessLookupError:
        return False
    deadline = time.monotonic() + 2
    while True:
        try:
            os.killpg(group, 0)
        except ProcessLookupError:
            return False
        if time.monotonic() >= deadline:
            return True
        time.sleep(0.05)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--observe', action='store_true')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    args.output.mkdir(parents=True, exist_ok=False)
    binary = args.binary.resolve()
    manifest = dict(base=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
                    platform=platform.platform(), python_executable=sys.executable, python_version=sys.version,
                    repetitions=1 if args.observe else 3,
                    delay_ms=[0] if args.observe else [0, 50, 200], tables=[10] if args.observe else [10, 100, 1000],
                    csv_rows=[50] if args.observe else [50, 1000], deadline_seconds=600,
                    byte_limit=10*1024*1024, process_deadline_seconds=60,
                    binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                    sources={str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest() for p in
                             [root/'src/tests/cf14_measurements.rs', root/'scripts/cf14/mysql', Path(__file__)]},
                    p95='nearest rank; n=3 means maximum', real_db_connections=None, rtt_ms=None,
                    max_fake_processes=None, db_cold_warm=None, live_terminal_display=None)
    (args.output/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    deadline = time.monotonic()+manifest['deadline_seconds']
    cases = [('csv', delay, 10, rows, fault) for delay in manifest['delay_ms'] for rows in manifest['csv_rows']
             for fault in (['none'] if args.observe else ['none', 'truncate'])]
    cases += [('connection', delay, tables, 50, 'none') for delay in manifest['delay_ms'] for tables in manifest['tables']]
    all_samples = []
    with tempfile.TemporaryDirectory(prefix='cf14-stage2-') as scratch:
        for operation, delay, tables, rows, fault in cases:
            for repetition in range(manifest['repetitions']):
                if time.monotonic() >= deadline or sum(p.stat().st_size for p in args.output.iterdir()) >= manifest['byte_limit']:
                    raise RuntimeError('budget reached; raw retained')
                if hashlib.sha256(binary.read_bytes()).hexdigest() != manifest['binary_sha256']:
                    raise RuntimeError('binary changed during run; raw retained')
                key = f'{operation}-{delay}-{tables}-{rows}-{fault}-{repetition}'
                log = (args.output/(key+'-cli.jsonl')).resolve()
                sample_path = (args.output/(key+'-sample.jsonl')).resolve()
                env = dict(os.environ, PATH=str(root/'scripts/cf14')+os.pathsep+os.environ['PATH'],
                           CF14_LOG=str(log), CF14_SAMPLE=str(sample_path), CF14_TABLES=str(tables),
                           CF14_CSV_ROWS=str(rows), CF14_CSV_FAULT=fault, CF14_DELAY_MS=str(delay), TMPDIR=scratch)
                env.pop('CF14_FAULT_PORT', None)
                with subprocess.Popen([str(binary), TESTS[operation], '--exact', '--ignored', '--nocapture'],
                                      env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                      text=True, start_new_session=True) as child:
                    try:
                        output, _ = child.communicate(timeout=min(60, max(0.1, deadline-time.monotonic())))
                    except BaseException:
                        os.killpg(child.pid, signal.SIGKILL)
                        output, _ = child.communicate()
                        (args.output/(key+'-test.txt')).write_text(output)
                        raise
                (args.output/(key+'-test.txt')).write_text(output)
                events = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
                time.sleep(0.5)
                residual = []
                for pid in {e['pid'] for e in events if e['kind'] == 'start'}:
                    try:
                        os.kill(pid, 0)
                        residual.append(pid)
                    except ProcessLookupError:
                        pass
                (args.output/(key+'-cleanup.json')).write_text(json.dumps(dict(residual_pids_after_500ms=residual))+'\n')
                if child.returncode or residual:
                    group_remaining = reclaim_process_group(child.pid)
                    (args.output/(key+'-reclaim.json')).write_text(
                        json.dumps(dict(owned_process_group=child.pid, group_exists_after_cleanup=group_remaining))+'\n')
                    raise RuntimeError(f'{key}: return code {child.returncode}; residual {residual}; group remaining {group_remaining}; raw retained')
                measurements = [json.loads(line) for line in sample_path.read_text().splitlines()]
                assert len(measurements) == (1 if operation == 'csv' else 3)
                for measurement in measurements:
                    selected = [e for e in events if measurement['start_ns'] <= e['ns'] <= measurement['end_ns']]
                    sample = dict(operation=operation, delay_ms=delay, tables=tables, rows=rows, fault=fault,
                                  repetition=repetition, **measurement,
                                  fake_processes=sum(e['kind'] == 'start' for e in selected),
                                  sql_count=sum(e['kind'] == 'sql' for e in selected),
                                  generated_xml_bytes=sum(e.get('output_bytes', 0) for e in selected),
                                  max_fake_processes=None, real_db_connections=None, rtt_ms=None)
                    all_samples.append(sample)
                    with (args.output/'samples.jsonl').open('a') as stream:
                        stream.write(json.dumps(sample)+'\n')
    groups = {}
    for sample in all_samples:
        key = (sample['operation'], sample['delay_ms'], sample['tables'], sample['rows'], sample['fault'], sample['phase'])
        groups.setdefault(key, []).append(sample)
    summary = []
    for key, samples in groups.items():
        result = dict(zip(['operation', 'delay_ms', 'tables', 'rows', 'fault', 'phase'], key))
        result['n'] = len(samples)
        for metric in ['elapsed_ms', 'list_state_ms', 'metadata_loaded_ms', 'first_list_draw_ms', 'refreshed_list_draw_ms']:
            values = [s[metric] for s in samples if s.get(metric) is not None]
            if values:
                result[metric] = dict(median=statistics.median(values), p95=max(values), min=min(values), max=max(values))
        summary.append(result)
    (args.output/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    assert sum(p.stat().st_size for p in args.output.iterdir()) < manifest['byte_limit']
    print(f'{len(all_samples)} samples retained in {args.output}')


if __name__ == '__main__':
    main()
