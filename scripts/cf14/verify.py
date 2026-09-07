#!/usr/bin/env python3
"""Cross-check retained event totals, sample coverage, and summary calculations."""
import json
import math
from pathlib import Path
import statistics
import sys


def main():
    directory = Path(sys.argv[1])
    manifest = json.loads((directory/'manifest.json').read_text())
    samples = [json.loads(line) for line in (directory/'samples.jsonl').read_text().splitlines()]
    expected = {(operation, delay, tables, repetition, phase)
                for operation in manifest['operations'] for delay in manifest['delay_ms']
                for tables in manifest['tables'] for repetition in range(manifest['repetitions'])
                for phase in ['fresh-harness', 'repeated-harness']}
    actual = [(s['operation'], s['delay_ms'], s['tables'], s['repetition'], s['phase']) for s in samples]
    assert len(actual) == len(set(actual)) and set(actual) == expected, 'coverage or duplicate sample'
    groups = {}
    for sample in samples:
        key = f"{sample['delay_ms']}-{sample['tables']}-{sample['operation']}-{sample['repetition']}"
        events = [json.loads(line) for line in (directory/(key+'.jsonl')).read_text().splitlines()]
        events = [e for e in events if sample['start_ns'] <= e['ns'] <= sample['end_ns']]
        assert not sample['error'] and not sample['residual_pids'], key
        assert sample['max_fake_processes'] is None
        assert sample['db_connections'] is None and sample['rtt_ms'] is None and sample['render_ms'] is None
        assert sum(e['kind']=='start' for e in events) == sample['processes'], key
        assert sum(e['kind']=='sql' for e in events) == sample['sql_count'], key
        assert sum(e.get('input_bytes',0) for e in events) == sample['sql_text_bytes'], key
        assert sum(e.get('output_bytes',0) for e in events) == sample['xml_bytes'], key
        group = (sample['operation'],sample['delay_ms'],sample['tables'],sample['phase'])
        groups.setdefault(group,[]).append(sample['elapsed_ms'])
    summaries = json.loads((directory/'summary.json').read_text())
    assert len(summaries) == len(groups)
    for summary in summaries:
        values = groups[(summary['operation'],summary['delay_ms'],summary['tables'],summary['phase'])]
        assert summary['n'] == len(values)
        assert summary['median_ms'] == statistics.median(values)
        assert summary['p95_ms'] == sorted(values)[math.ceil(.95*len(values))-1]
        assert summary['min_ms'] == min(values) and summary['max_ms'] == max(values)
    print(f'{len(samples)} samples and {len(summaries)} summaries verified')


if __name__ == '__main__':
    main()
