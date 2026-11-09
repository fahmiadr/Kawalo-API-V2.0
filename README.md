# Kawalo Mobile API (Kawalo-API-V2.0)

Backend REST + WebSocket + SSE untuk aplikasi Kawalo Mobile, ditulis dengan **Rust** (axum, sqlx/MariaDB, redis-rs).
Kontraknya identik dengan versi TypeScript sebelumnya (ElysiaJS + Prisma), jadi aplikasi Flutter tidak berubah.
Kode TypeScript asli ada di cabang `legacy/typescript`.

## Prasyarat

* **Rust** stable (edition 2024, diuji dengan 1.99): <https://rustup.rs>
* **MariaDB** `10.5+` dan **Redis** `6.0+`
* **Node.js** `>= 20.19.0` hanya bila perlu menjalankan migrasi Prisma atau `tools/` (paritas, benchmark)

## Memulai

```bash
cp .env.example .env.dev      # sesuaikan DATABASE_URL dan REDIS_URL
cargo run                     # http://localhost:3000, membaca certs/ dari working directory
```

Skema database dikelola lewat `prisma/migrations/*/migration.sql` (lihat "Migrasi database"). Server membaca kunci RS256
dari `certs/` (`private.key`, `public.key`, `active_kid.txt`, arsip di `certs/archived/`) dan membuat pasangan baru bila belum ada.

## Perintah

| Perintah | Deskripsi |
| :--- | :--- |
| `cargo run` | Menjalankan server (development) |
| `cargo build --release` | Build produksi: `target/release/kawalo-api` |
| `cargo fmt --check` / `cargo clippy --all-targets -- -D warnings` | Format dan lint (harus bersih) |
| `eval "$(scripts/dev-services.sh start)" && cargo test` | Test unit + integrasi dengan MariaDB dan Redis scratch |
| `scripts/dev-services.sh stop` / `reset` / `env` | Matikan, reset DB + Redis, atau cetak `DATABASE_URL`/`REDIS_URL` |
| `cargo run --release --bin rotate_key` | Rotasi pasangan kunci RSA 4096-bit di `./certs` (kunci lama diarsipkan, server yang berjalan memuat ulang otomatis) |
| `npm ci` lalu `npm run prisma:migrate:deploy` | Menerapkan migrasi SQL ke database |
| `npm run prisma:migrate:status` | Status migrasi pada database |
| `npm run prisma:validate` | Validasi `prisma/schema.prisma` |

`scripts/dev-services.sh` menjalankan MariaDB dan Redis scratch di `/tmp` pada port non-default (3380 / 6390), terpisah dari
instans milik Anda, lalu menerapkan seluruh migrasi dan `scripts/seed.sql`. Port dan nama dapat diganti lewat
`KW_RUST_BASE`, `KW_RUST_DB_SOCK`, `KW_RUST_DB_PORT`, `KW_RUST_REDIS_PORT`, `KW_RUST_DB_NAME`. Test memakai grup dan ID unik
per test, sehingga aman berjalan paralel. `tests/token_interop.rs` hanya berjalan bila oracle TypeScript tersedia (lihat `tools/README.md`);
selain itu dilewati.

## Migrasi database

`prisma/` (schema + migrasi SQL) adalah sumber kebenaran skema, dan tabel `_prisma_migrations` di database nyata sudah
terisi. Prisma kini hanya alat migrasi: `package.json` hanya berisi `prisma` dan `dotenv`, dan `prisma.config.ts` membaca `DATABASE_URL`
dari `.env.dev` / `.env`. Server Rust tidak memakai Prisma Client (folder `generated/` sudah dihapus, jadi jangan menjalankan `prisma generate`).

Migrasi baru: tulis SQL di `prisma/migrations/YYYYMMDDHHMMSS_deskripsi/migration.sql`, lalu `npm run prisma:migrate:deploy`.

Opsi (belum dikerjakan): memindahkan migrasi ke `sqlx migrate` akan menghilangkan kebutuhan Node sama sekali, tetapi
`_prisma_migrations` perlu dipetakan ke `_sqlx_migrations` (checksum berbeda) sehingga butuh langkah baseline di tiap database.

## Struktur kode

```
src/
  main.rs            Entry: config -> logging -> RsaKeyManager -> MariaDB -> Redis -> serve -> graceful shutdown
  lib.rs, app.rs     build_app: health, JWKS, WebSocket PTT, lalu /api/v1/<fitur>
  bin/rotate_key.rs  Rotasi kunci RSA
  core/              Lintas fitur: config, db, redis, error (ApiError), response (envelope), security (RSA, token,
                     session version), middleware (CORS, rate limit, request log), extract (auth, json, query), pagination, clean
  features/<nama>/   auth | group | guest | information | attendance | operational | incident | ptt
prisma/              schema.prisma + migrations/ (sumber kebenaran skema)
scripts/             dev-services.sh (MariaDB + Redis scratch), seed.sql
tests/               Integrasi per fitur (+ interop/ untuk skrip TS pembanding)
examples/            Micro-benchmark auth (auth_bench, auth_profile)
tools/               parity/ (paritas dengan server TS) dan bench/ (benchmark)
certs/               Kunci RS256 (jangan di-commit)
```

Pola handler: `ValidatedQuery` -> `CurrentUser`/`AuthedJson` -> `State`. Respons selalu lewat envelope
`{success, message, data}` atau `{success:false, message, error_code?, details?}`; kode error mengikuti versi TypeScript.

## Variabel lingkungan

Dimuat dari `.env.dev|staging|prod` (sesuai `NODE_ENV`) lalu `.env` di working directory; variabel proses tidak ditimpa. Template: `.env.example`.

| Kunci | Default | Keterangan |
| :--- | :--- | :--- |
| `NODE_ENV` | `development` | `production` menyembunyikan `details` pada error 422 dan memakai log JSON |
| `PORT` | `3000` | |
| `LOG_LEVEL` | `info` (production) / `debug` | Nama pino; `RUST_LOG` bila diset mengambil alih |
| `DATABASE_URL` | `mysql://root:@localhost:3306/kawalo` | Parameter `connection_limit` (pool, default 10) dan `connect_timeout` (ms, default 5000) |
| `DATABASE_USE_TEXT_PROTOCOL` | `false` | Dibaca demi paritas, tanpa efek pada sqlx |
| `REDIS_URL` | `redis://localhost:6379` | |
| `JWT_EXPIRES_IN` | `900` | Detik |
| `REFRESH_TOKEN_EXPIRES_IN` | `1209600` | Detik (14 hari) |
| `CORS_ORIGIN` | `*` | |
| `RATE_LIMIT_MAX` / `RATE_LIMIT_DURATION` | `100` / `60000` | Rate limit global per IP; durasi dalam ms |
| `TRUSTED_PROXY_CIDRS` | loopback + jaringan privat | Daftar CIDR proxy yang dipercaya untuk `X-Forwarded-For` |
| `LOGIN_RATE_LIMIT_IP_MAX`, `LOGIN_RATE_LIMIT_DURATION`, `LOGIN_MAX_FAILED_ATTEMPTS`, `LOGIN_LOCKOUT_DURATION` | `10`, `900`, `5`, `900` | Throttling login (detik) |

## Endpoint ringkas

Semua di bawah `/api/v1`, kecuali tiga di luar prefix.

| Fitur | Endpoint |
| :--- | :--- |
| Sistem | `GET /health`, `GET /health/ready`, `GET /.well-known/jwks.json` |
| auth | `POST /auth/login`, `/auth/refresh`, `/auth/switch-group`, `/auth/logout` |
| group | `GET /groups/members` |
| guest | `/guests`: `GET /master-types`, `GET /`, `GET /:id`, `POST /`, `PATCH /:id/acknowledge`, `PATCH /:id/checkout`, `PUT/PATCH/DELETE /:id`, `DELETE /` (batch) |
| information | `/information`: GET, POST, PUT, PATCH, DELETE (tunggal dan batch) |
| attendance | `GET /attendance` (read-only) |
| operational | `GET /operational` (read-only) |
| incident | `/incidents`: `GET /`, `POST /`, `GET /:id`, `GET /:id/stream` (SSE), `POST /:id/replies`, `PATCH /:id/resolve`, `DELETE /:id` |
| ptt | WebSocket `/ws/ptt/:groupUid` (di luar `/api/v1`) |

## Paritas dan benchmark

`tools/parity` memutar skenario yang sama ke server TypeScript (acuan) dan Rust, lalu membandingkan status, header kontrak, dan body
JSON (juga transkrip SSE/WebSocket). Gerbang rilis: 0 beda pada run penuh dengan `KW_NODE_ENV=production`. Perbedaan yang disengaja
tercatat di `tools/parity/KNOWN_DIFFERENCES.md`. `tools/bench` mengukur throughput, latensi, dan memori kedua server pada data yang sama
(hasil di `tools/bench/results/`). Keduanya butuh checkout `legacy/typescript` sebagai oracle; cara menyiapkannya ada di
[`tools/README.md`](tools/README.md).

## Deploy

* Satu biner: `cargo build --release`, lalu jalankan `target/release/kawalo-api`. Tidak perlu Node, Prisma Client, atau `node_modules` saat berjalan.
* Working directory harus berisi `certs/` (biner membaca `./certs` relatif cwd, dan file `.env.*` bila ada). Pemasangan `certs/` di server tidak berubah dari versi TypeScript.
* **Zona waktu**: set `TZ` sama dengan lingkungan sebelumnya (mis. `Asia/Jakarta`). Fitur `operational` mengelompokkan dan memformat waktu menurut zona proses, sehingga hasilnya berbeda bila `TZ` berbeda.
* `TRUSTED_PROXY_CIDRS` harus mencakup proxy produksi; bila tidak, `X-Forwarded-For` diabaikan dan rate limit serta log memandang semua klien sebagai satu IP.
* Jalankan migrasi (`npm run prisma:migrate:deploy`) sebelum merilis versi yang membutuhkannya. Migrasi `tkincident*` harus sudah ada di tiap database, kalau tidak endpoint `/incidents` mengembalikan 500.
* Liveness `GET /health`, readiness `GET /health/ready`. Pada `SIGTERM`/`SIGINT` server menutup koneksi dengan tenggang 10 detik.
