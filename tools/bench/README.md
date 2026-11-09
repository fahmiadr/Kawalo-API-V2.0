# Benchmark harness: TypeScript vs Rust

Mengukur server TS (acuan) dan Rust pada data, DB, Redis, dan konfigurasi yang sama persis.
Semua angka di sini harus bisa direproduksi dengan perintah di bawah.

## Prasyarat

`oha`, `mariadbd`, `redis-server`, Node >= 20, biner Rust (`cargo build --release`), dan checkout oracle TypeScript di `$KW_TS_ROOT` (default `../Kawalo-API-V2.0-ts`, cara menyiapkannya di [`tools/README.md`](../README.md)); token dibuat dengan `tsx` + `src/core/security` milik checkout itu.
Semua perintah dijalankan dari root `Kawalo-API-V2.0/`.

## Langkah

```bash
# 1. DB scratch (MariaDB :3392, datadir /tmp/kw-bench-data) + Redis :6393. Aman: terpisah dari instans lain.
tools/bench/services.sh start            # stop | reset (hapus datadir, mulai ulang)
node tools/bench/seed.mjs                # 50k tamu, 20k feed, 100k absensi, 60k duty, 150 user, 3 grup (--scale 0.1 untuk kecil)

# 2. Build & jalankan server. serve.mjs memberi env yang identik (bench-env.mjs) dan mencatat pid + waktu start.
export KW_TS_ROOT=$PWD/../Kawalo-API-V2.0-ts
(cd "$KW_TS_ROOT" && node scripts/build.mjs)   # TS: bundle dist/server.js (fallback: --cmd "$KW_TS_ROOT/node_modules/.bin/tsx --tsconfig $KW_TS_ROOT/tsconfig.json $KW_TS_ROOT/src/server.ts")
node tools/bench/serve.mjs start --name a --port 3100 --cmd "node $KW_TS_ROOT/dist/server.js"
node tools/bench/serve.mjs start --name b --port 3101 --cmd "target/release/kawalo-api"

# 3. Ukur. pid dicetak serve.mjs (atau /tmp/kw-bench-run/{a,b}.json)
node tools/bench/run.mjs --a http://127.0.0.1:3100 --pid-a <pid> --only a \
     --out tools/bench/results/baseline-ts.json                       # TS saja
node tools/bench/run.mjs --a http://127.0.0.1:3100 --pid-a <pidA> --b http://127.0.0.1:3101 --pid-b <pidB> \
     --artifact-b target/release/kawalo-api                     # TS vs Rust (default out: results/<tanggal>.json|md)

node tools/bench/serve.mjs stop --name a
tools/bench/services.sh stop
```

Opsi `run.mjs`: `--duration 10` (dtk per run), `--warmup 5`, `--reps 3`, `--conc 1,16,64,256`,
`--scenarios health,jwks,guests,information,attendance,operational,incident-get,incident-post,login,ptt`
(`operational-all` hanya bila disebut), `--ptt-clients 50,256`, `--ptt-seconds 28`, `--only a|b`, `--label-a/-b`.
Satu run penuh (9 skenario HTTP x 4 konkurensi x 3 ulangan + PTT) sekitar 25 menit per server. Untuk uji cepat:
`--duration 2 --warmup 1 --reps 1 --conc 1,16`.

Server B harus: membaca `PORT`, `DATABASE_URL` (termasuk `connection_limit=10` untuk ukuran pool), `REDIS_URL`, `RATE_LIMIT_*`,
membaca `certs/` relatif terhadap cwd (serve.mjs menjalankan dari root API), dan memakai token/kunci yang sama
(token dibuat ulang tiap `run.mjs` dengan kunci `certs/`, kedaluwarsa 7 hari).

## Skenario

| Nama | Request | Yang diuji |
|---|---|---|
| health | `GET /health` | overhead dasar HTTP + serialisasi |
| jwks | `GET /.well-known/jwks.json` | respons statis dari cache |
| guests / information / attendance | `GET /api/v1/<x>?limit=20` | JWT RS256 verify + cek `session_ver` Redis + query MariaDB (grup `bench-g1`) |
| operational | `GET /api/v1/operational?limit=20&start_date=<7 hari lalu>` | idem, query GROUP BY duty journey. Tanpa `start_date` (`operational-all`) query memindai seluruh grup (~170 ms, terikat DB) |
| incident-get | `GET /api/v1/incidents/:id` | detail + 10 balasan |
| incident-post | `POST /api/v1/incidents` | tulis ke MariaDB (tabel dikosongkan sebelum tiap run) |
| login | `POST /api/v1/auth/login` | lookup user + grup, cek Redis, tanda tangan RS256, tulis sesi refresh (kunci Redis dibersihkan sebelum tiap run) |
| ptt | WebSocket `/ws/ptt/bench-g1` | 1 pembicara + N-1 pendengar, frame biner 67 B (header 7 B + payload 60 B) @50 fps |

Konkurensi 1/16/64/256 = jumlah koneksi keep-alive oha (loop tertutup). PTT memakai `--ptt-clients` (default 50 dan 256 klien).

## Membaca hasil

- `results/<nama>.json`: tiap skenario x konkurensi menyimpan semua ulangan (`servers.<id>.reps`) dan `median`. Tabel Markdown (`.md`) hanya memuat median.
- `rps`, `p50/p90/p99/max` (ms), `err %` (non-2xx + error koneksi), `RSS peak/avg` (MB, `ps` tiap 500 ms selama run), `CPU %` (selisih waktu CPU proses / waktu dinding; 100 = satu core penuh), `rpsPerCpuCore` (rps per core terpakai: ukuran efisiensi, bukan sekadar kecepatan).
- Footprint proses: waktu start sampai `/health` pertama sukses (`serve.mjs`), RSS idle (3 dtk pertama setelah start, sebelum ada trafik), RSS setelah semua run, ukuran artefak (TS: `dist` + `node_modules` produksi; Rust: biner).
- PTT: latensi satu arah pembicara ke pendengar (p50/p90/p99/max), `loss %` = frame yang tidak sampai ke pendengar / (pendengar x frame terkirim), `delivered/s`, waktu connect seluruh klien, waktu `FLOOR_REQUEST` ke `FLOOR_GRANTED`.
- Bandingkan dua server hanya pada konkurensi yang sama. Rasio `rps B/A` > 1 berarti B lebih cepat; `p99 A/B` > 1 berarti B punya ekor lebih pendek.

## Batasan metodologi

- Generator beban, server, MariaDB, dan Redis berbagi satu mesin (loopback, tanpa TLS/proxy). Beban CPU lain di mesin (build, indexing, agen lain) mengganggu hasil; `run.mjs` mencatat load average awal/akhir. Jalankan saat mesin tenang.
- Loop tertutup (coordinated omission): saat server jenuh, latensi terlihat lebih baik daripada yang dialami klien nyata. Bandingkan pada konkurensi sama, jangan membaca p99 sebagai SLA.
- Rate limiter global diset sangat tinggi lewat env (bukan dimatikan): biaya pengecekannya tetap ikut terukur di TS, jadi server Rust harus menjalankan limiter yang setara.
- Login membandingkan kata sandi secara plaintext (perilaku TS saat ini, lihat `login.usecase.ts`), jadi yang terukur adalah lookup DB + Redis + tanda tangan RS256, bukan hashing kata sandi. Bila Rust menambah hashing, angkanya bukan lagi apel dengan apel.
- Env diberikan eksplisit lewat `bench-env.mjs` karena server TS membaca `.env.prod` bila ada; `LOG_LEVEL=warn` untuk kedua server; pool DB 10 koneksi.
- Pembicara PTT bicara 28 dtk (bukan 30) karena time-out timer server mencabut lantai tepat 30 dtk. Latensi PTT memakai `process.hrtime` di satu proses (pendengar di worker thread) sehingga bebas selisih jam, tetapi masih termasuk jitter event loop klien (sub-milidetik hingga beberapa ms).
- Waktu CPU dari `ps` beresolusi 10 ms; sampling RSS 500 ms bisa melewatkan lonjakan singkat (puncak sebenarnya >= puncak terukur). RSS macOS bukan angka yang sama dengan RSS Linux; bandingkan hanya di mesin yang sama.
- Data sintetis (bukan `data.sql`): 70% baris ada di grup `bench-g1`, tempat semua token uji berada. Pola akses selalu halaman pertama, tanpa pencarian.
- Warm-up 5 dtk dibuang sekali per (server, skenario, konkurensi); ulangan berikutnya tidak di-warm-up ulang. A dan B bergantian per ulangan agar drift mesin terbagi rata.
- "RSS setelah semua run" bisa lebih kecil dari RSS idle: macOS mengompresi/mengembalikan halaman. Yang bermakna adalah puncak dan rata-rata saat beban.
