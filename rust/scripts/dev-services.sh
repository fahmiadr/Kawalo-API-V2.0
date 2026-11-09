#!/usr/bin/env bash
# Scratch MariaDB + Redis for Rust integration tests. Never touches the developer's own instances.
#
#   dev-services.sh start   start both (idempotent), apply schema + seed on first run
#   dev-services.sh stop    shut both down
#   dev-services.sh reset   drop and recreate the database (schema + seed), flush Redis
#   dev-services.sh env     print the export lines for DATABASE_URL / REDIS_URL
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
API_ROOT="$(cd "$HERE/../.." && pwd)"
BASE="${KW_RUST_BASE:-/tmp/kw-rust}"
SOCK="${KW_RUST_DB_SOCK:-/tmp/kw-rust-db.sock}"
DB_PORT="${KW_RUST_DB_PORT:-3380}"
REDIS_PORT="${KW_RUST_REDIS_PORT:-6390}"
DB_NAME="${KW_RUST_DB_NAME:-kawalo_rust}"

sql() { mariadb --no-defaults -uroot --socket="$SOCK" "$@"; }

wait_for() { # wait_for <tries> <cmd...>
  local n=$1; shift
  for _ in $(seq 1 "$n"); do "$@" >/dev/null 2>&1 && return 0; sleep 0.5; done
  return 1
}

start_db() {
  mkdir -p "$BASE"
  if ! sql -e 'select 1' >/dev/null 2>&1; then
    if [ ! -d "$BASE/mariadb/mysql" ]; then
      mariadb-install-db --no-defaults --datadir="$BASE/mariadb" \
        --auth-root-authentication-method=normal --skip-test-db >"$BASE/install-db.log" 2>&1
    fi
    nohup mariadbd --no-defaults --datadir="$BASE/mariadb" --socket="$SOCK" --port="$DB_PORT" \
      --bind-address=127.0.0.1 --skip-name-resolve --pid-file="$BASE/mariadb.pid" \
      --default-time-zone=+00:00 --innodb-flush-log-at-trx-commit=2 --max-connections=200 \
      >"$BASE/mariadb.log" 2>&1 &
    wait_for 120 sql -e 'select 1' || { echo "MariaDB failed to start, see $BASE/mariadb.log" >&2; exit 1; }
  fi
  if ! sql "$DB_NAME" -e 'select 1 from tkuser limit 1' >/dev/null 2>&1; then
    load_schema
  fi
}

load_schema() {
  sql -e "DROP DATABASE IF EXISTS \`$DB_NAME\`; CREATE DATABASE \`$DB_NAME\` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci"
  local f
  for f in "$API_ROOT"/prisma/migrations/*/migration.sql; do
    # 0_init starts with a BOM and dotenv banner lines that are not SQL.
    LC_ALL=C sed -e '1s/^\xEF\xBB\xBF//' -e '/injected env/d' "$f" | sql "$DB_NAME"
  done
  sql "$DB_NAME" <"$HERE/seed.sql"
}

start_redis() {
  mkdir -p "$BASE/redis"
  if ! redis-cli -p "$REDIS_PORT" ping >/dev/null 2>&1; then
    redis-server --port "$REDIS_PORT" --bind 127.0.0.1 --save "" --appendonly no \
      --dir "$BASE/redis" --pidfile "$BASE/redis/redis.pid" --daemonize yes >/dev/null
    wait_for 40 redis-cli -p "$REDIS_PORT" ping || { echo "Redis failed to start" >&2; exit 1; }
  fi
}

print_env() {
  echo "export DATABASE_URL='mysql://root:@127.0.0.1:$DB_PORT/$DB_NAME' REDIS_URL='redis://127.0.0.1:$REDIS_PORT'"
}

case "${1:-}" in
  start) start_db; start_redis; print_env ;;
  stop)
    redis-cli -p "$REDIS_PORT" shutdown nosave >/dev/null 2>&1 || true
    mariadb-admin --no-defaults -uroot --socket="$SOCK" shutdown >/dev/null 2>&1 || true ;;
  reset) start_db; start_redis; load_schema; redis-cli -p "$REDIS_PORT" flushall >/dev/null; print_env ;;
  env) print_env ;;
  *) echo "usage: $0 start|stop|reset|env" >&2; exit 1 ;;
esac
