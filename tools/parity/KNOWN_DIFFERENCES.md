# Known differences between the TypeScript and Rust servers

State at the last full parity run: `KW_NODE_ENV=production` (the hard requirement), 1420 steps, 0 differences, stable over
repeated runs; `KW_NODE_ENV=development` has the differences listed under "Development mode `details`" and none otherwise
(`run --ignore-details` is 0 differences). Everything below is a decision, not an open bug.

## Not replicated on purpose (TS defects)

| TS behaviour | Rust | Why |
|---|---|---|
| `elysia-rate-limit` clones a consumed static `Response`: on a warm process the second 429 can become a 500, and a rate-limited request to an unknown route crashes the process | Always the 429 JSON envelope with `retry-after` and `ratelimit-*` | A crash is not a contract. The harness checks the intended 429 on a fresh process (`run` restarts both servers first). |
| crossws answers a WebSocket `ping` frame incorrectly | A normal pong | No client depends on the defect. |
| The 101 upgrade response leaks `content-type: text/plain`, CORS and rate-limit headers | Only what the handshake needs | The harness compares `Upgrade` and the subprotocol echo only. |
| Login compares the stored plain-text `Password` only, and returns sooner for an unknown e-mail than for a known one | While `Password` is filled it still decides; Argon2id verification against `tkuser.PasswordHash` is an extra check that refreshes the hash when stale (phase 2: empty `Password` makes the hash decide). Every failed login spends one real or dummy Argon2 verification | Response bodies, statuses and throttling are identical, so the harness sees nothing; TS ignores the new column and Rust never writes `Password`. See README, "Password: Argon2id". |

## Elysia quirks that Rust reproduces (probed against the real server)

* Body parser choice: the part of `Content-Type` before the first `;`, untrimmed and case sensitive, is classified by its
  13th character: `j` JSON, `x` URL-encoded form (so `application/xml` is a form), `r` multipart, `o` binary, otherwise
  `text/*` (anything starting with `t`) is text; everything else, or no header, leaves no body (`422 : Expected object`).
  `application/vnd.api+json` and `APPLICATION/JSON` are therefore not JSON.
* Text and binary bodies validate as an empty object: a schema with only optional properties accepts them (`PATCH` with
  `text/plain` succeeds), one with required properties answers 422.
* Forms: every value is text (no coercion for numbers), a repeated key becomes an array (`photos=a&photos=b` satisfies an
  array field, `title=A&title=B` fails a string field), `+` is a space, a key without `=` has the value `""`.
* Multipart: text fields as in a form, a part with a filename is a file object (fails every scalar type), a lone field
  whose text starts with `{` or `[` and parses as JSON is replaced by the parsed value, a body that is not multipart is
  `400 BAD_REQUEST`.
* Routing: one trailing slash is ignored (`/health/`, `/api/v1/guests/`), `//`, a second slash and `//x` are 404. Wrong
  methods are plain 404 envelopes without an `allow` header. `OPTIONS` is answered by the CORS plugin before the request
  id and logger hooks, so it carries no `x-request-id`.
* `/guests/master-types` accepts only `GET` as its own route, but `PUT`, `PATCH` and `DELETE` fall through to the `/:id`
  handlers with the id `master-types` ("Data tamu tidak ditemukan." after validation and auth).
* `t.Numeric` query values: JavaScript `Number(text)` (`0x5`, `0b11`, `1e1`, `" 2 "`, blank is 0, `Infinity`), the empty
  string and `NaN` spellings are format errors, the last of a repeated parameter wins (for every query field, keys are
  percent-decoded). Validation has two stages: format errors of all fields are listed together, and only when none exist
  the first range violation is reported alone with an empty path (`: Expected number to be less or equal to 100`).
* `Number()` is also what `cleanNumber`/`cleanCoordinate` use, so `0x1A` is a valid stored coordinate.

## Development mode `details` (not implemented)

In `NODE_ENV=production` 422 bodies have no `details`. In development TS prints TypeBox's own error list; Rust prints
serde-derived text, which differs for most body errors (113 of 1419 steps). Query-string errors are already exact.
Grammar of the TS text, should someone want to close the gap (it needs a per-DTO schema description, because the text
depends on nullability and on *all* failing properties, in schema order, not just the first):

* wrong or missing string: `/title: Expected property 'title' to be string but found: undefined` (the found value is
  JavaScript `String(value)`: `5`, `null`, `a,b` for arrays, `[object Object]`, `[object File]`); nested paths use dots
  in the property name and slashes in the pointer (`/photos/0: Expected property 'photos.0' ...`)
* a nullable field (`t.Nullable`) answers every failure of its value, including length, format and range, with
  `/glat: Property 'glat' should be one of: 'number', 'null'`; nullable email `'email', 'null'`; a nullable enum
  `'undefined', 'null'`
* required object missing: `: Expected object`; wrong array: `/photos: Expected array`,
  `/photos: Expected array length to be less or equal to 4`
* non-nullable length/format: `/title: Expected string length greater or equal to 1`, `Property 'email' should be email`

## Approximations

* Multipart keys such as `a.b` or `a[0]` are nested by Elysia; Rust keeps them under their literal name, so they cannot
  reach a DTO field. Only an adversarial client can tell.
* `application/octet-stream` is treated as an empty object (an `ArrayBuffer` passes an all-optional schema in TS).
* JavaScript and Rust disagree on a few exotic whitespace code points when trimming numeric text, and TS leaves an
  invalid `%XX` sequence in a form value raw while Rust's decoder only skips the broken triplet.
* Redis outage longer than a few seconds: redis-rs backs off its reconnect attempts, so `/health/ready` can stay 503 for
  a few seconds after Redis returns (a short outage recovers in about 150 ms). ioredis in TS reconnects within a
  second. Both answer the same 503 envelope while it is down.
* Database outage: both answer the same 500 envelope, the TS driver after about 10 s, Rust after 5 s (pool acquire timeout).
* `x-request-id`, `date`, JWT signatures and the exact `retry-after` second are random or clock based; the harness checks
  presence/echo and a 3 second tolerance instead of equality.

## What the harness relaxes (and nothing else)

| Relaxation | Where | Scope |
|---|---|---|
| `retry-after` / `ratelimit-reset` may differ by 3 s | `lib/harness.mjs` `APPROX_HEADERS` | every step |
| `date`, `content-length`, `connection`, `keep-alive`, `transfer-encoding` ignored | `IGNORED_NAMES` | every step |
| Generated ids, near-now timestamps, JWT claims normalised to placeholders | `lib/normalize.mjs` | every step |
| WebSocket 101 headers reduced to `Upgrade` and the subprotocol echo | `Server.ws` | PTT handshakes |
| `details` dropped from error bodies | `run --ignore-details` | opt-in, only for the development-mode run |
| SSE heartbeat recorded as "a ping arrived 13-17.5 s after the retry hint" | `heartbeat` | incident/sse |

Run `run.mjs` without flags and with `KW_NODE_ENV=production` for the gate that must be 0.

## Beyond the scenario harness

* `services.sh relay`: two processes on one Redis (ts+ts, rs+rs, ts+rs, rs+ts). The PTT transcript (presence, grant, deny,
  audio fan-out exactly once, cross-instance admin preemption, revoke/reset, release, disconnect) is identical for all four
  pairings, so the Rust relay and the TS relay interoperate on the same channels.
* `services.sh race`: ten rounds of eight parallel refreshes of one token: one winner (grace replays carry the same
  tokens), the others get 429 `RATE_LIMIT_EXCEEDED`, and the follow-up refresh works, on both servers.
* `/health/ready` with Redis stopped, with MariaDB stopped, and after both return: same 503 envelope, both recover
  (checked by hand, see "Approximations" for the timing difference).
