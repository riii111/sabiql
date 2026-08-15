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
mkdir -p -- "$fake_bin"
: >"$state_file"
: >"$label_log"

cleanup() {
    rm -rf -- "$test_root"
}

trap cleanup EXIT

cat >"$fake_bin/docker" <<'EOF'
#!/usr/bin/env bash
set -Eeuo pipefail

state_file="${FAKE_DOCKER_STATE:?}"
label_log="${FAKE_DOCKER_LABEL_LOG:?}"

case "${1:-}" in
    compose)
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
        exit 0
        ;;
    failure|timeout)
        exit "${FAKE_CARGO_STATUS:-17}"
        ;;
    signal)
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

cat >"$fake_bin/mktemp" <<'EOF'
#!/usr/bin/env bash
set -Eeuo pipefail

for argument in "$@"; do
    case "$argument" in
        */mysql-client-label.XXXXXX)
            label_file="$(printf '%s\n' "$argument" | sed 's/XXXXXX$/fixed/')"
            mkdir -p -- "$(dirname "$label_file")"
            : >"$label_file"
            printf '%s\n' "$label_file"
            exit 0
            ;;
    esac
done

exec /usr/bin/mktemp "$@"
EOF
chmod +x "$fake_bin/mktemp"

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

cleanup_failure_output="$test_root/cleanup-failure.out"
cleanup_failure_status=0
if FAKE_DOCKER_RM_STATUS=1 FAKE_CARGO_MODE=success \
    PATH="$fake_bin:$PATH" "$script_dir/mysql_integration.sh" test \
    >"$cleanup_failure_output" 2>&1; then
    cleanup_failure_status=0
else
    cleanup_failure_status=$?
fi
if [[ "$cleanup_failure_status" != 1 ]] || ! rg -q 'failed to clean up MySQL client containers' "$cleanup_failure_output"; then
    printf 'cleanup failure exited %s, expected 1\n%s\n' \
        "$cleanup_failure_status" "$(<"$cleanup_failure_output")" >&2
    exit 1
fi
while IFS='|' read -r container_id label; do
    if [[ "$label" == com.sabiql.mysql.integration=run-* ]]; then
        "$fake_bin/docker" rm --force "$container_id"
    fi
done <"$state_file"
assert_only_unrelated_container_remains

unique_run_labels="$(awk '/^com\.sabiql\.mysql\.integration=run-/ { labels[$0] = 1 } END { print length(labels) + 0 }' "$label_log")"
if [[ "$unique_run_labels" != 7 ]]; then
    printf 'expected seven unique run labels, got %s\n%s\n' \
        "$unique_run_labels" "$(<"$label_log")" >&2
    exit 1
fi

PATH="$fake_bin:$PATH" "$script_dir/mysql_integration.sh" stop >/dev/null 2>&1
assert_only_unrelated_container_remains
