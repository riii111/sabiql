#!/usr/bin/env bash
set -Eeuo pipefail

script_dir="$(cd "$(dirname "$0")" && pwd)"
readonly script_dir
test_root="$(mktemp -d "${TMPDIR:-/tmp}/sabiql-mysql-cleanup.XXXXXX")"
readonly test_root
fake_bin="$test_root/bin"
readonly fake_bin
state_file="$test_root/containers"
readonly state_file
label_log="$test_root/labels.log"
readonly label_log
compose_log="$test_root/compose.log"
readonly compose_log
cargo_log="$test_root/cargo.log"
readonly cargo_log
rm_log="$test_root/rm.log"
readonly rm_log
mkdir -p -- "$fake_bin"
: >"$state_file"
: >"$label_log"
: >"$compose_log"
: >"$cargo_log"
: >"$rm_log"

cleanup() {
    rm -rf -- "$test_root"
}

trap cleanup EXIT

cat >"$fake_bin/docker" <<'EOF'
#!/usr/bin/env bash
set -Eeuo pipefail

state_file="${FAKE_DOCKER_STATE:?}"
label_log="${FAKE_DOCKER_LABEL_LOG:?}"
compose_log="${FAKE_DOCKER_COMPOSE_LOG:?}"
rm_log="${FAKE_DOCKER_RM_LOG:?}"
state_lock="${state_file}.lock"
while ! mkdir "$state_lock" 2>/dev/null; do
    sleep 0.01
done
trap 'rmdir "$state_lock"' EXIT

case "${1:-}" in
    compose)
        project='default'
        command=''
        while (($# > 0)); do
            case "$1" in
                --project-name)
                    project="$2"
                    shift 2
                    ;;
                --file)
                    shift 2
                    ;;
                up|port|down|rm)
                    command="$1"
                    break
                    ;;
                *)
                    shift
                    ;;
            esac
        done
        if [[ ! "$project" =~ ^[a-z0-9][a-z0-9_-]*$ ]]; then
            printf 'invalid fake Compose project name: %s\n' "$project" >&2
            exit 1
        fi
        printf '%s|%s|%s\n' "$project" "$command" "$*" >>"$compose_log"
        if [[ "$command" == port ]]; then
            port="$(printf '%s' "$project" | cksum | awk '{ print 30000 + ($1 % 20000) }')"
            printf '127.0.0.1:%s\n' "$port"
        fi
        exit 0
        ;;
    ps)
        filter=''
        for argument in "$@"; do
            case "$argument" in
                label=*)
                    filter="${argument#label=}"
                    ;;
            esac
        done
        while IFS='|' read -r container_id label; do
            if [[ -n "$container_id" && "$label" == "$filter" ]]; then
                printf '%s\n' "$container_id"
            fi
        done <"$state_file"
        ;;
    rm)
        printf '%s\n' "$*" >>"$rm_log"
        if [[ "${FAKE_DOCKER_RM_STATUS:-0}" != 0 ]]; then
            exit "$FAKE_DOCKER_RM_STATUS"
        fi
        temporary_state="$state_file.tmp"
        : >"$temporary_state"
        while IFS='|' read -r container_id label; do
            keep=1
            for argument in "$@"; do
                if [[ "$argument" == "$container_id" ]]; then
                    keep=0
                fi
            done
            if ((keep)); then
                printf '%s|%s\n' "$container_id" "$label" >>"$temporary_state"
            fi
        done <"$state_file"
        mv -- "$temporary_state" "$state_file"
        ;;
    run)
        label=''
        for ((index = 1; index < $#; index++)); do
            if [[ "${!index}" == '--label' ]]; then
                next_index=$((index + 1))
                label="${!next_index}"
            fi
        done
        sequence_file="$state_file.sequence"
        sequence=0
        if [[ -f "$sequence_file" ]]; then
            sequence="$(<"$sequence_file")"
        fi
        sequence=$((sequence + 1))
        printf '%s\n' "$sequence" >"$sequence_file"
        printf 'fake-container-%s|%s\n' "$sequence" "$label" >>"$state_file"
        printf '%s\n' "$label" >>"$label_log"
        case " $* " in
            *' --version '*)
                printf '%s\n' 'mysql  Ver 8.4.10 for Linux on aarch64 (MySQL Community Server - GPL)'
                ;;
            *' --execute=SELECT VERSION() '*)
                printf '%s\n' '8.4.10'
                ;;
        esac
        ;;
    *)
        printf 'unexpected fake docker command: %s\n' "$*" >&2
        exit 1
        ;;
esac
EOF

cat >"$fake_bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -Eeuo pipefail

case "${FAKE_CARGO_MODE:-success}" in
    success)
        printf '%s|%s|%s\n' "${SABIQL_MYSQL_RUN_LABEL:-}" \
            "${SABIQL_MYSQL_TEST_PORT:-}" "${CARGO_TARGET_DIR:-}" >>"${FAKE_CARGO_LOG:?}"
        exit 0
        ;;
    failure|timeout)
        printf '%s|%s|%s\n' "${SABIQL_MYSQL_RUN_LABEL:-}" \
            "${SABIQL_MYSQL_TEST_PORT:-}" "${CARGO_TARGET_DIR:-}" >>"${FAKE_CARGO_LOG:?}"
        exit "${FAKE_CARGO_STATUS:-17}"
        ;;
    signal)
        printf '%s|%s|%s\n' "${SABIQL_MYSQL_RUN_LABEL:-}" \
            "${SABIQL_MYSQL_TEST_PORT:-}" "${CARGO_TARGET_DIR:-}" >>"${FAKE_CARGO_LOG:?}"
        : >"${FAKE_CARGO_MARKER:?}"
        while [[ ! -e "${FAKE_CARGO_RELEASE:?}" ]]; do
            sleep 0.05
        done
        ;;
    *)
        printf 'unexpected fake cargo mode: %s\n' "$FAKE_CARGO_MODE" >&2
        exit 1
        ;;
esac
EOF

cat >"$fake_bin/openssl" <<'EOF'
#!/usr/bin/env bash
set -Eeuo pipefail

for ((index = 1; index < $#; index++)); do
    if [[ "${!index}" == '-out' ]]; then
        next_index=$((index + 1))
        : >"${!next_index}"
    fi
done
EOF

chmod +x "$fake_bin/docker" "$fake_bin/cargo" "$fake_bin/openssl"

export FAKE_DOCKER_STATE="$state_file"
export FAKE_DOCKER_LABEL_LOG="$label_log"
export FAKE_DOCKER_COMPOSE_LOG="$compose_log"
export FAKE_DOCKER_RM_LOG="$rm_log"
export FAKE_CARGO_LOG="$cargo_log"

assert_only_unrelated_container_remains() {
    local unrelated_count
    local run_count
    unrelated_count="$(awk -F'|' '$2 == "com.sabiql.mysql.integration=unrelated" { count++ } END { print count + 0 }' "$state_file")"
    run_count="$(awk -F'|' '$2 ~ /^com\.sabiql\.mysql\.integration=run-/ { count++ } END { print count + 0 }' "$state_file")"
    if [[ "$unrelated_count" != 1 || "$run_count" != 0 ]]; then
        printf 'unexpected fake container state:\n%s\n' "$(<"$state_file")" >&2
        return 1
    fi
}

run_case() {
    local name="$1"
    local expected_status="$2"
    local output_file="$test_root/$name.out"
    local actual_status

    if FAKE_CARGO_MODE="$name" FAKE_CARGO_STATUS="$expected_status" \
        PATH="$fake_bin:$PATH" "$script_dir/mysql_integration.sh" test \
        >"$output_file" 2>&1; then
        actual_status=0
    else
        actual_status=$?
    fi
    if [[ "$actual_status" != "$expected_status" ]]; then
        printf '%s exited %s, expected %s\n%s\n' \
            "$name" "$actual_status" "$expected_status" "$(<"$output_file")" >&2
        return 1
    fi
    assert_only_unrelated_container_remains
}

"$fake_bin/docker" run --label com.sabiql.mysql.integration=unrelated mysql:8.4.10 mysql --version \
    >/dev/null

run_case success 0
run_case failure 17
run_case timeout 124

signal_marker="$test_root/signal.marker"
signal_release="$test_root/signal.release"
signal_output="$test_root/signal.out"
signal_status=0
FAKE_CARGO_MODE=signal \
FAKE_CARGO_MARKER="$signal_marker" \
FAKE_CARGO_RELEASE="$signal_release" \
PATH="$fake_bin:$PATH" "$script_dir/mysql_integration.sh" test \
    >"$signal_output" 2>&1 &
integration_pid=$!
for _ in {1..100}; do
    if [[ -e "$signal_marker" ]]; then
        break
    fi
    sleep 0.05
done
if [[ ! -e "$signal_marker" ]]; then
    printf 'signal test did not reach fake cargo\n%s\n' "$(<"$signal_output")" >&2
    kill "$integration_pid" 2>/dev/null || true
    exit 1
fi
kill -TERM "$integration_pid"
: >"$signal_release"
if wait "$integration_pid"; then
    signal_status=0
else
    signal_status=$?
fi
if [[ "$signal_status" != 143 ]]; then
    printf 'signal exited %s, expected 143\n%s\n' "$signal_status" "$(<"$signal_output")" >&2
    exit 1
fi
assert_only_unrelated_container_remains

same_suffix_root="$test_root/other-worktree"
mkdir -p -- "$same_suffix_root/scripts"
cp -- "$script_dir/mysql_integration.sh" "$same_suffix_root/scripts/mysql_integration.sh"
cp -- "$script_dir/mysql-docker-cli.sh" "$same_suffix_root/scripts/mysql-docker-cli.sh"

PATH="$fake_bin:$PATH" "$script_dir/mysql_integration.sh" test >/dev/null 2>&1
PATH="$fake_bin:$PATH" "$same_suffix_root/scripts/mysql_integration.sh" test >/dev/null 2>&1
same_suffix_labels="$(awk '/^com\.sabiql\.mysql\.integration=run-/ { labels[$0] = 1 } END { print length(labels) + 0 }' "$label_log")"
if [[ "$same_suffix_labels" != 6 ]]; then
    printf 'expected distinct labels for same suffix across worktrees, got %s\n%s\n' \
        "$same_suffix_labels" "$(<"$label_log")" >&2
    exit 1
fi
assert_only_unrelated_container_remains

shared_target="$test_root/shared-cargo-target"
shared_target_start="$(wc -l <"$cargo_log")"
CARGO_TARGET_DIR="$shared_target" PATH="$fake_bin:$PATH" \
    "$script_dir/mysql_integration.sh" test >/dev/null 2>&1
shared_target_row="$(tail -n +$((shared_target_start + 1)) "$cargo_log")"
if [[ "$(printf '%s\n' "$shared_target_row" | awk -F'|' 'NF >= 3 { print $3; exit }')" != "$shared_target" ]]; then
    printf 'configured cargo target was not preserved:\n%s\n' "$shared_target_row" >&2
    exit 1
fi
assert_only_unrelated_container_remains

parallel_start="$(wc -l <"$cargo_log")"
parallel_one_marker="$test_root/parallel-one.marker"
parallel_one_release="$test_root/parallel-one.release"
parallel_two_marker="$test_root/parallel-two.marker"
parallel_two_release="$test_root/parallel-two.release"
parallel_one_output="$test_root/parallel-one.out"
parallel_two_output="$test_root/parallel-two.out"
FAKE_CARGO_MODE=signal \
FAKE_CARGO_MARKER="$parallel_one_marker" \
FAKE_CARGO_RELEASE="$parallel_one_release" \
PATH="$fake_bin:$PATH" "$script_dir/mysql_integration.sh" test \
    >"$parallel_one_output" 2>&1 &
parallel_one_pid=$!
FAKE_CARGO_MODE=signal \
FAKE_CARGO_MARKER="$parallel_two_marker" \
FAKE_CARGO_RELEASE="$parallel_two_release" \
PATH="$fake_bin:$PATH" "$same_suffix_root/scripts/mysql_integration.sh" test \
    >"$parallel_two_output" 2>&1 &
parallel_two_pid=$!
for _ in {1..100}; do
    if [[ -e "$parallel_one_marker" && -e "$parallel_two_marker" ]]; then
        break
    fi
    sleep 0.05
done
if [[ ! -e "$parallel_one_marker" || ! -e "$parallel_two_marker" ]]; then
    printf 'parallel test did not reach fake cargo\n%s\n%s\n' \
        "$(<"$parallel_one_output")" "$(<"$parallel_two_output")" >&2
    kill "$parallel_one_pid" "$parallel_two_pid" 2>/dev/null || true
    exit 1
fi
kill -TERM "$parallel_one_pid" "$parallel_two_pid"
: >"$parallel_one_release"
: >"$parallel_two_release"
parallel_one_status=0
parallel_two_status=0
if wait "$parallel_one_pid"; then
    parallel_one_status=0
else
    parallel_one_status=$?
fi
if wait "$parallel_two_pid"; then
    parallel_two_status=0
else
    parallel_two_status=$?
fi
if [[ "$parallel_one_status" != 143 || "$parallel_two_status" != 143 ]]; then
    printf 'parallel tests exited %s and %s, expected 143\n%s\n%s\n' \
        "$parallel_one_status" "$parallel_two_status" \
        "$(<"$parallel_one_output")" "$(<"$parallel_two_output")" >&2
    exit 1
fi
parallel_rows="$(tail -n +$((parallel_start + 1)) "$cargo_log")"
parallel_label_count="$(printf '%s\n' "$parallel_rows" | awk -F'|' 'NF >= 3 { labels[$1] = 1 } END { print length(labels) + 0 }')"
parallel_port_count="$(printf '%s\n' "$parallel_rows" | awk -F'|' 'NF >= 3 { ports[$2] = 1 } END { print length(ports) + 0 }')"
parallel_target_count="$(printf '%s\n' "$parallel_rows" | awk -F'|' 'NF >= 3 { targets[$3] = 1 } END { print length(targets) + 0 }')"
if [[ "$parallel_label_count" != 2 || "$parallel_port_count" != 2 || "$parallel_target_count" != 2 ]]; then
    printf 'parallel runs did not isolate label, port, or target:\n%s\n' "$parallel_rows" >&2
    exit 1
fi
assert_only_unrelated_container_remains

cleanup_failure_output="$test_root/cleanup-failure.out"
cleanup_failure_status=0
cleanup_failure_message=''
if FAKE_DOCKER_RM_STATUS=1 FAKE_CARGO_MODE=success \
    PATH="$fake_bin:$PATH" "$script_dir/mysql_integration.sh" test \
    >"$cleanup_failure_output" 2>&1; then
    cleanup_failure_status=0
else
    cleanup_failure_status=$?
fi
cleanup_failure_message="$(<"$cleanup_failure_output")"
if [[ "$cleanup_failure_status" != 1 ]] || \
    [[ "$cleanup_failure_message" != *'failed to clean up MySQL client containers'* ]]; then
    printf 'cleanup failure exited %s, expected 1\n%s\n' \
        "$cleanup_failure_status" "$cleanup_failure_message" >&2
    exit 1
fi
while IFS='|' read -r container_id label; do
    if [[ "$label" == com.sabiql.mysql.integration=run-* ]]; then
        "$fake_bin/docker" rm --force --volumes "$container_id"
    fi
done <"$state_file"
assert_only_unrelated_container_remains

unique_run_labels="$(awk '/^com\.sabiql\.mysql\.integration=run-/ { labels[$0] = 1 } END { print length(labels) + 0 }' "$label_log")"
if [[ "$unique_run_labels" != 10 ]]; then
    printf 'expected ten unique run labels, got %s\n%s\n' \
        "$unique_run_labels" "$(<"$label_log")" >&2
    exit 1
fi

run_project_count="$(awk -F'|' '$2 == "up" { print $1 }' "$compose_log" | sort -u | wc -l | tr -d ' ')"
port_lookup_count="$(awk -F'|' '$2 == "port" { print $1 }' "$compose_log" | sort -u | wc -l | tr -d ' ')"
down_project_count="$(awk -F'|' '$2 == "down" && $1 != "default" { print $1 }' "$compose_log" | sort -u | wc -l | tr -d ' ')"
if [[ "$run_project_count" != 10 || "$port_lookup_count" != 10 || "$down_project_count" != 10 ]] || \
    ! diff -u \
        <(awk -F'|' '$2 == "up" { print $1 }' "$compose_log" | sort -u) \
        <(awk -F'|' '$2 == "down" && $1 != "default" { print $1 }' "$compose_log" | sort -u); then
    printf 'Compose project lifecycle was not isolated:\n%s\n' "$(<"$compose_log")" >&2
    exit 1
fi
if ! grep -q -- '--force --volumes' "$rm_log"; then
    printf 'client cleanup did not request anonymous volume removal:\n%s\n' "$(<"$rm_log")" >&2
    exit 1
fi

if grep -Fq 'default|' "$compose_log"; then
    printf 'unowned default Compose project was accessed:\n%s\n' "$(<"$compose_log")" >&2
    exit 1
fi
