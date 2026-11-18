#!/usr/bin/env bash
set -euo pipefail
assets=$(cd "$(dirname "$0")" && pwd)
: "${SABIQL_DEMO_BIN:?Set the path to the sabiql executable}"
: "${SABIQL_DEMO_DIR:?Set an empty temporary directory}"
cd "$SABIQL_DEMO_DIR"
mkdir northline viewer
mkdir -p 'profile/Library/Application Support/sabiql'
printf 'version = 3\nicon_mode = "nerd-font"\nconnections = []\n' > 'profile/Library/Application Support/sabiql/connections.toml'
# The viewer scene is rendered from the exported SVG after the terminal recording.
ln -s /usr/bin/true viewer/open
initdb -D postgres -A trust --no-locale -E UTF8 -U merchant >/dev/null
pg_ctl -D postgres -l postgres.log -o "-k $SABIQL_DEMO_DIR -c listen_addresses=''" -w start
trap 'pg_ctl -D postgres -m fast -w stop >/dev/null' EXIT
export PGHOST="$SABIQL_DEMO_DIR" PGPORT=5432 PGUSER=merchant
createdb northline
psql -X -v ON_ERROR_STOP=1 -d northline -f "$assets/shop.sql" >/dev/null
cat > service.conf <<SERVICE
[shop]
host=$SABIQL_DEMO_DIR
port=5432
dbname=northline
user=merchant
SERVICE
vhs -o terminal.gif "$assets/shop.tape"
test "$(psql -XAt -d northline -c 'SELECT name FROM products WHERE id=103')" = 'Coffee Mug'
cp profile/Library/Caches/sabiql/northline/er_partial_public_products.svg diagram.svg
test -s diagram.svg
