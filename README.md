# Kawalo Mobile API (Kawalo-API-V2.0)

Backend REST + WebSocket + SSE untuk aplikasi Kawalo Mobile, ditulis dengan **Rust** (axum, sqlx/MariaDB, redis-rs).
Kontraknya identik dengan versi TypeScript awal (ElysiaJS + Prisma; belum pernah dideploy, tersimpan di cabang `legacy/typescript`), jadi aplikasi Flutter tidak berubah.
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
dari `.env.dev` / `.env`. Server Rust tidak memakai Prisma Client (`schema.prisma` tanpa blok `generator`, folder `generated/` sudah dihapus; `prisma generate` tidak dipakai).

Migrasi baru: tulis SQL di `prisma/migrations/YYYYMMDDHHMMSS_deskripsi/migration.sql`, lalu `npm run prisma:migrate:deploy`.

Opsi (belum dikerjakan): memindahkan migrasi ke `sqlx migrate` akan menghilangkan kebutuhan Node sama sekali, tetapi
`_prisma_migrations` perlu dipetakan ke `_sqlx_migrations` (checksum berbeda) sehingga butuh langkah baseline di tiap database.

## Password: Argon2id dengan upgrade transparan

`tkuser.Password` (VARCHAR(64)) masih berisi plaintext karena sistem lain mungkin membaca dan **mengubahnya**; API tidak pernah mengubah,
menimpa, atau mengosongkan kolom itu. Migrasi `20261110000000_add_user_password_hash` menambah kolom `tkuser.PasswordHash`
(`VARCHAR(255) NULL`, `ADD COLUMN IF NOT EXISTS`, jadi aman diulang) berisi string PHC `$argon2id$v=19$m=19456,t=2,p=1$...`
(parameter minimum OWASP; string PHC menyimpan parameternya, jadi menaikkan parameter nanti tidak merusak hash lama).

**Selama `Password` terisi, plaintext tetap sumber kebenaran dan hash hanya bayangannya.** `POST /auth/login` (kontrak luar tidak berubah)
memutuskan begini (`src/features/auth/password_check.rs`):

| Hash cocok | Plaintext `Password` | Hasil |
| :--- | :--- | :--- |
| ya | kosong | terima (fase 2: hash saja) |
| ya | cocok | terima |
| ya | terisi tetapi beda | **tolak**: hash basi (password sudah diganti di sistem lain) |
| tidak / `PasswordHash` kosong | cocok | terima, lalu hitung ulang dan simpan hash (`UPDATE ... WHERE UserId = ? AND BINARY Password = <yang dibandingkan>`) |
| tidak / `PasswordHash` kosong | beda atau kosong | tolak |

Penolakan selalu 401 yang sama dan dihitung oleh pembatas login/penguncian seperti biasa. Kondisi `BINARY Password = ...` mencegah hash basi bila password
diganti di sela-sela baca dan tulis. Gagal menyimpan hash hanya menulis peringatan di log; login tetap berhasil. Hash yang bukan string PHC valid diperlakukan
sebagai tidak cocok (dan diganti saat plaintext cocok). Kolom `Password` tidak pernah ditulis.

Biaya: satu verifikasi Argon2id (~19 MiB, puluhan ms CPU) per login selama fase 1, di thread blocking di balik semaphore sebanyak jumlah core
(pola yang sama dengan penandatanganan RSA). Setiap login gagal juga menghabiskan satu verifikasi, nyata atau dummy (hash tetap dengan parameter saat ini),
sehingga waktu respons tidak membedakan email tak dikenal dari password salah.

Dampak pada throughput login (Apple M1 Pro 10 core, `tools/bench`, `--scenarios login`, akun yang sudah ber-hash; rinci di `tools/bench/results/login-argon2id.md`):

| konkurensi | sebelum (rps / p99 ms) | sesudah (rps / p99 ms) | RSS puncak sebelum / sesudah |
| :--- | :--- | :--- | :--- |
| 1 | 218 / 6,4 | 49 / 23,5 | 12 / 31 MB |
| 16 | 1720 / 21 | 339 / 95 | 24 / 245 MB |
| 64 | 1830 / 56 | 352 / 250 | 32 / 257 MB |

Login turun sekitar 5x (satu verifikasi Argon2id ~20 ms per login) dan RSS puncak naik sekitar 19 MiB per slot hash yang aktif (maksimum = jumlah core).
Endpoint lain tidak terpengaruh.

Urutan rilis: (1) terapkan migrasi ke database (`npm run prisma:migrate:deploy`), (2) baru rilis biner baru. Biner baru men-`SELECT PasswordHash`,
jadi tanpa migrasi login mengembalikan 500. Pembatalan (bila sudah ada rilis): deploy ulang biner sebelumnya; kolom boleh dibiarkan karena biner lama tidak membacanya dan `Password` tidak pernah disentuh.

**Fase 2 (belum dikerjakan):** mengosongkan `Password` (kolom `NOT NULL`, jadi string kosong) agar hanya hash yang tersisa. Hanya aman setelah semua konsumen lain
`Password` dipastikan tidak memerlukannya **dan** setiap hash segar. Kesegaran tidak bisa diperiksa lewat SQL (hash satu arah dan tidak ada kolom sidik jari),
jadi jangan mengosongkan berdasarkan hitungan baris. Kandidat pemeriksaan kasar: `SELECT COUNT(*) AS total, COUNT(PasswordHash) AS hashed, SUM(Password <> '' AND PasswordHash IS NULL) AS belum_di_hash FROM tkuser`
(menangkap akun yang belum pernah login, bukan yang basi). Cara aman: paksa hash segar dengan skrip terpisah yang membaca `Password`, menghitung Argon2id, menulis `PasswordHash`
dalam satu transaksi dengan `WHERE ... AND BINARY Password = <yang dibaca>`, dan baru sesudahnya (setelah sistem lain berhenti mengubah `Password`) mengosongkan `Password` untuk baris yang hash-nya baru saja diverifikasi.
Mengosongkan `Password` juga menonaktifkan login dengan password lama yang salah-simpan di sistem lain, jadi lakukan di jendela perawatan.

## Struktur kode

```
src/
  main.rs            Entry: config -> logging -> RsaKeyManager -> MariaDB -> Redis -> serve -> graceful shutdown
  lib.rs, app.rs     build_app: health, JWKS, WebSocket PTT, lalu /api/v1/<fitur>
  bin/rotate_key.rs  Rotasi kunci RSA
  core/              Lintas fitur: config, db, redis, error (ApiError), response (envelope), security (RSA, token,
                     session version), middleware (CORS, rate limit, request log), extract (auth, json, query), pagination, clean
  features/<nama>/   auth | group | guest | information | attendance | operational | schedule | incident | ptt | uploads
prisma/              schema.prisma + migrations/ (sumber kebenaran skema)
scripts/             dev-services.sh (MariaDB + Redis scratch), seed.sql
tests/               Integrasi per fitur (+ interop/ untuk skrip TS pembanding)
examples/            Micro-benchmark auth (auth_bench, auth_profile)
tools/               parity/ (paritas dengan server TS) dan bench/ (benchmark)
certs/               Kunci RS256 (jangan di-commit)
```

Pola handler: `ValidatedQuery` -> `CurrentUser`/`AuthedJson` -> `State`. Respons selalu lewat envelope
`{success, message, data}` atau `{success:false, message, error_code?, details?}`; kode error mengikuti versi TypeScript.

Body request dideskripsikan satu kali per DTO lewat `impl Body` (`Schema` dengan `Field`/`Kind`, lihat `src/core/extract/schema.rs`);
skema itu dipakai `ValidatedJson`/`AuthedJson` dan menghasilkan teks `details` 422 yang sama dengan TypeBox di non-production
(di production `details` tidak dikirim). Query dan path memakai `Validate`. Menambah DTO body: tulis `Schema`, tambahkan tipenya ke
`tools/parity/typebox-corpus.mts` dan `src/features/typebox_corpus.rs`, lalu regenerasi `tests/fixtures/typebox_details.json`.

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
| `UPLOAD_DIR` | `./uploads` | Akar penyimpanan unggahan; di produksi harus volume persisten |
| `PUBLIC_BASE_URL` | kosong | Origin URL unggahan tanpa slash akhir (mis. `https://api.contoh.id`). Kosong: diturunkan dari header `Host` dengan skema `http` (hanya untuk dev) |
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
| schedule | `GET /schedules/me?from=YYYY-MM-DD&to=YYYY-MM-DD` (jadwal shift milik sendiri; default hari ini + 30 hari, maks 62 hari; tidak ada di versi TypeScript) |
| incident | `/incidents`: `GET /`, `POST /`, `GET /:id`, `GET /:id/stream` (SSE), `POST /:id/replies`, `PATCH /:id/resolve`, `DELETE /:id` |
| ptt | WebSocket `/ws/ptt/:groupUid` (di luar `/api/v1`) |
| ptt recordings | `/ptt/recordings`: `GET /?limit=&before=`, `GET /:id/audio`, `DELETE /:id` (hanya `allow_admin`) |
| uploads | `POST /uploads` (multipart, perlu token); `GET /uploads/{yyyy}/{mm}/{uuid}.{ext}` publik, di luar `/api/v1` |

### Riwayat rekaman PTT (`ptt/recordings`)

Fitur baru, tidak ada di versi TypeScript. Selalu aktif (tanpa flag): tiap giliran bicara (dari lantai diberikan sampai dilepas, dicabut/diambil alih admin,
`TOT_DURATION` 30 detik habis, atau soket pembicara putus) direkam di instance yang memegang soket pembicara: payload 640 byte (PCM 16 kHz mono 16-bit, 20 ms) di-encode
ke Opus (VoIP, 24 kbps) dan dikemas Ogg/Opus; payload dengan panjang lain dianggap sudah Opus dan dikemas apa adanya. Berkas ditulis atomik ke
`UPLOAD_DIR/ptt/{yyyy}/{mm}/{uuid}.opus` (di luar rute publik `/uploads`), lalu satu baris `tkpttrecording` (migrasi `20261111010000_add_ptt_recording`).
Giliran tanpa audio tidak disimpan. Jalur relay tidak berubah: frame masuk ke kanal terbatas lewat `try_send` dan dibuang (dicatat) bila penuh. Dengan flag mati, perilaku
sama persis seperti sebelumnya dan tidak ada perekam yang dibuat.

Semua endpoint memakai `Authorization: Bearer` (termasuk audio, jadi pemutar harus mengirim header itu), dibatasi ke `group_uid` token, dan mengabaikan baris yang sudah dihapus.
Peran `ADMIN`, `GUARD`, dan `USER` boleh melihat dan memutar. `SUPER_ADMIN` tidak lintas grup di sini (sama seperti fitur REST lain: grup aktif dari token).

* `GET /api/v1/ptt/recordings?limit=&before=`: terbaru dulu. `limit` 1 sampai 100 (default 20), `before` = kursor buram dari `next_before`; nilai tidak valid `422 VALIDATION_ERROR`.
  Token tanpa grup aktif `400`; tanpa token `401`.

  ```json
  {"success":true,"message":"Riwayat rekaman PTT berhasil diambil","data":{
    "items":[{"id":"rec_<uuid v7>","speaker":{"id":"<sub token>","name":"Budi","role":"GUARD","email":"budi@contoh.id"},
              "started_at":"2026-11-11T08:15:30.120Z","duration_ms":4820,"size_bytes":10432,
              "audio_path":"/api/v1/ptt/recordings/rec_<uuid v7>/audio"}],
    "next_before":"<kursor>"}}
  ```

  `next_before` berisi kursor untuk halaman yang lebih lama, atau `null` bila itu halaman terakhir. Urutan `started_at` lalu `id` menurun; waktu mulai = saat lantai diberikan.
* `GET /api/v1/ptt/recordings/{id}/audio`: berkas Ogg/Opus, `Content-Type: audio/ogg`, `Accept-Ranges: bytes`, `Cache-Control: private`. Mendukung satu `Range: bytes=a-b|a-|-n`
  (`206` dengan `Content-Range`; `416` dengan `Content-Range: bytes */total` bila di luar berkas; header Range lain diabaikan dan berkas utuh dikirim). Rekaman grup lain, yang tidak ada, sudah
  dihapus, atau yang berkasnya hilang `404 RESOURCE_NOT_FOUND` ("Rekaman tidak ditemukan."); tanpa token `401`. Berkas dicari hanya dari baris DB dan jalurnya harus tetap di bawah `UPLOAD_DIR`.
* `DELETE /api/v1/ptt/recordings/{id}`: hanya `allow_admin` (aturan yang sama dengan `DELETE /incidents/:id`). Urutan cek: grup aktif (`400`), bukan admin `403 AUTH_FORBIDDEN`
  ("Hanya administrator yang dapat menghapus rekaman."), tidak ada/grup lain/sudah dihapus `404` (jadi hapus kedua `404`), tanpa token `401`. Sukses `200`
  `{"success":true,"message":"Rekaman berhasil dihapus","data":null}`: mengisi `DeletedAt` (baris tetap sebagai nisan) lalu menghapus berkas dengan upaya terbaik (gagal hanya dicatat `warn`).
  Tidak ada hapus massal, pemulihan, retensi otomatis, atau kuota disk.

### Unggah berkas (`uploads`)

Fitur baru, tidak ada di versi TypeScript. `POST /api/v1/uploads`, `multipart/form-data`, semua peran. Field `file` (wajib) dan `kind`
(`image` atau `document`, wajib). Sukses `201`:

```json
{"success":true,"message":"Berkas berhasil diunggah","data":{"url":"https://api.contoh.id/uploads/2026/11/<uuid v4>.webp","kind":"image","size":48213,"content_type":"image/webp","name":"foto.webp"}}
```

`name` hanya untuk tampilan (nama asli yang sudah disanitasi); nama simpan selalu UUID v4 acak dengan ekstensi hasil deteksi isi.

* **Gambar**: harus WebP sungguhan (header `RIFF`/`WEBP`, ukuran RIFF konsisten, chunk pertama VP8/VP8L/VP8X); content-type dan ekstensi dari klien diabaikan.
  Ukuran maksimal 150.000 byte (di bawah 150 KB); kompresi (WebP, kualitas 90-100) dikerjakan aplikasi, server hanya memvalidasi.
* **Dokumen**: ekstensi `pdf`, `doc`, `docx`, `xls`, `xlsx`, `txt` (ekstensi dari nama berkas) dan isinya harus cocok (`%PDF-`, zip `PK`, OLE, UTF-8 tanpa NUL). Maksimal 10 MB.
* Galat memakai envelope standar: tipe salah, isi tidak cocok, field hilang, atau bukan multipart `422 VALIDATION_ERROR`; terlalu besar `413 BAD_REQUEST`
  ("Ukuran gambar maksimal 150 KB." / "Ukuran dokumen maksimal 10 MB."); tanpa/dengan token tidak valid `401`; lebih dari 30 unggahan per pengguna per menit
  `429 RATE_LIMIT_EXCEEDED` dengan `Retry-After` (selain rate limit IP global).
* `GET /uploads/{yyyy}/{mm}/{uuid}.{ext}` publik (URL tak bisa ditebak), mendukung `HEAD`, `Cache-Control: public, max-age=31536000, immutable`,
  `Content-Disposition: inline` untuk gambar dan `attachment` untuk dokumen. Path selain pola yang dikenal menghasilkan `404`. Tanpa dukungan `Range`.
* Tidak ada metadata di database dan tidak ada migrasi. Berkas yatim (diunggah tetapi tidak pernah dirujuk) tidak dibersihkan otomatis (di luar cakupan).

## Paritas dan benchmark

`tools/parity` memutar skenario yang sama ke server TypeScript (acuan) dan Rust, lalu membandingkan status, header kontrak, dan body
JSON (juga transkrip SSE/WebSocket). Gerbang rilis: 0 beda pada run penuh dengan `KW_NODE_ENV=production` (1424 langkah). Perbedaan yang
disengaja tercatat di `tools/parity/KNOWN_DIFFERENCES.md`. `tools/bench` mengukur throughput, latensi, dan memori kedua server pada data
yang sama. Keduanya butuh checkout `legacy/typescript` sebagai oracle; cara menyiapkannya ada di [`tools/README.md`](tools/README.md).

### Hasil: TypeScript (Elysia) vs Rust

Apple M1 Pro (10 core, 16 GB), Node 24, MariaDB 12, Redis 8. `oha` closed-loop dengan koneksi keep-alive, 8 sampai 10 detik per run
setelah warm-up, median 3 run, error 0%. Kedua server memakai MariaDB dan Redis yang sama, pool 10 koneksi, dan data uji sintetis
(50 ribu tamu, 20 ribu feed, 100 ribu absensi), dijalankan bergantian.

**Throughput (request/detik) dan latensi p99 (ms), TS lawan Rust**

| Skenario | Konkurensi | TypeScript | Rust | Rasio | p99 TS | p99 Rust |
|---|---|---|---|---|---|---|
| Daftar tamu | 1 | 841 | 2.055 | 2,4x | 2,39 | 0,74 |
| | 16 | 1.639 | 9.966 | 6,1x | 13,55 | 2,51 |
| | 64 | 1.549 | 9.862 | 6,4x | 50,86 | 9,17 |
| Daftar absensi | 1 | 928 | 1.950 | 2,1x | 2,01 | 0,69 |
| | 16 | 1.766 | 8.006 | 4,5x | 12,08 | 2,82 |
| | 64 | 1.647 | 7.935 | 4,8x | 49,24 | 10,97 |
| Detail insiden | 1 | 1.360 | 3.732 | 2,7x | 1,37 | 0,39 |
| | 16 | 3.526 | 16.118 | 4,6x | 6,96 | 1,42 |
| | 64 | 3.373 | 16.082 | 4,8x | 23,84 | 5,21 |
| Login* | 1 | 178 | 49 | 0,28x | 7,50 | 23,5 |
| | 16 | 1.107 | 339 | 0,31x | 16,37 | 95 |
| | 64 | 1.110 | 352 | 0,32x | 62,95 | 250 |

\* **Login memang lebih lambat di Rust, dan itu disengaja.** TypeScript membandingkan password sebagai plaintext (tanpa hashing),
sedangkan Rust kini memverifikasi hash Argon2id (sekitar 20 ms per login) demi keamanan. Sebelum Argon2id ditambahkan, login Rust
(hanya tanda tangan RS256) mencapai 214, 2.011, dan 2.071 rps pada konkurensi 1, 16, dan 64, yaitu 1,2x sampai 1,9x dari TypeScript.
Angka Rust di baris ini diukur di run terpisah dari angka TypeScript (beban mesin berbeda), jadi bandingkan sebagai perkiraan.
Rincian ada di bagian "Password: Argon2id" dan `tools/bench/results/login-argon2id.md`.

Skenario lain, dari run pertama (Rust sebelum optimasi jalur auth; belum diukur ulang, jadi angka Rust kemungkinan lebih baik sekarang),
pada konkurensi 64:

| Skenario | TypeScript | Rust | Rasio |
|---|---|---|---|
| `GET /health` | 30.337 | 83.045 | 2,7x |
| `GET /.well-known/jwks.json` | 28.742 | 79.822 | 2,8x |
| Daftar informasi | 1.449 | 5.412 | 3,7x |
| Daftar operasional (7 hari) | 359 | 2.012 | 5,6x |
| Buat insiden | 2.834 | 9.564 | 3,4x |

**Memori dan ukuran**

| | TypeScript | Rust |
|---|---|---|
| RSS idle | 57 MB | 5 sampai 11 MB |
| RSS puncak saat beban | 160 sampai 430 MB | 7 sampai 48 MB |
| Artefak deploy | sekitar 375 MB (`dist` + `node_modules`) | 8,4 MB (satu biner) |
| Waktu start sampai `/health` | 425 ms | 23 ms |

**PTT WebSocket** (1 pembicara, N-1 pendengar, 50 frame/detik, 28 detik): pada 256 klien p99 5,15 ms (TS) lawan 4,25 ms (Rust), tanpa
frame hilang di keduanya, memori puncak 132 MB lawan 48 MB. Pemakaian CPU Rust justru lebih tinggi pada skenario ini (23% lawan 12%), jadi
keunggulannya ada di memori, bukan CPU.

**Yang membuat Rust lebih cepat di jalur terautentikasi:** verifikasi JWT RS256 memakai `aws-lc-rs` dengan cache hasil verifikasi sampai
`exp` (cek `session_ver` ke Redis tetap tiap request), alokator `mimalloc`, dan pool sqlx yang tidak melakukan ping pada tiap acquire.
Backend RSA murni-Rust yang dipakai pada versi awal justru membuat login dan request terautentikasi lebih lambat dari TypeScript. Endpoint selain login tidak terpengaruh oleh Argon2id.

**Cara membaca angka ini:** ini perbandingan relatif pada satu mesin, bukan angka produksi absolut. Generator beban, server, MariaDB, dan
Redis berbagi mesin tanpa TLS atau proxy. Beban CPU lain di mesin (IDE, server dev) ikut memengaruhi hasil. Loop tertutup membuat latensi
terlihat lebih baik saat server jenuh, jadi bandingkan hanya pada konkurensi yang sama. Data lengkap per ulangan, p50/p90/max, CPU, dan
RSS ada di `tools/bench/results/` (`ts-vs-rust`, `perf-ts-vs-rust-after`, `perf-login-final`), dan cara mengulang ada di
[`tools/bench/README.md`](tools/bench/README.md).

## Deploy

* Satu biner: `cargo build --release`, lalu jalankan `target/release/kawalo-api`. Tidak perlu Node, Prisma Client, atau `node_modules` saat berjalan.
* Working directory harus berisi `certs/` (biner membaca `./certs` relatif cwd, dan file `.env.*` bila ada). Pasang `certs/` (kunci RS256) di server dengan izin 0600; jangan ikut dimasukkan ke image atau git.
* **Zona waktu**: set `TZ` secara eksplisit (mis. `Asia/Jakarta`, zona data operasional). Fitur `operational` mengelompokkan dan memformat waktu menurut zona proses, sehingga hasilnya berbeda bila `TZ` berbeda.
* `TRUSTED_PROXY_CIDRS` harus mencakup proxy produksi; bila tidak, `X-Forwarded-For` diabaikan dan rate limit serta log memandang semua klien sebagai satu IP.
* Jalankan migrasi (`npm run prisma:migrate:deploy`) sebelum merilis versi yang membutuhkannya. Migrasi `tkincident*` harus sudah ada di tiap database, kalau tidak endpoint `/incidents` mengembalikan 500; begitu pula `20261110000000_add_user_password_hash` (kolom `tkuser.PasswordHash`), kalau tidak login mengembalikan 500.
* **Unggahan**: `UPLOAD_DIR` harus volume persisten (di Docker: `/app/uploads`, dimiliki user `kawalo`), dicadangkan, dan dipantau kuotanya; berkas tidak pernah dihapus otomatis. Set `PUBLIC_BASE_URL` ke origin publik (https); bila kosong URL mengikuti header `Host`. Bila ada beberapa instance, semuanya harus berbagi volume yang sama.
* Liveness `GET /health`, readiness `GET /health/ready`. Pada `SIGTERM`/`SIGINT` server menutup koneksi dengan tenggang 10 detik.
