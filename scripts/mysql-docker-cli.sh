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

docker_args=(--rm --interactive)
if [[ -t 0 && -t 1 ]]; then
    docker_args+=(--tty)
fi

exec docker run "${docker_args[@]}" \
    --add-host=host.docker.internal:host-gateway \
    "${mount_args[@]}" \
    "$image" mysql "$@"
