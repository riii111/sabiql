#!/usr/bin/env bash
set -Eeuo pipefail

readonly image="${SABIQL_MYSQL_IMAGE:-mysql:8.4.10}"
mount_args=()

for argument in "$@"; do
    case "$argument" in
        --defaults-file=/*)
            option_file="${argument#--defaults-file=}"
            mount_args+=(--volume "$option_file:$option_file:ro")
            ;;
        --defaults-file=*)
            echo "MySQL option file must use an absolute path" >&2
            exit 2
            ;;
    esac
done

if [[ -n "${SABIQL_MYSQL_TEST_TLS_DIR:-}" ]]; then
    mount_args+=(--volume "$SABIQL_MYSQL_TEST_TLS_DIR:$SABIQL_MYSQL_TEST_TLS_DIR:ro")
fi

docker_args=(--rm --interactive)
if [[ -n "${SABIQL_MYSQL_CONTAINER_LABEL:-}" ]]; then
    docker_args+=(--label "$SABIQL_MYSQL_CONTAINER_LABEL")
fi

exec docker run "${docker_args[@]}" \
    --add-host=host.docker.internal:host-gateway \
    "${mount_args[@]}" \
    "$image" mysql "$@"
