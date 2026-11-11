# Worker transkrip PTT (`ptt_transcriber`)

Proses terpisah dari API yang mengisi transkrip **kosong** pada rekaman PTT (`tkpttrecording`) memakai
[whisper.cpp](https://github.com/ggml-org/whisper.cpp) (`whisper-cli`). Rancangan hibrida: perangkat Android 13+
pembicara biasanya mengirim teksnya sendiri (`PUT /api/v1/ptt/recordings/by-request/{request_id}/transcript`);
worker ini hanya menangani sisanya. Rancangan lengkap: `Task/progress/push-to-talk/ptt-transkrip-stt.md` (repo induk).

## Cara kerja
1. **Antrean = tabel `tkpttrecording`** (tanpa tabel job). Baris yang diambil: `DeletedAt IS NULL`, `Transcript IS NULL`,
   `TranscriptAttempts < STT_MAX_ATTEMPTS`, dan salah satu dari:
   - `TranscriptStatus IN ('unsupported','failed')` (dikirim app, atau gagal di server); bila server sudah pernah mencoba,
     percobaan berikutnya menunggu satu masa tunggu (`STT_GRACE_SECONDS`) sejak klaim terakhir;
   - `TranscriptStatus IS NULL` dan `CreatedAt` lebih tua dari `STT_GRACE_SECONDS` (app mati/offline/tidak mengirim apa pun).

   Urutan: `StartedAt` tertua dulu, satu job per waktu. Semua perbandingan waktu memakai jam database (`NOW()`).
2. **Klaim atomik** tanpa `SKIP LOCKED` (baru ada di MariaDB 10.6) dan tanpa `RETURNING` (tidak ada di MySQL): baca satu
   kandidat, lalu `UPDATE ... SET TranscriptStatus='processing', TranscriptClaimedAt=NOW(), TranscriptAttempts=TranscriptAttempts+1
   WHERE RecordingUID=? AND TranscriptAttempts=<nilai yang dibaca> AND <syarat antrean>`. Hanya satu worker yang mendapat
   `rows_affected = 1`; yang kalah mengambil kandidat berikutnya. Klaim dikenali dari pasangan `(RecordingUID, TranscriptAttempts)`,
   jadi worker yang klaimnya sudah dipulihkan tidak bisa lagi mengubah baris itu.
3. **Pemulihan klaim basi** saat start lalu tiap 60 detik: `processing` dengan `TranscriptClaimedAt` lebih tua dari
   `STT_STALE_CLAIM_SECONDS` kembali menjadi `failed` (diambil lagi), atau `error` bila percobaan habis.
4. **Proses satu rekaman:** `FilePath` harus path relatif biasa di bawah `UPLOAD_DIR` (aturan sama dengan endpoint audio; selain
   itu langsung `error`). Ogg/Opus didekode ke PCM 16 kHz mono (`ogg` + `unsafe-libopus`, tanpa ffmpeg).
   **Gerbang hening:** dihitung durasi frame 20 ms yang level RMS-nya >= `STT_SILENCE_DBFS` (bawaan -40 dBFS); bila kurang dari
   `STT_MIN_SPEECH_MS` (bawaan 1000 ms), status `no_speech` dan `whisper-cli` tidak dijalankan (Whisper berhalusinasi pada audio
   hening). Selain itu ditulis WAV sementara (direktori per job 0700, berkas 0600, selalu dihapus), lalu dijalankan
   `whisper-cli -m <model> -f <wav> -l <bahasa> -t <thread> -nt -np [--vad -vm <model VAD>]` tanpa shell, stdout dibatasi 64 KB,
   dibunuh setelah `STT_JOB_TIMEOUT_SECONDS`.
5. **Hasil:** tag non-ucapan (`[BLANK_AUDIO]`, `(musik)`, `*...*`) dibuang, spasi dirapikan, maks 2.000 karakter. Kosong = `no_speech`.
   Teks ditulis HANYA bila `Transcript` masih kosong (`TranscriptSource='server'`, `TranscriptStatus='done'`,
   `TranscriptLocale='id_ID'` untuk `id`), jadi teks perangkat yang datang belakangan tidak pernah tertimpa; sebaliknya teks perangkat
   boleh menimpa teks server.
6. **Gagal** (proses keluar non-nol, timeout, berkas hilang, dekode gagal): `failed` dan dicoba lagi, `error` setelah
   `STT_MAX_ATTEMPTS`. Bila `whisper-cli` tidak bisa dijalankan sama sekali (berkas tidak ada/izin), job dikembalikan tanpa
   menghitung percobaan dan worker menunggu `STT_POLL_SECONDS`. Error database: dicoba lagi dengan backoff 1 sampai 60 detik.
7. **SIGTERM/SIGINT:** `whisper-cli` dibunuh, klaim dikembalikan (`failed`, percobaan tidak dihitung), proses berhenti.

Log satu baris per job (`[STT] <id> attempt N: Done in 1234 ms`). Isi audio dan teks transkrip TIDAK pernah ditulis ke log
(data pribadi).

## Variabel lingkungan
Dibaca dari lingkungan proses (dan `.env.*` di working directory, sama seperti API). Di `server-114` cukup ditambahkan ke
`/etc/kawalo/kawalo.env`; API mengabaikan variabel `STT_*`.

| Variabel | Bawaan | Keterangan |
|---|---|---|
| `DATABASE_URL` | (sama dengan API) | Koneksi MySQL/MariaDB; worker memakai 1 koneksi pada satu waktu. |
| `UPLOAD_DIR` | `./uploads` | Akar unggahan; rekaman ada di `ptt/...`. Hanya dibaca. |
| `STT_MODEL_PATH` | WAJIB | Model ggml, mis. `/opt/kawalo/stt/models/ggml-small-q5_1.bin`. Worker menolak start bila berkas tidak ada. |
| `STT_BIN_PATH` | `whisper-cli` | Path `whisper-cli` (dicari di `PATH` bila bukan path). |
| `STT_VAD_MODEL_PATH` | kosong | Opsional: model VAD whisper.cpp (`ggml-silero-v5.1.2.bin`); bila diisi ditambahkan `--vad -vm <path>`. |
| `STT_LANGUAGE` | `id` | Kode bahasa Whisper. `id` disimpan sebagai locale `id_ID`. |
| `STT_THREADS` | `1` | Thread whisper (`-t`). |
| `STT_GRACE_SECONDS` | `60` | Tunggu laporan perangkat sebelum server mengambil; juga jeda sebelum mencoba ulang setelah gagal. |
| `STT_MIN_SPEECH_MS` | `1000` | Minimal durasi "keras" agar whisper dijalankan. |
| `STT_SILENCE_DBFS` | `-40` | Ambang RMS per frame 20 ms (dBFS, -120..0). Derau ruangan lewat jalur VOICE_COMMUNICATION biasanya -55 s.d. -60, ucapan -30 s.d. -15; setel dari data nyata. |
| `STT_JOB_TIMEOUT_SECONDS` | `120` | `whisper-cli` dibunuh setelah ini. |
| `STT_POLL_SECONDS` | `5` | Jeda cek antrean saat kosong. |
| `STT_STALE_CLAIM_SECONDS` | `300` | Klaim `processing` lebih tua dari ini dianggap milik worker mati. Harus > `STT_JOB_TIMEOUT_SECONDS`. |
| `STT_MAX_ATTEMPTS` | `3` | Percobaan maksimal sebelum `error`. |
| `TMPDIR` | `/tmp` | Tempat direktori kerja sementara (di systemd: `/tmp` privat karena `PrivateTmp=true`). |

## Build (dari Mac, seperti API)
```bash
cargo zigbuild --release --target x86_64-unknown-linux-gnu.2.39 --bin ptt_transcriber
# hasil: target/x86_64-unknown-linux-gnu/release/ptt_transcriber
```

## Pasang `whisper-cli` dan model di `server-114` (sekali)
Host aaPanel bersama (2 vCPU, RAM 3,9 GB, MySQL 9 + situs lain). Jangan memasang paket yang membawa layanan database
(`mariadb-server`, `mysql-server`, dll.; lihat CLAUDE.md bagian 5H). Paket di bawah hanya toolchain.

1. Cek toolchain dan RAM dulu: `cmake --version; g++ --version; git --version; free -h`. Bila belum ada:
   `apt-get install --no-install-recommends build-essential cmake git` (periksa daftar paket yang akan dipasang sebelum menjawab `Y`).
2. Build (CPU saja, AVX2; `-j1` agar kompilasi tidak menghabiskan RAM bersama; cek `free -h` selama build):
   ```bash
   mkdir -p /usr/local/src && cd /usr/local/src
   git clone --depth 1 --branch <tag rilis terbaru, mis. v1.8.x> https://github.com/ggml-org/whisper.cpp
   cd whisper.cpp
   cmake -B build -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF -DGGML_NATIVE=ON -DWHISPER_BUILD_TESTS=OFF
   cmake --build build -j1 --config Release --target whisper-cli
   install -d -m 755 /opt/kawalo/stt /opt/kawalo/stt/models
   install -m 755 build/bin/whisper-cli /opt/kawalo/stt/whisper-cli
   /opt/kawalo/stt/whisper-cli --help | grep -E -- '--vad|-nt|-np'   # opsi --vad butuh versi >= 1.7.6
   ```
   `GGML_NATIVE=ON` mengoptimalkan untuk CPU tempat build, jadi build di server itu sendiri (atau matikan dan set flag AVX2 bila
   build di mesin lain).
3. Model (unduh di server, lalu izin baca untuk user `kawalo`):
   ```bash
   cd /usr/local/src/whisper.cpp
   sh models/download-ggml-model.sh small-q5_1                # ~190 MB -> models/ggml-small-q5_1.bin
   sh models/download-vad-model.sh silero-v5.1.2              # opsional, VAD
   install -m 644 models/ggml-small-q5_1.bin models/ggml-silero-v5.1.2.bin /opt/kawalo/stt/models/
   ```
4. Tambahkan ke `/etc/kawalo/kawalo.env`:
   ```
   STT_BIN_PATH=/opt/kawalo/stt/whisper-cli
   STT_MODEL_PATH=/opt/kawalo/stt/models/ggml-small-q5_1.bin
   STT_VAD_MODEL_PATH=/opt/kawalo/stt/models/ggml-silero-v5.1.2.bin
   ```
5. Uji manual dengan rekaman uji buatan tim (BUKAN rekaman petugas asli), sebagai user `kawalo`:
   `runuser -u kawalo -- /opt/kawalo/stt/whisper-cli -m /opt/kawalo/stt/models/ggml-small-q5_1.bin -f <uji>.wav -l id -t 1 -nt -np`.

## Deploy, start, stop
```bash
scp target/x86_64-unknown-linux-gnu/release/ptt_transcriber server-114:/tmp/ptt_transcriber.new
scp deploy/kawalo-ptt-transcriber.service server-114:/etc/systemd/system/
ssh server-114 'install -m 755 /tmp/ptt_transcriber.new /opt/kawalo/ptt_transcriber && rm /tmp/ptt_transcriber.new \
  && systemctl daemon-reload && systemctl enable --now kawalo-ptt-transcriber'
```
- Migrasi `20261111020000_add_ptt_transcript` harus sudah diterapkan (lihat CLAUDE.md 6B).
- **Mematikan:** `systemctl disable --now kawalo-ptt-transcriber`. Tidak ada flag di API; tanpa worker, rekaman tanpa teks perangkat
  tetap tanpa transkrip (tidak ada yang rusak).
- Update biner: ulangi `scp` + `install`, lalu `systemctl restart kawalo-ptt-transcriber` (job yang sedang jalan dikembalikan ke antrean).
- Batas sumber daya di unit: `Nice=19`, `CPUWeight=10`, `CPUQuota=50%`, `IOSchedulingClass=idle`, `MemoryMax=1G`.

## Pemantauan
```bash
ssh server-114 'systemctl status kawalo-ptt-transcriber --no-pager; journalctl -u kawalo-ptt-transcriber -n 50 --no-pager -o cat'
```
Isi antrean (lewat klien MySQL aaPanel, lihat CLAUDE.md 6B):
```sql
SELECT TranscriptStatus, TranscriptSource, COUNT(*) FROM tkpttrecording WHERE DeletedAt IS NULL GROUP BY 1, 2;
```
Banyak `error` atau `failed` dengan log `exited with` / `timed out`: cek model, waktu proses, dan `MemoryMax` (OOM terlihat di
`journalctl -k` dan `systemctl status`).

## Belum terverifikasi (harus diukur di spike tahap 1)
- `whisper-cli` asli BELUM PERNAH dijalankan oleh worker ini; tes memakai skrip palsu. Format keluaran `-nt -np` dan opsi `--vad -vm`
  diambil dari dokumentasi whisper.cpp; cocokkan dengan versi yang dipasang.
- Kecepatan small q5_1 dengan 1 thread di Xeon E5-2698 v3 dengan `CPUQuota=50%` (perkiraan 15 sampai 40 detik per klip 30 detik)
  dan apakah `STT_JOB_TIMEOUT_SECONDS=120` cukup; RAM (RSS) terhadap `MemoryMax=1G`; dampak ke latensi relay PTT.
- Akurasi bahasa Indonesia pada audio PTT berderau (WER), dan halusinasi pada klip hampir hening yang lolos gerbang energi.
- Ambang gerbang `STT_SILENCE_DBFS=-40` dan `STT_MIN_SPEECH_MS=1000` belum disetel dengan rekaman nyata.
- SQL klaim diuji di MariaDB 12.3 (scratch); belum diuji di MySQL 9.7 (`server-114`) maupun MariaDB 10.5, walau hanya memakai
  sintaks umum keduanya (`UPDATE` bersyarat, `CASE`, `INTERVAL`).
- Unit systemd belum pernah dipasang; opsi hardening (`ProtectSystem=strict`, `PrivateDevices`, dll.) mungkin perlu dilonggarkan
  bila `whisper-cli` butuh akses lain.
