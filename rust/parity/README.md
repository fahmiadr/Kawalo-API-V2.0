# Parity harness: TypeScript (A, reference) vs Rust (B)

Replays the same scenarios against two servers with separate, identically seeded MariaDB databases and Redis
instances, then diffs status, contract headers and normalized JSON bodies (and SSE / WebSocket transcripts).
Exit code is non-zero when anything differs. Node >= 20, no npm dependencies (uses `fetch`, `node:http`).

## Run

```bash
rust/parity/services.sh start              # scratch MariaDB :3391 (kawalo_ts, kawalo_rs), Redis :6391 / :6392, schema + seed
rust/parity/services.sh servers ts-rs      # A = TS on :3410, B = rust/target/release/kawalo-api on :3411
rust/parity/services.sh run                # restart servers, reset DB + Redis, run every scenario
rust/parity/services.sh run --only guest,incident --verbose
rust/parity/services.sh run --slow         # also the 30s PTT time-out-timer scenario
rust/parity/services.sh run --ignore-details   # skip `details` of error bodies (see below)
rust/parity/services.sh servers ts-ts      # self-check: both sides TypeScript, must be 100% green
rust/parity/services.sh relay              # PTT across two processes on one Redis: ts+ts, rs+rs, ts+rs, rs+ts transcripts must be identical
rust/parity/services.sh race               # parallel refresh of one token on both servers: same outcome shapes
rust/parity/services.sh stop               # everything (mariadb, redis, servers)
```

`run` always restarts both servers first: the TS rate limiter and the PTT rooms are in-memory, and
`elysia-rate-limit` misbehaves on a warm process after its first 429 (see "TS behaviours"). Use
`KW_NO_RESTART=1` to skip that. Direct invocation is also possible:
`node rust/parity/run.mjs --a http://127.0.0.1:3410 --b http://127.0.0.1:3411 --redis redis://127.0.0.1:6391,redis://127.0.0.1:6392 [--only x] [--skip y] [--list] [--json out.json] [--fail-fast]`
(the databases must be freshly seeded: `services.sh reset`).

Environment knobs (all optional): `KW_PORT_A`, `KW_PORT_B`, `KW_DB_PORT`, `KW_REDIS_A_PORT`, `KW_REDIS_B_PORT`,
`KW_NODE_ENV` (default `development`; `production` hides `details`), `KW_TZ` (default `Asia/Jakarta`, see
"TS behaviours"), `KW_RUST_BIN`, `KW_BASE` (default `/tmp/kw-parity`). Both servers run from `$KW_BASE/cwd`, which only
contains a `certs` symlink, so no repo `.env*` file can influence either side. They get the same variables
(`NODE_ENV PORT TZ LOG_LEVEL DATABASE_URL REDIS_URL RATE_LIMIT_MAX=300 RATE_LIMIT_DURATION=60000`); logs are in
`$KW_BASE/logs`. MariaDB runs with `--default-time-zone=+00:00`.

Tokens come from `mint-token.mts` (real `TokenService` + `RsaKeyManager`, key material never printed). It also writes
the matching `auth:user:<id>:session_ver` keys into both Redis instances. `run.mjs` calls it automatically;
the result is cached in `.cache/tokens.json` (git-ignored, valid 15 minutes).

Every decision about a difference is recorded in `KNOWN_DIFFERENCES.md`; the gate that must be 0 is a no-flag run with `KW_NODE_ENV=production`.

## Layout

| Path | Purpose |
|---|---|
| `run.mjs` | CLI, scenario loading, summary |
| `lib/harness.mjs` | `Server` (HTTP / SSE / WebSocket against one side) and `Harness` (runs both, diffs) |
| `lib/normalize.mjs` | placeholders + structural diff |
| `lib/ws.mjs` | tiny RFC 6455 client (custom handshake headers, rejected-handshake status/body) |
| `lib/cases.mjs` | query matrix shared by every list endpoint |
| `scenarios/*.mjs` | scenarios (core, auth, readonly, guest, information, incident, ptt) |
| `relay-check.mjs`, `refresh-race.mjs` | cross-process PTT transcript and parallel refresh checks (`services.sh relay` / `race`) |
| `seed.sql` | deterministic dummy data (groups alpha/beta/gamma, users with every role/flag, guests, feeds, attendance, duty journeys, incidents) |
| `mint-token.mts`, `services.sh` | tokens, services |

## Adding a scenario

A scenario file default-exports one definition or an array: `{ name: 'guest/foo', order: 30, slow?: true, async run(t) {...} }`.

```js
await t.step('label', {
  method: 'POST', path: '/api/v1/guests',          // path / query / body / headers / token may be (vars) => value
  token: 'guard1_g1',                              // a name from mint-token.mts, or 'raw:<jwt>', or null
  body: { ... } | rawBody: '{bad',                 // rawBody sends text untouched
  expect: 201,                                     // sanity check on A only: prints WARN (and fails the run) if the reference answers differently
  capture: { gid: 'data.id' },                     // stored per server in vars; later specs use (v) => `/guests/${v.gid}`
  ip: t.ipFor(2),                                  // default is one client ip per scenario (X-Forwarded-For), so rate limits never leak
  ignore: ['data.something'],                      // last resort: drop a body path before diffing
});
await t.compute('label', async (srv) => ({ ... }));          // arbitrary per-server logic returning a comparable value
await t.sse('label', { path, token }, async (s) => { await s.expect('frame'); await s.req('do', {...}); await s.silence('quiet'); });
await t.wsSession('label', async (srv, log) => { const c = await srv.ws('c', '/ws/ptt/grp-alpha', { token: 'ptt_guard1' }, log); await c.expect('first frame'); c.send({...}); });
```

Rules of thumb: give every scenario its own users/rows when it mutates state (the seed has `lock*`, `refresh*`, `login*`,
`logout*`, `switch1` users and `ptt_*` / `sse_*` tokens for that), sleep >= 5 ms between writes whose order matters (UUIDv7 ids sort by
millisecond), and never rely on rows created in the same second keeping an order (timestamps have 1 s resolution).
WebSocket/SSE reads are strict-order: `expect` takes the next frame, so an extra or reordered frame is a diff.

## What is normalized (and nothing else)

- Generated ids: `inc_|rpl_|guest_|feed_` + UUIDv7 -> `<inc#1>`, bare UUIDv4 -> `<uuid4#1>`, UUIDv7 -> `<uuid7#1>`; 64-hex strings (refresh tokens) -> `<hex64#1>`.
  Numbered per server and scenario by order of first appearance (keys visited in sorted order), so *reuse* of an id is still checked. Seeded ids are not UUIDv4/v7-shaped and stay literal.
- Timestamps (`...Z` strings) within +-10 minutes of "now" -> `<now>`. Anything else (seeded dates, user supplied dates) is compared literally, so time-zone shifts are caught.
- JWT strings become `{ $jwt: n, sig: valid|INVALID, header, claims }`: signature verified against the server's own JWKS, `iat` -> `<now>`, `exp` -> `ttl = exp - iat`, all other claims literal.
- `uptime`, `latencyMs` numbers -> `<num>`.
- JSON object key order is ignored; array order is kept.
- Headers: compared are `content-type` (whitespace after `;` ignored), `x-content-type-options`, `x-frame-options`, `cache-control`, `access-control-*`, `vary`, `ratelimit-limit`, `ratelimit-remaining`, `x-accel-buffering`, `allow`, `location`, plus the sorted *set* of header names. `retry-after` and `ratelimit-reset` may differ by 3 s. `x-request-id` is checked for presence (or echo when the request supplied one).
- Heartbeat: only "a `: ping` arrived 13-17.5 s after the retry hint" is recorded.

## Intentionally not compared

- `date`, `content-length`, `connection`, `keep-alive`, `transfer-encoding`: transport detail.
- The exact `x-request-id` value, JWT signature bytes, `iat`/`exp` values, refresh token bytes: random by design.
- The 101 upgrade response headers except `Upgrade` and the subprotocol echo: Elysia leaks `content-type: text/plain`, CORS and rate-limit headers there; no client depends on them.
- `details` of error bodies only with `--ignore-details`. By default they ARE compared (TS `development` mode returns TypeBox text such as `/email: Expected property 'email' to be string but found: undefined`). A practical gate: run once with `KW_NODE_ENV=production` (details absent) as the hard requirement, once in development to see how far the validation messages match.
- Not exercised by the scenarios, with reason: multi-instance PTT relay and concurrent refresh (covered by `services.sh relay` / `race`); `/health/ready` with a dead dependency (checked by hand, see KNOWN_DIFFERENCES.md); refresh-token expiry (14 days); `tkuser` ids beyond int32 (Prisma error text is engine specific); a rate-limited request to an unknown route (the TS process crashes, see below); key rotation / file watcher; log output; graceful shutdown.

## TS behaviours the harness pins down (Rust must reproduce them or the diff shows)

- Validation (422) runs before authentication: an unauthenticated request with an invalid body/query/params answers 422, not 401/403.
- `limit`/`offset` are `t.Numeric`: `2.5` -> 2, `1e1` -> 10, `0x5` -> 5, `" 2 "` -> 2, repeated param -> last value, `""` / `abc` / negative / `limit>100` -> 422; `limit=0` means unlimited and ignores `offset`.
- `search` uses `LIKE '%x%'` without escaping: `%` and `_` act as wildcards; collation is `utf8mb4_unicode_ci` (case and accent insensitive, `ñ` matches `n`). Values `N/A`, `null`, blanks are dropped by `cleanString`; anything shaped like an email is lower-cased everywhere (claims, `created_by`, host email).
- Dates: `start_date`/`end_date` that are invalid, epoch-zero or `0000-...` are ignored silently.
- JWT verification: unknown `kid` falls back to the active key; `session_ver` may be a numeric string or float (truncated); role is upper-cased; Redis key missing -> rehydrated from the token; token version above Redis is accepted.
- Login: password is trimmed, compared in plaintext; the 5th wrong password still answers 401 and locks, later attempts (even correct ones, any ip, any email casing) are 429 `AUTH_ACCOUNT_LOCKED` + `Retry-After`; the ip throttle counts failures only and blocks the 11th attempt.
- Refresh: replay within 15 s returns the same tokens (grace); replay after that revokes the whole family (`AUTH_REUSE_DETECTED`).
- Operational endpoint depends on the process time zone (`toLocalDatetime`, `DATE_FORMAT`): TS and Rust must run with the same `TZ` (the harness uses `Asia/Jakarta`).
- Detail lookups by id also match QR uid (`/guests/12` matches id 12 or a QR uid `12`); delete by QR uid is a 404.
- PTT: no validation of `requestId` (missing/numeric is echoed), an admin re-requesting its own floor is denied `CHANNEL_BUSY`, member count is a Redis set of user ids (two sockets of one user count once; closing either removes the user), handshake rejections are `text/plain` bodies (`Unauthorized: ...` 401, `Forbidden: ...` 403), token via `?token=`, `Authorization` (any scheme case) or `Sec-WebSocket-Protocol`, the query token wins.
- `elysia-rate-limit` bug (do not replicate): its static `errorResponse` Response is cloned after being consumed, so on a warm process a 429 can turn into a 500 and a rate-limited 404 crashes the process. The intended answer, which the scenario checks, is the 429 JSON body with `retry-after` and `ratelimit-*` headers.
