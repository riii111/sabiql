#!/usr/bin/env python3
"""Own loopback peer, never a DB server. Observe adapter timeout/cancel cleanup."""
import argparse
import json
import os
import signal
from pathlib import Path
import socket
import subprocess
import tempfile
import threading
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    root = Path(__file__).resolve().parents[2]
    for mode, operation in [('disconnect', 'startup'), ('probe_timeout', 'startup'), ('metadata_timeout', 'metadata_reload'), ('cancel', 'cancel')]:
        events = []
        stopping = threading.Event()
        clients = []

        def serve(peer):
            with peer:
                events.append(dict(kind='accept', ns=time.time_ns()))
                if mode == 'disconnect':
                    return
                peer.settimeout(.1)
                while not stopping.is_set():
                    try:
                        if not peer.recv(1):
                            events.append(dict(kind='eof', ns=time.time_ns()))
                            return
                    except socket.timeout:
                        pass

        with socket.socket() as listener, tempfile.TemporaryDirectory(prefix='cf14-fault-') as scratch:
            listener.bind(('127.0.0.1', 0))
            listener.listen()
            listener.settimeout(.1)

            def accept():
                while not stopping.is_set():
                    try:
                        peer, _ = listener.accept()
                    except socket.timeout:
                        continue
                    thread = threading.Thread(target=serve, args=(peer,))
                    clients.append(thread)
                    thread.start()

            thread = threading.Thread(target=accept)
            thread.start()
            log = args.output / (mode + '-cli.jsonl')
            env = dict(os.environ, PATH=str(root/'scripts/cf14') + os.pathsep + os.environ['PATH'],
                       CF14_LOG=str(log.resolve()), CF14_TABLES='10', CF14_DELAY_MS='0',
                       CF14_FAULT_PORT=str(listener.getsockname()[1]), TMPDIR=scratch)
            try:
                with subprocess.Popen([str(args.binary.resolve()), operation], env=env, stdout=subprocess.PIPE,
                                      stderr=subprocess.PIPE, text=True, start_new_session=True) as child:
                    try:
                        stdout, stderr = child.communicate(timeout=75)
                    except BaseException:
                        os.killpg(child.pid, signal.SIGKILL)
                        child.wait()
                        raise
                    run = subprocess.CompletedProcess(child.args, child.returncode, stdout, stderr)
                time.sleep(.5)
            finally:
                stopping.set()
                thread.join()
                for client in clients:
                    client.join()
            cli = [json.loads(line) for line in log.read_text().splitlines()]
            residual = []
            for pid in {event['pid'] for event in cli if event['kind']=='start'}:
                try:
                    os.kill(pid, 0)
                    residual.append(pid)
                except ProcessLookupError:
                    pass
            result = dict(mode=mode, operation=operation, samples=[json.loads(line) for line in run.stdout.splitlines()],
                          returncode=run.returncode, stderr=run.stderr, loopback_events=events,
                          loopback_accepts=sum(event['kind']=='accept' for event in events),
                          db_connections=None, residual_pids_after_500ms=residual,
                          stage='synthetic peer holds first XML response' if mode!='disconnect' else 'synthetic peer closes before XML response')
            (args.output/(mode+'.json')).write_text(json.dumps(result, indent=2)+'\n')
            if run.returncode or residual or not events:
                raise RuntimeError(result)
            if mode != 'cancel' and any(not sample['error'] for sample in result['samples']):
                raise RuntimeError('expected failure')
            print(mode, flush=True)


if __name__ == '__main__':
    main()
