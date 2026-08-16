#!/usr/bin/env bash
set -Eeuo pipefail

script_dir="$(cd "$(dirname "$0")" && pwd)"
readonly script_dir
repo_root="$(cd "$script_dir/.." && pwd)"
readonly repo_root
readonly compose_file="$repo_root/compose.yml"
readonly tls_compose_file="$repo_root/compose.mysql-tls.yml"
readonly temp_root="$repo_root/.tmp/mysql"
readonly mysql_image="mysql:8.4.10"
readonly mysql_host="${SABIQL_MYSQL_TEST_HOST:-host.docker.internal}"
mysql_port="${SABIQL_MYSQL_TEST_PORT:-}"
readonly mysql_database="${SABIQL_MYSQL_TEST_DATABASE:-sabiql_test}"
readonly mysql_user="${SABIQL_MYSQL_TEST_USER:-sabiql_test_runner}"
readonly mysql_password="${SABIQL_MYSQL_TEST_PASSWORD:-p a#ss;=\"word}"
readonly mysql_client_label_key='com.sabiql.mysql.integration'

run_dir=''
temp_dir=''
tls_dir=''
compose_project=''
compose_args=()
repo_hash=''
mysql_host_port=''
mysql_option_file=''
mysql_bin_dir=''
mysql_run_label=''
mysql_client_label=''
cleanup_failed=0

create_client_container_label() {
    mysql_run_label="run-${repo_hash}-$(basename "$run_dir")"
    mysql_client_label="${mysql_client_label_key}=${mysql_run_label}"
}

run_compose() {
    docker compose "${compose_args[@]}" "$@"
}

prepare_run() {
    local repo_hash_value

    mkdir -p -- "$temp_root"
    run_dir="$(mktemp -d "$temp_root/run-XXXXXX")"
    temp_dir="$run_dir"
    tls_dir="$temp_dir/tls"
    repo_hash_value="$(printf '%s' "$repo_root" | cksum | awk '{print $1}')"
    repo_hash="$repo_hash_value"
    compose_project="sabiql-mysql-${repo_hash}-${run_dir##*/}"
    compose_args=(
        --project-name "$compose_project"
        --file "$compose_file"
        --file "$tls_compose_file"
    )
    if [[ -n "${SABIQL_MYSQL_TEST_PORT:-}" ]]; then
        mysql_host_port="$SABIQL_MYSQL_TEST_PORT"
    else
        mysql_host_port=0
    fi
    create_client_container_label
    export SABIQL_MYSQL_CONTAINER_LABEL="$mysql_client_label"
    export SABIQL_MYSQL_HOST_PORT="$mysql_host_port"
    export SABIQL_MYSQL_RUN_LABEL="$mysql_run_label"
    export SABIQL_MYSQL_TLS_DIR="$tls_dir"
    export TMPDIR="$temp_dir"
    export RUSTC_WRAPPER=
    export CARGO_TARGET_DIR="$temp_dir/cargo-target"
}

cleanup_client_containers() {
    if [[ -z "$mysql_client_label" ]]; then
        return
    fi

    local container_id
    local container_ids
    local remaining_containers
    if ! container_ids="$(docker ps --all --quiet --filter "label=$mysql_client_label")"; then
        return 1
    fi
    while IFS= read -r container_id; do
        if [[ -n "$container_id" ]]; then
            docker rm --force --volumes "$container_id" >/dev/null 2>&1 || :
        fi
    done <<<"$container_ids"

    if ! remaining_containers="$(docker ps --all --quiet --filter "label=$mysql_client_label")"; then
        return 1
    fi
    [[ -z "$remaining_containers" ]]
}

cleanup() {
    local status="$1"
    cleanup_failed=0

    if ! cleanup_client_containers; then
        printf 'failed to clean up MySQL client containers for %s\n' "$mysql_client_label" >&2
        cleanup_failed=1
    fi
    if [[ -n "$compose_project" ]] && ! run_compose down --volumes --remove-orphans >/dev/null 2>&1; then
        printf 'failed to clean up MySQL Compose project %s\n' "$compose_project" >&2
        cleanup_failed=1
    fi
    if [[ -n "$mysql_option_file" ]] && ! rm -f -- "$mysql_option_file"; then
        cleanup_failed=1
    fi
    if [[ -n "$mysql_bin_dir" ]] && ! rm -rf -- "$mysql_bin_dir"; then
        cleanup_failed=1
    fi
    if [[ -n "$run_dir" ]] && ! rm -rf -- "$run_dir"; then
        cleanup_failed=1
    fi
    if [[ "$status" == 0 && "$cleanup_failed" != 0 ]]; then
        status=1
    fi
    return "$status"
}

handle_exit() {
    local status="$1"
    local cleanup_status

    trap - EXIT HUP INT TERM
    if cleanup "$status"; then
        cleanup_status=0
    else
        cleanup_status=$?
    fi
    if [[ "$status" == 0 && "$cleanup_status" != 0 ]]; then
        exit "$cleanup_status"
    fi
    exit "$status"
}

handle_signal() {
    local status="$1"

    trap - EXIT HUP INT TERM
    cleanup "$status" || :
    if [[ "$cleanup_failed" != 0 ]]; then
        exit 1
    fi
    exit "$status"
}

trap 'handle_exit "$?"' EXIT
trap 'handle_signal 129' HUP
trap 'handle_signal 130' INT
trap 'handle_signal 143' TERM

quote_option_value() {
    local value="$1"
    value="${value//\\/\\\\}"
    value="${value//\"/\\\"}"
    printf '"%s"' "$value"
}

create_option_file() {
    mysql_option_file="$(mktemp "$temp_dir/mysql-option.XXXXXX")"
    chmod 600 "$mysql_option_file"
    {
        printf '%s\n' '[client]'
        printf 'host = %s\n' "$(quote_option_value "$mysql_host")"
        printf 'port = %s\n' "$(quote_option_value "$mysql_port")"
        printf 'user = %s\n' "$(quote_option_value "$mysql_user")"
        printf 'password = %s\n' "$(quote_option_value "$mysql_password")"
        printf 'database = %s\n' "$(quote_option_value "$mysql_database")"
    } >"$mysql_option_file"
}

discover_mysql_port() {
    if [[ "$mysql_host_port" != 0 ]]; then
        return
    fi

    local published_port
    published_port="$(run_compose port mysql 3306)"
    if [[ "$published_port" =~ :([0-9]+)$ ]]; then
        mysql_port="${BASH_REMATCH[1]}"
    else
        printf 'failed to discover the published MySQL port: %s\n' "$published_port" >&2
        return 1
    fi
}

create_tls_material() {
    mkdir -p -- "$tls_dir"
    openssl genrsa -out "$tls_dir/ca-key.pem" 2048 >/dev/null 2>&1
    openssl req -x509 -new -key "$tls_dir/ca-key.pem" -sha256 -days 1 \
        -subj '/CN=sabiql-test-ca' -out "$tls_dir/ca.pem" >/dev/null 2>&1
    openssl genrsa -out "$tls_dir/server-key.pem" 2048 >/dev/null 2>&1
    openssl req -new -key "$tls_dir/server-key.pem" -subj '/CN=localhost' \
        -out "$tls_dir/server.csr" >/dev/null 2>&1
    printf '%s\n' '[v3_server]' 'subjectAltName = DNS:localhost,IP:127.0.0.1' \
        >"$tls_dir/server-ext.cnf"
    openssl x509 -req -in "$tls_dir/server.csr" -CA "$tls_dir/ca.pem" \
        -CAkey "$tls_dir/ca-key.pem" -CAcreateserial -out "$tls_dir/server-cert.pem" \
        -days 1 -sha256 -extfile "$tls_dir/server-ext.cnf" -extensions v3_server \
        >/dev/null 2>&1
    openssl genrsa -out "$tls_dir/client-key.pem" 2048 >/dev/null 2>&1
    openssl req -new -key "$tls_dir/client-key.pem" -subj '/CN=sabiql-client' \
        -out "$tls_dir/client.csr" >/dev/null 2>&1
    openssl x509 -req -in "$tls_dir/client.csr" -CA "$tls_dir/ca.pem" \
        -CAkey "$tls_dir/ca-key.pem" -CAcreateserial -out "$tls_dir/client-cert.pem" \
        -days 1 -sha256 >/dev/null 2>&1
    chmod 644 "$tls_dir"/*.pem
    rm -f -- "$tls_dir/ca-key.pem" "$tls_dir/server.csr" "$tls_dir/server-ext.cnf" \
        "$tls_dir/client.csr" "$tls_dir/ca.srl"
}

install_cli_wrapper() {
    mysql_bin_dir="$(mktemp -d "$temp_dir/mysql-bin.XXXXXX")"
    ln -s "$script_dir/mysql-docker-cli.sh" "$mysql_bin_dir/mysql"
    export PATH="$mysql_bin_dir:$PATH"
    export SABIQL_MYSQL_IMAGE="$mysql_image"
}

assert_versions() {
    local cli_version
    local server_version
    cli_version="$(mysql --version)"
    case "$cli_version" in
        *'Ver 8.4.10'*) ;;
        *)
            echo "expected Oracle MySQL CLI 8.4.10, got: $cli_version" >&2
            return 1
            ;;
    esac

    server_version="$(mysql \
        --defaults-file="$mysql_option_file" \
        --no-login-paths \
        --protocol=TCP \
        --batch \
        --raw \
        --skip-column-names \
        --binary-mode \
        --skip-reconnect \
        --execute='SELECT VERSION()')"
    case "$server_version" in
        8.4.10*) ;;
        *)
            echo "expected Oracle MySQL server 8.4.10, got: $server_version" >&2
            return 1
            ;;
    esac
}

run_tests() {
    export SABIQL_MYSQL_TEST_HOST="$mysql_host"
    export SABIQL_MYSQL_TEST_PORT="$mysql_port"
    export SABIQL_MYSQL_TEST_DATABASE="$mysql_database"
    export SABIQL_MYSQL_TEST_USER="$mysql_user"
    export SABIQL_MYSQL_TEST_PASSWORD="$mysql_password"
    export SABIQL_MYSQL_TEST_TLS_DIR="$tls_dir"
    export SABIQL_MYSQL_TEST_SSL_CA="$tls_dir/ca.pem"
    export SABIQL_MYSQL_TEST_SSL_CERT="$tls_dir/client-cert.pem"
    export SABIQL_MYSQL_TEST_SSL_KEY="$tls_dir/client-key.pem"
    export SABIQL_MYSQL_TRANSCRIPT=1
    cargo nextest run -p sabiql --run-ignored ignored-only \
        -E 'test(tests::adapter_mysql)' \
        --test-threads 1 \
        --no-fail-fast \
        --hide-progress-bar
}

case "${1:-test}" in
    test)
        prepare_run
        create_tls_material
        run_compose up --detach --wait mysql
        discover_mysql_port
        install_cli_wrapper
        create_option_file
        assert_versions
        run_tests
        ;;
    stop)
        docker compose --file "$compose_file" --file "$tls_compose_file" down --volumes --remove-orphans
        ;;
    *)
        echo "usage: $0 [test|stop]" >&2
        exit 2
        ;;
esac
