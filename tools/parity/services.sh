#!/usr/bin/env bash
# Parity-harness services: scratch MariaDB (two databases), two Redis, and the two API servers.
#
#   services.sh start            MariaDB + 2x Redis, schema + seed into both databases
#   services.sh servers ts-ts|ts-rs   (re)start server A (TS) and B (TS or Rust)
#   services.sh reset            truncate + reseed both databases, flush both Redis (servers keep running)
#   services.sh run [run.mjs args]    restart servers (same mode as last `servers`), reset, then node run.mjs against A/B
#   services.sh race             parallel refresh of one token against A and B (outcome shapes must match)
#   services.sh relay            PTT across two processes sharing one Redis: ts+ts, rs+rs, ts+rs, rs+ts must give identical transcripts
#   services.sh status | stop | stop-servers
#
# Everything lives under /tmp/kw-parity; ports are deliberately away from the dev defaults.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
BASE="${KW_BASE:-/tmp/kw-parity}"
SOCK="${KW_DB_SOCK:-/tmp/kw-parity.sock}"
DB_PORT="${KW_DB_PORT:-3391}"
REDIS_A_PORT="${KW_REDIS_A_PORT:-6391}"
REDIS_B_PORT="${KW_REDIS_B_PORT:-6392}"
PORT_A="${KW_PORT_A:-3410}"
PORT_B="${KW_PORT_B:-3411}"
NODE_ENV_VALUE="${KW_NODE_ENV:-development}"
TZ_VALUE="${KW_TZ:-Asia/Jakarta}"
RUST_BIN="${KW_RUST_BIN:-$REPO/target/release/kawalo-api}"
# Checkout of the legacy/typescript branch (the oracle); see tools/README.md.
TS_ROOT="${KW_TS_ROOT:-$REPO/../Kawalo-API-V2.0-ts}"
DB_A=kawalo_ts
DB_B=kawalo_rs

# Servers run from a scratch cwd so no .env file in the repo can leak into either side;
# certs/ is symlinked because both implementations read ./certs at startup.
CWD_DIR="$BASE/cwd"
mkdir -p "$BASE/logs" "$CWD_DIR"
[ -e "$CWD_DIR/certs" ] || ln -s "$REPO/certs" "$CWD_DIR/certs"

sql() { mariadb --no-defaults -S "$SOCK" -uroot "$@"; }

wait_for() { # wait_for <seconds> <cmd...>
  local n=$1; shift
  for _ in $(seq 1 "$n"); do "$@" >/dev/null 2>&1 && return 0; sleep 1; done
  return 1
}

start_mariadb() {
  if sql -e 'select 1' >/dev/null 2>&1; then return; fi
  if [ ! -d "$BASE/mariadb/mysql" ]; then
    mkdir -p "$BASE/mariadb"
    mariadb-install-db --no-defaults --datadir="$BASE/mariadb" --auth-root-authentication-method=normal >"$BASE/logs/install-db.log" 2>&1
  fi
  nohup mariadbd --no-defaults --datadir="$BASE/mariadb" --socket="$SOCK" --port="$DB_PORT" \
    --bind-address=127.0.0.1 --pid-file="$BASE/mariadb.pid" --default-time-zone=+00:00 \
    --innodb-buffer-pool-size=256M --innodb-flush-log-at-trx-commit=2 --max-connections=300 \
    >"$BASE/logs/mariadb.log" 2>&1 &
  wait_for 60 sql -e 'select 1' || { echo "MariaDB did not start; see $BASE/logs/mariadb.log" >&2; exit 1; }
}

apply_schema() { # apply_schema <db>
  local db=$1
  sql -e "CREATE DATABASE IF NOT EXISTS \`$db\` DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci"
  if [ "$(sql -N -e "select count(*) from information_schema.tables where table_schema='$db'")" != "0" ]; then return; fi
  for m in "$REPO"/prisma/migrations/*/migration.sql; do
    # some migration files start with dotenv banner lines and a BOM
    LC_ALL=C sed -e '1s/^\xEF\xBB\xBF//' "$m" | grep -v 'injected env' | sql "$db"
  done
}

seed_db() { # seed_db <db>: truncate everything (resets AUTO_INCREMENT) then load seed.sql
  local db=$1
  {
    echo "SET FOREIGN_KEY_CHECKS=0;"
    sql -N -e "select concat('TRUNCATE TABLE \`',table_name,'\`;') from information_schema.tables where table_schema='$db' and table_type='BASE TABLE'"
    echo "SET FOREIGN_KEY_CHECKS=1;"
  } | sql "$db"
  sql "$db" <"$HERE/seed.sql"
}

redis_cmd() { redis-cli -p "$1" "${@:2}"; }

start_redis() { # start_redis <port> <name>
  if redis_cmd "$1" ping >/dev/null 2>&1; then return; fi
  mkdir -p "$BASE/redis-$2"
  nohup redis-server --port "$1" --bind 127.0.0.1 --save "" --appendonly no --dir "$BASE/redis-$2" \
    >"$BASE/logs/redis-$2.log" 2>&1 &
  echo $! >"$BASE/redis-$2.pid"
  wait_for 20 redis_cmd "$1" ping
}

server_env() { # server_env <port> <db> <redis-port>
  echo "NODE_ENV=$NODE_ENV_VALUE PORT=$1 TZ=$TZ_VALUE LOG_LEVEL=warn"
  echo "DATABASE_URL=mysql://root@127.0.0.1:$DB_PORT/$2 REDIS_URL=redis://127.0.0.1:$3"
  echo "RATE_LIMIT_MAX=300 RATE_LIMIT_DURATION=60000"
}

start_server() { # start_server <label> <kind ts|rs> <port> <db> <redis-port>
  local label=$1 kind=$2 port=$3 db=$4 rport=$5
  stop_port "$port"
  # shellcheck disable=SC2046
  if [ "$kind" = ts ]; then
    [ -x "$TS_ROOT/node_modules/.bin/tsx" ] || { echo "TypeScript oracle not found at $TS_ROOT (set KW_TS_ROOT; see tools/README.md)" >&2; exit 1; }
    (cd "$CWD_DIR"; env $(server_env "$port" "$db" "$rport") nohup \
      "$TS_ROOT/node_modules/.bin/tsx" --tsconfig "$TS_ROOT/tsconfig.json" "$TS_ROOT/src/server.ts" \
      >"$BASE/logs/server-$label.log" 2>&1 </dev/null &)
  else
    [ -x "$RUST_BIN" ] || { echo "Rust binary not found: $RUST_BIN (run cargo build --release in the repo root)" >&2; exit 1; }
    (cd "$CWD_DIR"; env $(server_env "$port" "$db" "$rport") nohup "$RUST_BIN" \
      >"$BASE/logs/server-$label.log" 2>&1 </dev/null &)
  fi
  wait_for 90 curl -fs "http://127.0.0.1:$port/health" || { echo "server $label did not come up; see $BASE/logs/server-$label.log" >&2; exit 1; }
  echo "server $label ($kind) up on :$port"
}

stop_port() {
  local pids
  pids=$(lsof -tiTCP:"$1" -sTCP:LISTEN 2>/dev/null || true)
  [ -n "$pids" ] && kill $pids 2>/dev/null || true
  [ -n "$pids" ] && sleep 1
  return 0
}

cmd_start() {
  start_mariadb
  for db in "$DB_A" "$DB_B"; do apply_schema "$db"; seed_db "$db"; done
  start_redis "$REDIS_A_PORT" a
  start_redis "$REDIS_B_PORT" b
  echo "mariadb :$DB_PORT ($DB_A, $DB_B), redis :$REDIS_A_PORT / :$REDIS_B_PORT ready"
}

cmd_reset() {
  for db in "$DB_A" "$DB_B"; do seed_db "$db"; done
  redis_cmd "$REDIS_A_PORT" flushall >/dev/null
  redis_cmd "$REDIS_B_PORT" flushall >/dev/null
}

cmd_servers() {
  case "${1:-}" in
    ts-ts) kind_b=ts ;;
    ts-rs) kind_b=rs ;;
    *) echo "usage: services.sh servers ts-ts|ts-rs" >&2; exit 2 ;;
  esac
  echo "$1" >"$BASE/mode"
  start_server A ts "$PORT_A" "$DB_A" "$REDIS_A_PORT"
  start_server B "$kind_b" "$PORT_B" "$DB_B" "$REDIS_B_PORT"
}

cmd_stop_servers() { stop_port "$PORT_A"; stop_port "$PORT_B"; }

cmd_stop() {
  cmd_stop_servers
  redis_cmd "$REDIS_A_PORT" shutdown nosave 2>/dev/null || true
  redis_cmd "$REDIS_B_PORT" shutdown nosave 2>/dev/null || true
  mariadb-admin --no-defaults -S "$SOCK" -uroot shutdown 2>/dev/null || true
}

cmd_status() {
  sql -e 'select 1' >/dev/null 2>&1 && echo "mariadb: up" || echo "mariadb: down"
  for p in "$REDIS_A_PORT" "$REDIS_B_PORT"; do redis_cmd "$p" ping >/dev/null 2>&1 && echo "redis :$p up" || echo "redis :$p down"; done
  for p in "$PORT_A" "$PORT_B"; do curl -fs "http://127.0.0.1:$p/health" >/dev/null && echo "server :$p up" || echo "server :$p down"; done
}

# A run needs pristine servers: the TS rate limiter (elysia-rate-limit) breaks after its first 429 on a warm
# process, and in-memory state (PTT rooms, rate-limit buckets) must start empty on both sides.
cmd_run() {
  if [ -z "${KW_NO_RESTART:-}" ] && [ -f "$BASE/mode" ]; then cmd_servers "$(cat "$BASE/mode")"; fi
  cmd_reset
  node "$HERE/run.mjs" --a "http://127.0.0.1:$PORT_A" --b "http://127.0.0.1:$PORT_B" \
    --redis "redis://127.0.0.1:$REDIS_A_PORT,redis://127.0.0.1:$REDIS_B_PORT" "$@"
}

# Extra instances on :3421 (TS, Redis A), :3412 (Rust, Redis B) and :3413 (Rust, Redis A) next to A and B.
cmd_relay() {
  local out="$BASE/relay"
  mkdir -p "$out"
  start_server A ts "$PORT_A" "$DB_A" "$REDIS_A_PORT"
  start_server B rs "$PORT_B" "$DB_B" "$REDIS_B_PORT"
  start_server X ts 3421 "$DB_A" "$REDIS_A_PORT"
  start_server Y rs 3412 "$DB_B" "$REDIS_B_PORT"
  start_server Z rs 3413 "$DB_A" "$REDIS_A_PORT"
  cmd_reset
  # mints the tokens and their session_ver keys; the scenario itself is irrelevant
  node "$HERE/run.mjs" --a "http://127.0.0.1:$PORT_A" --b "http://127.0.0.1:$PORT_B" \
    --redis "redis://127.0.0.1:$REDIS_A_PORT,redis://127.0.0.1:$REDIS_B_PORT" --only core/jwks >/dev/null
  local rc=0 pair
  for pair in "ts-ts $PORT_A 3421" "rs-rs $PORT_B 3412" "ts-rs $PORT_A 3413" "rs-ts 3413 $PORT_A"; do
    set -- $pair
    node "$HERE/relay-check.mjs" --left "http://127.0.0.1:$2" --right "http://127.0.0.1:$3" >"$out/$1.json"
  done
  for pair in rs-rs ts-rs rs-ts; do
    if diff -q "$out/ts-ts.json" "$out/$pair.json" >/dev/null; then echo "PASS  relay ts-ts == $pair"
    else echo "FAIL  relay ts-ts != $pair (diff $out/ts-ts.json $out/$pair.json)"; rc=1; fi
  done
  stop_port 3421; stop_port 3412; stop_port 3413
  return $rc
}

case "${1:-}" in
  race) cmd_reset; node "$HERE/refresh-race.mjs" --a "http://127.0.0.1:$PORT_A" --b "http://127.0.0.1:$PORT_B" ;;
  relay) cmd_relay ;;
  start) cmd_start ;;
  reset) cmd_reset ;;
  servers) shift; cmd_servers "$@" ;;
  stop-servers) cmd_stop_servers ;;
  stop) cmd_stop ;;
  status) cmd_status ;;
  run) shift; cmd_run "$@" ;;
  *) sed -n '2,10p' "${BASH_SOURCE[0]}"; exit 2 ;;
esac
