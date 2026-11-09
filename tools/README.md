# Tools: paritas dan benchmark terhadap server TypeScript

`parity/` membandingkan server Rust dengan server TypeScript asli (respons HTTP, SSE, WebSocket PTT) dan `bench/`
mengukur keduanya. Server TypeScript (oracle) sudah tidak ada di cabang ini; ia hidup di cabang `legacy/typescript`
dan harus di-checkout terpisah.

## Menyiapkan oracle TypeScript

Dari root repo ini:

```bash
git worktree add ../Kawalo-API-V2.0-ts legacy/typescript
cd ../Kawalo-API-V2.0-ts
npm ci
node scripts/patch-elysia.mjs
```

`npm ci` memakai `.npmrc` dengan `ignore-scripts=true`, jadi `patch-elysia.mjs` (patch WS `rawData` pada
`@elysiajs/node`, wajib untuk PTT) dijalankan manual. Hanya bila ingin menjalankan `vitest` di checkout itu pada macOS arm64
dan muncul "Cannot find native binding": `npm i --no-save @rolldown/binding-darwin-arm64@<versi rolldown terpasang>`
(jangan `--package-lock=false`: itu mengacak versi dependensi). Harness paritas/bench hanya memakai `tsx`, jadi langkah ini tidak diperlukan.

Lokasi checkout dibaca dari `KW_TS_ROOT` (default `../Kawalo-API-V2.0-ts`, relatif terhadap root repo ini):

```bash
export KW_TS_ROOT=/path/ke/Kawalo-API-V2.0-ts
```

Jangan mengubah file di checkout itu. Harness membuat symlink `tools/node_modules` ke `$KW_TS_ROOT/node_modules` (ter-ignore git)
agar `mint-token.mts` / `mktokens.mts` menemukan `ioredis` dan `jose`. Setelah selesai: `git worktree remove ../Kawalo-API-V2.0-ts`
(dan hapus symlink `tools/node_modules` bila oracle sudah dihapus).

## Menjalankan

Bangun biner Rust dulu (`cargo build --release` dari root), lalu:

- Paritas: [`parity/README.md`](parity/README.md). Gerbang yang harus 0 beda: `KW_NODE_ENV=production KW_TZ=Asia/Jakarta tools/parity/services.sh run`
  setelah `tools/parity/services.sh start` dan `servers ts-rs`; keputusan tiap perbedaan ada di [`parity/KNOWN_DIFFERENCES.md`](parity/KNOWN_DIFFERENCES.md).
- Benchmark: [`bench/README.md`](bench/README.md).

Semua service (MariaDB, Redis) yang dibuat harness adalah instans scratch di `/tmp` pada port non-default; `services.sh stop` mematikannya.
