#!/usr/bin/env bash
# Scratch MariaDB (3392) + Redis (6393) for benchmarks. Isolated from the user's instances and other agents.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
API_ROOT="$(cd "$HERE/../.." && pwd)"
DATA=/tmp/kw-bench-data
SOCK=/tmp/kw-bench.sock
DBPORT=3392
REDIS_PORT=6393
REDIS_DIR=/tmp/kw-bench-redis
DB=kawalo_bench

sql() { mariadb --no-defaults -uroot --socket="$SOCK" "$@"; }

start_db() {
  if [ ! -d "$DATA/mysql" ]; then
    mariadb-install-db --no-defaults --datadir="$DATA" --auth-root-authentication-method=normal --skip-test-db >/dev/null
  fi
  if ! sql -e 'select 1' >/dev/null 2>&1; then
    # Tuned like a modest production box, identical for every server under test.
    nohup mariadbd --no-defaults --datadir="$DATA" --socket="$SOCK" --port=$DBPORT \
      --bind-address=127.0.0.1 --skip-name-resolve --pid-file=/tmp/kw-bench-db.pid \
      --innodb-buffer-pool-size=1G --innodb-flush-log-at-trx-commit=2 --max-connections=500 \
      >/tmp/kw-bench-db.log 2>&1 &
    for _ in $(seq 1 60); do sql -e 'select 1' >/dev/null 2>&1 && break; sleep 0.5; done
  fi
  if ! sql "$DB" -e "select 1 from tkuser limit 1" >/dev/null 2>&1; then
    sql -e "drop database if exists $DB; create database $DB character set utf8mb4"
    # 0_init starts with a dotenv banner lines that are not SQL; drop them.
    for f in "$API_ROOT"/prisma/migrations/*/migration.sql; do sed '/injected env/d' "$f" | sql "$DB"; done
    echo "schema applied (run: node tools/bench/seed.mjs)"
  fi
}

start_redis() {
  mkdir -p "$REDIS_DIR"
  if ! redis-cli -p $REDIS_PORT ping >/dev/null 2>&1; then
    redis-server --port $REDIS_PORT --bind 127.0.0.1 --save "" --appendonly no --dir "$REDIS_DIR" \
      --pidfile "$REDIS_DIR/redis.pid" --daemonize yes >/dev/null
    for _ in $(seq 1 20); do redis-cli -p $REDIS_PORT ping >/dev/null 2>&1 && break; sleep 0.25; done
  fi
}

stop_all() {
  redis-cli -p $REDIS_PORT shutdown nosave >/dev/null 2>&1 || true
  mariadb-admin --no-defaults -uroot --socket="$SOCK" shutdown >/dev/null 2>&1 || true
}

case "${1:-}" in
  start) start_db; start_redis
    echo "DATABASE_URL=mysql://root:@127.0.0.1:$DBPORT/$DB"; echo "REDIS_URL=redis://127.0.0.1:$REDIS_PORT" ;;
  stop) stop_all ;;
  reset) stop_all; sleep 1; rm -rf "$DATA" "$REDIS_DIR"; "$0" start ;;
  *) echo "usage: $0 start|stop|reset" >&2; exit 1 ;;
esac
