#!/usr/bin/env bash
set -Eeuo pipefail

readonly script_dir="$(cd "$(dirname "$0")" && pwd)"
readonly repo_root="$(cd "$script_dir/.." && pwd)"
readonly compose_file="$repo_root/compose.yml"
readonly tls_compose_file="$repo_root/compose.mysql-tls.yml"
readonly temp_dir="$repo_root/.tmp/mysql"
readonly tls_dir="$temp_dir/tls"
readonly mysql_image="mysql:8.4.10"
readonly mysql_host="${SABIQL_MYSQL_TEST_HOST:-host.docker.internal}"
readonly mysql_port="${SABIQL_MYSQL_TEST_PORT:-3306}"
readonly mysql_database="${SABIQL_MYSQL_TEST_DATABASE:-sabiql_test}"
readonly mysql_user="${SABIQL_MYSQL_TEST_USER:-sabiql}"
readonly mysql_password="${SABIQL_MYSQL_TEST_PASSWORD:-p a#ss;=\"word}"

mysql_option_file=''
mysql_bin_dir=''

cleanup() {
    if [[ -n "$mysql_option_file" ]]; then
        rm -f -- "$mysql_option_file"
    fi
    if [[ -n "$mysql_bin_dir" ]]; then
        rm -rf -- "$mysql_bin_dir"
    fi
    rm -rf -- "$temp_dir"
    docker compose --file "$compose_file" --file "$tls_compose_file" rm --force --stop mysql >/dev/null 2>&1 || true
}

trap cleanup EXIT

quote_option_value() {
    local value="$1"
    value="${value//\\/\\\\}"
    value="${value//\"/\\\"}"
    printf '"%s"' "$value"
}

create_option_file() {
    mysql_option_file="$(mktemp)"
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
    mysql_bin_dir="$(mktemp -d)"
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
        --show-progress=none
}

case "${1:-test}" in
    test)
        mkdir -p -- "$temp_dir"
        export TMPDIR="$temp_dir"
        docker compose --file "$compose_file" --file "$tls_compose_file" rm --force --stop mysql >/dev/null 2>&1 || true
        create_tls_material
        docker compose --file "$compose_file" --file "$tls_compose_file" up --detach --wait mysql
        install_cli_wrapper
        create_option_file
        assert_versions
        run_tests
        ;;
    stop)
        docker compose --file "$compose_file" --file "$tls_compose_file" rm --force --stop mysql
        ;;
    *)
        echo "usage: $0 [test|stop]" >&2
        exit 2
        ;;
esac
