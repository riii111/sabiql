#!/usr/bin/env python3
"""Bounded CF-14 measurements against owned local PostgreSQL and MySQL fixtures."""
import argparse
from collections import defaultdict
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import select
import shutil
import signal
import socket
import statistics
import subprocess
import sys
import tempfile
import threading
import time
import uuid


DATABASES = ("postgres", "mysql")
TABLE_COUNTS = (10, 100, 1000)
DELAYS_MS = (0, 50, 200)
OPERATIONS = (
    "startup",
    "db_selection",
    "metadata_reload",
    "table_selection",
    "inspector",
    "preview",
    "page",
    "completion",
    "er_metadata",
)
REPETITIONS = 3
DEADLINE_SECONDS = 1200
CASE_TIMEOUT_SECONDS = 90
POLL_INTERVAL_SECONDS = 0.02
APP_USER = "cf14_app"
APP_PASSWORD = "cf14_app_password"
ADMIN_PASSWORD = "cf14_admin_password"


class BudgetReached(RuntimeError):
    pass


class FixtureUnavailable(RuntimeError):
    pass


def now_ns():
    return time.time_ns()


def docker(args, timeout=30, check=True):
    try:
        return subprocess.run(
            ["docker", *args],
            check=check,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
    except subprocess.CalledProcessError as error:
        detail = error.stderr.strip() or error.stdout.strip() or "no Docker error output"
        raise RuntimeError(detail) from error


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def source_manifest(root):
    paths = [
        "scripts/cf14/run.py",
        "scripts/cf14/stage2.py",
        "scripts/cf14/stage3.py",
        "src/tests/cf14_measurements.rs",
        "src/infra/examples/cf14_mysql.rs",
        "src/infra/examples/cf14_postgres.rs",
    ]
    return {path: {"bytes": (root / path).stat().st_size, "sha256": sha256(root / path)} for path in paths}


def fixture_sql(kind, table_count):
    names = ["items", *[f"t{i:04}" for i in range(1, table_count)]]
    if kind == "postgres":
        tables = "\n".join(
            f'CREATE TABLE "app"."{name}" (id INTEGER PRIMARY KEY, name TEXT NOT NULL);'
            for name in names
        )
        rows = ",\n".join(f"({i}, 'row-{i}')" for i in range(100))
        return f"""
CREATE USER {APP_USER} PASSWORD '{APP_PASSWORD}';
CREATE SCHEMA app AUTHORIZATION {APP_USER};
{tables}
INSERT INTO "app"."items" (id, name) VALUES {rows};
GRANT USAGE ON SCHEMA app TO {APP_USER};
GRANT SELECT ON ALL TABLES IN SCHEMA app TO {APP_USER};
""".strip() + "\n"
    tables = "\n".join(
        f"CREATE TABLE `app`.`{name}` (id INT PRIMARY KEY, name VARCHAR(64) NOT NULL);"
        for name in names
    )
    rows = ",\n".join(f"({i}, 'row-{i}')" for i in range(100))
    return f"""
{tables}
INSERT INTO `app`.`items` (id, name) VALUES {rows};
GRANT ALL PRIVILEGES ON `app`.* TO '{APP_USER}'@'%';
FLUSH PRIVILEGES;
""".strip() + "\n"


class Fixture:
    def __init__(self, kind, table_count, scratch, run_id):
        self.kind = kind
        self.table_count = table_count
        self.name = f"cf14-{run_id}-{kind}-{table_count}"
        self.network = f"{self.name}-network"
        self.volume = f"{self.name}-volume"
        self.scratch = scratch
        self.init_sql = scratch / f"{self.name}.sql"
        self.init_sql.write_text(fixture_sql(kind, table_count))
        self.port = None

    def start(self):
        label = f"com.sabiql.cf14.stage3={self.name}"
        docker(["network", "create", "--label", label, self.network])
        docker(["volume", "create", "--label", label, self.volume])
        if self.kind == "postgres":
            image = "postgres:16-alpine"
            env = [
                "-e",
                "POSTGRES_USER=postgres",
                "-e",
                f"POSTGRES_PASSWORD={ADMIN_PASSWORD}",
                "-e",
                "POSTGRES_DB=app",
            ]
            container_port = "5432"
        else:
            image = "mysql:8.4.10"
            env = [
                "-e",
                f"MYSQL_ROOT_PASSWORD={ADMIN_PASSWORD}",
                "-e",
                "MYSQL_DATABASE=app",
                "-e",
                f"MYSQL_USER={APP_USER}",
                "-e",
                f"MYSQL_PASSWORD={APP_PASSWORD}",
            ]
            container_port = "3306"
        docker(
            [
                "run",
                "--detach",
                "--name",
                self.name,
                "--network",
                self.network,
                "--label",
                label,
                "--publish",
                f"127.0.0.1::{container_port}",
                "--volume",
                f"{self.volume}:/var/lib/{'postgresql/data' if self.kind == 'postgres' else 'mysql'}",
                "--volume",
                f"{self.init_sql}:/docker-entrypoint-initdb.d/01-cf14.sql:ro",
                *env,
                image,
            ],
            timeout=60,
        )
        self.port = self.host_port(container_port)
        self.wait_ready()

    def host_port(self, container_port):
        result = docker(["port", self.name, f"{container_port}/tcp"])
        value = result.stdout.strip().splitlines()[0]
        return int(value.rsplit(":", 1)[1])

    def wait_ready(self):
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            if self.kind == "postgres":
                command = ["pg_isready", "-U", "postgres", "-d", "app"]
            else:
                command = [
                    "mysqladmin",
                    "ping",
                    "-h",
                    "127.0.0.1",
                    "-uroot",
                    f"-p{ADMIN_PASSWORD}",
                    "--silent",
                ]
            result = docker(["exec", self.name, *command], timeout=10, check=False)
            if result.returncode == 0:
                return
            time.sleep(1)
        raise TimeoutError(f"fixture did not become ready: {self.name}")

    def restart(self):
        docker(["restart", self.name], timeout=60)
        self.wait_ready()

    def dsn(self, proxy_port):
        if self.kind == "postgres":
            return f"postgres://{APP_USER}:{APP_PASSWORD}@127.0.0.1:{proxy_port}/app?sslmode=disable"
        return f"mysql://{APP_USER}:{APP_PASSWORD}@127.0.0.1:{proxy_port}/app?ssl-mode=DISABLED"

    def observer_config(self):
        if self.kind == "postgres":
            return {
                "program": shutil.which("psql"),
                "dsn": f"postgres://postgres@127.0.0.1:{self.port}/app",
                "query": "SELECT count(*) FILTER (WHERE usename = 'cf14_app'), count(*) FROM pg_stat_activity;",
            }
        return {
            "program": shutil.which("mysql"),
            "query": "SELECT (SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE USER = 'cf14_app'), (SELECT VARIABLE_VALUE FROM performance_schema.global_status WHERE VARIABLE_NAME = 'Threads_connected'), (SELECT VARIABLE_VALUE FROM performance_schema.global_status WHERE VARIABLE_NAME = 'Connections'), (SELECT VARIABLE_VALUE FROM performance_schema.global_status WHERE VARIABLE_NAME = 'Bytes_received'), (SELECT VARIABLE_VALUE FROM performance_schema.global_status WHERE VARIABLE_NAME = 'Bytes_sent');",
        }

    def cleanup(self):
        for args in (["rm", "-f", self.name], ["volume", "rm", self.volume], ["network", "rm", self.network]):
            docker(args, timeout=30, check=False)


class Observer:
    def __init__(self, fixture, output):
        self.fixture = fixture
        self.output = output
        self.events = []
        self.stop_event = threading.Event()
        self.thread = None
        self.process = None
        self.passfile = None

    def start(self):
        config = self.fixture.observer_config()
        if not config["program"]:
            raise RuntimeError(f"missing observer CLI for {self.fixture.kind}")
        if self.fixture.kind == "postgres":
            self.passfile = self.fixture.scratch / f"{self.output.name}.pgpass"
            self.passfile.write_text(
                f"127.0.0.1:{self.fixture.port}:app:postgres:{ADMIN_PASSWORD}\n"
            )
            self.passfile.chmod(0o600)
            command = [config["program"], config["dsn"], "-X", "-A", "-t", "-q", "-v", "ON_ERROR_STOP=1"]
            environment = dict(os.environ, PGPASSFILE=str(self.passfile))
        else:
            option_file = self.fixture.scratch / f"{self.output.name}.cnf"
            option_file.write_text(
                "[client]\n"
                f"host=127.0.0.1\nport={self.fixture.port}\nuser=root\n"
                f"password={ADMIN_PASSWORD}\ndatabase=app\n"
            )
            option_file.chmod(0o600)
            command = [
                config["program"],
                f"--defaults-extra-file={option_file}",
                "--batch",
                "--skip-column-names",
                "--raw",
                "--silent",
            ]
            environment = dict(os.environ)
        self.process = subprocess.Popen(
            command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
            env=environment,
        )
        self.thread = threading.Thread(target=self._poll, args=(config["query"],), daemon=True)
        self.thread.start()
        deadline = time.monotonic() + 10
        while not self.events and time.monotonic() < deadline:
            time.sleep(0.01)
        if not self.events:
            raise RuntimeError(f"observer did not produce a baseline: {self.fixture.kind}")

    def _poll(self, query):
        while not self.stop_event.is_set():
            try:
                self.process.stdin.write(query + "\n")
                self.process.stdin.flush()
                ready, _, _ = select.select([self.process.stdout], [], [], 0.5)
                if not ready:
                    continue
                line = self.process.stdout.readline().strip()
                if not line:
                    continue
                fields = line.split("|")
                event = {"ns": now_ns(), "raw": line}
                if self.fixture.kind == "postgres":
                    event.update(
                        active_app_sessions=int(fields[0]),
                        total_sessions=int(fields[1]),
                    )
                else:
                    names = [
                        "active_app_sessions",
                        "threads_connected",
                        "connections_total",
                        "bytes_received_status",
                        "bytes_sent_status",
                    ]
                    event.update({name: int(value) for name, value in zip(names, fields)})
                self.events.append(event)
            except (BrokenPipeError, OSError, ValueError, IndexError):
                return

    def stop(self):
        self.stop_event.set()
        if self.process is not None:
            try:
                self.process.stdin.close()
            except (OSError, ValueError):
                pass
            try:
                self.process.terminate()
                self.process.wait(timeout=3)
            except (OSError, subprocess.TimeoutExpired):
                self.process.kill()
        if self.thread is not None:
            self.thread.join(timeout=3)
        write_jsonl(self.output / "observer.jsonl", self.events)


def write_jsonl(path, events):
    with path.open("w") as stream:
        for event in events:
            stream.write(json.dumps(event, sort_keys=True) + "\n")


class ProcessMonitor:
    def __init__(self, output):
        self.output = output
        self.events = []
        self.stop_event = threading.Event()
        self.thread = None
        self.parent = None

    def start(self, parent):
        self.parent = parent
        self.thread = threading.Thread(target=self._poll, daemon=True)
        self.thread.start()

    def _poll(self):
        while not self.stop_event.is_set():
            processes = self.descendants(self.parent)
            self.events.append({"ns": now_ns(), "count": len(processes), "pids": processes})
            time.sleep(POLL_INTERVAL_SECONDS)

    @staticmethod
    def descendants(parent):
        result = subprocess.run(
            ["ps", "-axo", "pid=,ppid="], capture_output=True, text=True, check=False
        )
        children = defaultdict(list)
        for line in result.stdout.splitlines():
            fields = line.split()
            if len(fields) == 2:
                children[int(fields[1])].append(int(fields[0]))
        found = []
        pending = [parent]
        while pending:
            current = pending.pop()
            for child in children[current]:
                found.append(child)
                pending.append(child)
        return found

    def stop(self):
        self.stop_event.set()
        if self.thread is not None:
            self.thread.join(timeout=3)
        write_jsonl(self.output / "processes.jsonl", self.events)


class TcpProxy:
    def __init__(self, kind, target_port, delay_ms, output):
        self.kind = kind
        self.target_port = target_port
        self.delay_seconds = delay_ms / 2000
        self.output = output
        self.events = []
        self.lock = threading.Lock()
        self.stop_event = threading.Event()
        self.listener = None
        self.thread = None
        self.port = None
        self.connection_id = 0

    def start(self):
        self.listener = socket.socket()
        self.listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.listener.bind(("127.0.0.1", 0))
        self.listener.listen()
        self.listener.settimeout(0.2)
        self.port = self.listener.getsockname()[1]
        self.thread = threading.Thread(target=self._accept, daemon=True)
        self.thread.start()

    def _append(self, event):
        with self.lock:
            self.events.append(event)

    def _accept(self):
        while not self.stop_event.is_set():
            try:
                client, _ = self.listener.accept()
            except socket.timeout:
                continue
            with self.lock:
                self.connection_id += 1
                connection_id = self.connection_id
            threading.Thread(target=self._forward, args=(client, connection_id), daemon=True).start()

    def _forward(self, client, connection_id):
        started = now_ns()
        self._append({"kind": "connection_start", "connection": connection_id, "ns": started})
        try:
            server = socket.create_connection(("127.0.0.1", self.target_port), timeout=10)
            client.setblocking(False)
            server.setblocking(False)
            buffers = {client: bytearray(), server: bytearray()}
            protocol_state = {"postgres_startup_done": False}
            sockets = [client, server]
            while sockets and not self.stop_event.is_set():
                readable, _, _ = select.select(sockets, [], [], 0.2)
                for source in readable:
                    try:
                        data = source.recv(65536)
                    except BlockingIOError:
                        continue
                    if not data:
                        sockets.remove(source)
                        continue
                    destination = server if source is client else client
                    direction = "client_to_server" if source is client else "server_to_client"
                    sql_messages = self.sql_messages(
                        buffers[source], data, direction, protocol_state
                    )
                    if self.delay_seconds:
                        time.sleep(self.delay_seconds)
                    destination.sendall(data)
                    self._append(
                        {
                            "kind": "traffic",
                            "connection": connection_id,
                            "direction": direction,
                            "bytes": len(data),
                            "sql_messages": sql_messages,
                            "ns": now_ns(),
                        }
                    )
            server.close()
        except (OSError, TimeoutError):
            pass
        finally:
            client.close()
            self._append({"kind": "connection_end", "connection": connection_id, "ns": now_ns()})

    def sql_messages(self, buffer, data, direction, protocol_state):
        if direction != "client_to_server":
            return 0
        buffer.extend(data)
        count = 0
        if self.kind == "mysql":
            while len(buffer) >= 4:
                length = int.from_bytes(buffer[:3], "little")
                if len(buffer) < 4 + length:
                    break
                payload = buffer[4 : 4 + length]
                del buffer[: 4 + length]
                if payload and payload[0] in (3, 22, 23, 24, 25):
                    count += 1
        else:
            if not protocol_state["postgres_startup_done"]:
                while len(buffer) >= 4:
                    startup_length = int.from_bytes(buffer[:4], "big")
                    if startup_length < 8 or len(buffer) < startup_length:
                        break
                    del buffer[:startup_length]
                    protocol_state["postgres_startup_done"] = True
                    break
                if not protocol_state["postgres_startup_done"]:
                    return 0
            while len(buffer) >= 5:
                length = int.from_bytes(buffer[1:5], "big")
                if length < 4 or len(buffer) < 1 + length:
                    break
                message = buffer[0]
                del buffer[: 1 + length]
                if message in (ord("Q"), ord("P")):
                    count += 1
        return count

    def stop(self):
        self.stop_event.set()
        if self.listener is not None:
            self.listener.close()
        if self.thread is not None:
            self.thread.join(timeout=3)
        write_jsonl(self.output / "proxy.jsonl", self.events)


def write_cli_wrappers(directory, cli_paths, log_path):
    wrapper = directory / "cli-wrapper.py"
    wrapper.write_text(
        "#!/usr/bin/env python3\n"
        "import json, os, subprocess, sys, time\n"
        "def record(value):\n"
        "    with open(os.environ['CF14_CLI_LOG'], 'a') as stream:\n"
        "        stream.write(json.dumps(value) + '\\n')\n"
        "program = os.environ['CF14_REAL_CLI']\n"
        "start = time.time_ns()\n"
        "record({'kind': 'start', 'program': os.environ['CF14_CLI_KIND'], 'pid': os.getpid(), 'ns': start, 'argv_count': len(sys.argv) - 1})\n"
        "child = subprocess.Popen([program, *sys.argv[1:]])\n"
        "record({'kind': 'child_start', 'program': os.environ['CF14_CLI_KIND'], 'pid': child.pid, 'wrapper_pid': os.getpid(), 'ns': time.time_ns()})\n"
        "returncode = child.wait()\n"
        "record({'kind': 'end', 'program': os.environ['CF14_CLI_KIND'], 'pid': os.getpid(), 'child_pid': child.pid, 'ns': time.time_ns(), 'returncode': returncode})\n"
        "sys.exit(returncode)\n"
    )
    wrapper.chmod(0o700)
    for kind, program in cli_paths.items():
        link = directory / kind
        link.symlink_to(wrapper)
    return {"PATH": str(directory), "CF14_CLI_LOG": str(log_path), "wrapper": str(wrapper)}


def phase_events(events, start, end):
    return [event for event in events if start <= event.get("ns", 0) <= end]


def cli_metrics(events, start, end):
    starts = [
        event
        for event in events
        if event["kind"] == "child_start" and event["ns"] <= end
    ]
    ends = {
        event.get("child_pid"): event["ns"]
        for event in events
        if event["kind"] == "end"
    }
    relevant = [event for event in starts if event["ns"] >= start]
    intervals = []
    for event in starts:
        finish = ends.get(event["pid"], end)
        if event["ns"] <= end and finish >= start:
            intervals.append((max(start, event["ns"]), min(end, finish)))
    boundaries = sorted({point for interval in intervals for point in interval})
    max_parallel = 0
    for point in boundaries:
        max_parallel = max(
            max_parallel,
            sum(begin <= point < finish for begin, finish in intervals),
        )
    return {
        "cli_processes_started": len(relevant),
        "max_cli_processes": max_parallel,
        "cli_process_pids": [event["pid"] for event in relevant],
    }


def observer_metrics(events, start, end):
    before = [event for event in events if event["ns"] <= start]
    during = phase_events(events, start, end)
    after = [event for event in events if event["ns"] <= end]
    candidates = before[-1:] + during + after[-1:]
    if not candidates:
        return {"server_observation": "unmeasured: observer returned no samples"}
    result = {
        "server_active_app_sessions_max": max(
            event["active_app_sessions"] for event in candidates if "active_app_sessions" in event
        ),
        "server_observation_samples": len(during),
    }
    if "connections_total" in candidates[0]:
        result["server_connections_counter_delta"] = (
            after[-1].get("connections_total", candidates[-1].get("connections_total", 0))
            - before[-1].get("connections_total", candidates[0].get("connections_total", 0))
            if before
            else None
        )
        result["server_threads_connected_max"] = max(
            event["threads_connected"] for event in candidates
        )
    else:
        result["server_total_sessions_max"] = max(
            event["total_sessions"] for event in candidates
        )
    return result


def sample_case(case, fixture, binary, output_root, deadline):
    case_name = "-".join(str(case[key]) for key in ("database", "tables", "delay_ms", "operation", "repetition"))
    case_dir = output_root / "raw" / case_name
    case_dir.mkdir(parents=True, exist_ok=False)
    fixture.restart()
    proxy = TcpProxy(fixture.kind, fixture.port, case["delay_ms"], case_dir)
    proxy.start()
    observer = Observer(fixture, case_dir)
    observer.start()
    cli_log = case_dir / "cli.jsonl"
    wrapper_dir = case_dir / "wrappers"
    wrapper_dir.mkdir()
    real_cli = shutil.which("mysql" if fixture.kind == "mysql" else "psql")
    wrapper_env = write_cli_wrappers(
        wrapper_dir,
        {"mysql" if fixture.kind == "mysql" else "psql": real_cli},
        cli_log,
    )
    environment = dict(
        os.environ,
        PATH=wrapper_env["PATH"] + os.pathsep + os.environ.get("PATH", ""),
        CF14_DSN=fixture.dsn(proxy.port),
        CF14_CLI_LOG=wrapper_env["CF14_CLI_LOG"],
        CF14_REAL_CLI=real_cli,
        CF14_CLI_KIND=fixture.kind,
    )
    monitor = ProcessMonitor(case_dir)
    try:
        process = subprocess.Popen(
            [str(binary), case["operation"]],
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            start_new_session=True,
        )
        monitor.start(process.pid)
        remaining = max(1, min(CASE_TIMEOUT_SECONDS, int(deadline - time.monotonic())))
        try:
            stdout, stderr = process.communicate(timeout=remaining)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            stdout, stderr = process.communicate()
            raise RuntimeError(f"case timed out after {CASE_TIMEOUT_SECONDS}s: {case_name}")
    finally:
        monitor.stop()
        observer.stop()
        proxy.stop()
    write_json(case_dir / "app.json", {"returncode": process.returncode, "stdout": stdout, "stderr": stderr})
    cli_events = [json.loads(line) for line in cli_log.read_text().splitlines()] if cli_log.exists() else []
    app_samples = [json.loads(line) for line in stdout.splitlines() if line.strip()]
    if process.returncode != 0 or len(app_samples) != 2:
        raise RuntimeError(f"case failed: {case_name}: {stderr[-1000:]}")
    proxy_events = proxy.events
    observer_events = observer.events
    process_events = monitor.events
    samples = []
    for phase_index, app_sample in enumerate(app_samples):
        start = app_sample["start_ns"]
        end = app_sample["end_ns"]
        traffic = phase_events(proxy_events, start, end)
        server = observer_metrics(observer_events, start, end)
        cli = cli_metrics(cli_events, start, end)
        process_slice = phase_events(process_events, start, end)
        sample = {
            **case,
            "phase": app_sample["phase"],
            "cache_stage": "cold-after-container-restart" if phase_index == 0 else "warm-followup",
            "elapsed_ms": app_sample["elapsed_ms"],
            "error": app_sample["error"],
            "sql_count": sum(event.get("sql_messages", 0) for event in traffic),
            "wire_bytes_client_to_server": sum(
                event["bytes"] for event in traffic if event.get("direction") == "client_to_server"
            ),
            "wire_bytes_server_to_client": sum(
                event["bytes"] for event in traffic if event.get("direction") == "server_to_client"
            ),
            "proxy_connections": len(
                [event for event in traffic if event.get("kind") == "connection_start"]
            ),
            "rtt_target_ms": case["delay_ms"],
            "terminal_display_ms": None,
            "terminal_display_observed": False,
            **cli,
            **server,
            "max_process_descendants_observed": max(
                (event["count"] for event in process_slice), default=None
            ),
        }
        samples.append(sample)
    write_json(case_dir / "samples.json", samples)
    return samples


def percentile(values, percentile):
    return sorted(values)[math.ceil(percentile * len(values)) - 1]


def summarize(samples):
    groups = defaultdict(list)
    for sample in samples:
        key = tuple(sample[field] for field in ("database", "tables", "delay_ms", "operation", "cache_stage"))
        groups[key].append(sample)
    result = []
    for key, values in sorted(groups.items()):
        elapsed = [value["elapsed_ms"] for value in values]
        row = dict(zip(("database", "tables", "delay_ms", "operation", "cache_stage"), key))
        row.update(
            n=len(values),
            elapsed_median_ms=statistics.median(elapsed),
            elapsed_p95_ms=percentile(elapsed, 0.95),
            sql_count_median=statistics.median(value["sql_count"] for value in values),
            client_to_server_bytes_median=statistics.median(
                value["wire_bytes_client_to_server"] for value in values
            ),
            server_to_client_bytes_median=statistics.median(
                value["wire_bytes_server_to_client"] for value in values
            ),
            cli_processes_median=statistics.median(
                value["cli_processes_started"] for value in values
            ),
            max_cli_processes=max(value["max_cli_processes"] for value in values),
            max_server_sessions=max(value["server_active_app_sessions_max"] for value in values),
        )
        result.append(row)
    return result


def build_report(manifest, samples, summary, status, reason):
    lines = [
        "# CF-14 stage 3 measurement",
        "",
        f"status: {status}",
        f"base: `{manifest['base']}`",
        f"budget: repetitions={manifest['repetitions']}, tables={manifest['tables']}, rtt_proxy_ms={manifest['rtt_proxy_ms']}, deadline_seconds={manifest['deadline_seconds']}",
        "",
        "Cold is the first phase after a dedicated DB container restart; warm is the second phase in the same Rust harness. The proxy delays each direction by half the requested RTT and counts cleartext protocol messages and wire bytes. PostgreSQL and MySQL sessions are observed from a separate administrative connection.",
        "",
        "## Observed limits",
        "",
        "- `terminal_display_ms` is null: no real terminal rendering was exercised.",
        "- `max_process_descendants_observed` is an OS sampling aid; CLI counts use wrapper child start/end events.",
        "- Results are local-only and are not a production performance claim.",
    ]
    if reason:
        lines.extend([f"- partial/blocker: {reason}"])
    lines.extend(["", "## Summary", "", "| DB | tables | RTT ms | operation | cache | n | elapsed median/p95 ms | SQL median | wire C>S/S>C bytes | CLI median/max | DB sessions max |", "|---|---:|---:|---|---|---:|---:|---:|---:|---:|---:|"])
    for row in summary:
        lines.append(
            f"| {row['database']} | {row['tables']} | {row['delay_ms']} | {row['operation']} | {row['cache_stage']} | {row['n']} | {row['elapsed_median_ms']:.3f}/{row['elapsed_p95_ms']:.3f} | {row['sql_count_median']:.0f} | {row['client_to_server_bytes_median']:.0f}/{row['server_to_client_bytes_median']:.0f} | {row['cli_processes_median']:.0f}/{row['max_cli_processes']} | {row['max_server_sessions']} |"
        )
    lines.extend(
        [
            "",
            "## Measurement interpretation",
            "",
            "The retained structure is the observed CLI-per-operation boundary: the SQL count and wire bytes are sums of messages crossing the local proxy, while DB session maxima are independently observed server-side. `table_selection` can show concurrent CLI activity because its detail and preview effects are joined. Any next optimization should be selected from these measured cells; this stage intentionally contains no production performance change.",
        ]
    )
    return "\n".join(lines) + "\n"


def file_manifest(output):
    entries = []
    for path in sorted(output.rglob("*")):
        if path.is_file() and path.name not in {"file-manifest.json", "manifest-check.json"}:
            entries.append(
                {"path": str(path.relative_to(output)), "bytes": path.stat().st_size, "sha256": sha256(path)}
            )
    result = {
        "scope": "all generated measurement files except manifest control files",
        "files": entries,
        "file_count": len(entries),
        "bytes": sum(entry["bytes"] for entry in entries),
    }
    write_json(output / "file-manifest.json", result)
    checked = []
    for entry in entries:
        path = output / entry["path"]
        checked.append(path.stat().st_size == entry["bytes"] and sha256(path) == entry["sha256"])
    if not all(checked):
        raise RuntimeError("file manifest verification failed")
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--mysql-binary", type=Path, required=True)
    parser.add_argument("--postgres-binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    output = args.output.resolve()
    relative_output = output.relative_to(root) if output == root or root in output.parents else None
    allowed_in_checkout = relative_output is not None and relative_output.parts[:3] == (
        "memo",
        "cf14-measurements-2026-09-07",
        "stage3",
    )
    if relative_output is not None and not allowed_in_checkout:
        raise RuntimeError("measurement output must be outside the repository")
    output.mkdir(parents=True, exist_ok=False)
    run_id = f"{root.name}-{os.getpid()}-{uuid.uuid4().hex[:8]}"
    scratch = Path(tempfile.mkdtemp(prefix="cf14-stage3-"))
    source_hashes = source_manifest(root)
    manifest = {
        "base": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
        "platform": platform.platform(),
        "python": sys.version,
        "run_id": run_id,
        "repetitions": REPETITIONS,
        "tables": list(TABLE_COUNTS),
        "rtt_proxy_ms": list(DELAYS_MS),
        "operations": list(OPERATIONS),
        "deadline_seconds": DEADLINE_SECONDS,
        "case_timeout_seconds": CASE_TIMEOUT_SECONDS,
        "source_hashes": source_hashes,
        "binaries": {
            "mysql": {"path": str(args.mysql_binary.resolve()), "sha256": sha256(args.mysql_binary.resolve())},
            "postgres": {"path": str(args.postgres_binary.resolve()), "sha256": sha256(args.postgres_binary.resolve())},
        },
        "terminal_display_ms": None,
    }
    write_json(output / "manifest.json", manifest)
    write_json(output / "environment.json", {"platform": platform.platform(), "python": sys.version, "base": manifest["base"]})
    samples = []
    status = "complete"
    reason = None
    deadline = time.monotonic() + DEADLINE_SECONDS
    fixtures = []
    try:
        for database in DATABASES:
            binary = args.postgres_binary if database == "postgres" else args.mysql_binary
            for tables in TABLE_COUNTS:
                fixture = Fixture(database, tables, scratch, run_id)
                fixtures.append(fixture)
                try:
                    fixture.start()
                except Exception as error:
                    raise FixtureUnavailable(
                        f"{database}/{tables}: dedicated Docker fixture unavailable: {error}"
                    ) from error
                try:
                    for delay_ms in DELAYS_MS:
                        for operation in OPERATIONS:
                            for repetition in range(REPETITIONS):
                                if time.monotonic() >= deadline:
                                    raise BudgetReached("deadline reached; completed cells retained")
                                case = {
                                    "database": database,
                                    "tables": tables,
                                    "delay_ms": delay_ms,
                                    "operation": operation,
                                    "repetition": repetition,
                                }
                                samples.extend(sample_case(case, fixture, binary, output, deadline))
                                with (output / "samples.jsonl").open("a") as stream:
                                    for sample in samples[-2:]:
                                        stream.write(json.dumps(sample, sort_keys=True) + "\n")
                                print(" ".join(f"{key}={value}" for key, value in case.items()), flush=True)
                finally:
                    fixture.cleanup()
                    fixtures.remove(fixture)
    except (BudgetReached, FixtureUnavailable) as error:
        status = "partial"
        reason = str(error)
    except Exception as error:
        status = "failed"
        reason = str(error)
    finally:
        for fixture in fixtures:
            fixture.cleanup()
        shutil.rmtree(scratch, ignore_errors=True)
    summary = summarize(samples)
    manifest.update(status=status, reason=reason, samples=len(samples), summary_cells=len(summary))
    write_json(output / "summary.json", summary)
    (output / "report.md").write_text(build_report(manifest, samples, summary, status, reason))
    write_json(output / "manifest.json", manifest)
    checked_manifest = file_manifest(output)
    write_json(output / "manifest-check.json", {"verified": True, **checked_manifest})
    print(json.dumps({"status": status, "samples": len(samples), "output": str(output), "reason": reason}))
    if status == "failed":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
