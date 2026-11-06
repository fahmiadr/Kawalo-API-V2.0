# Kawalo Mobile API (Kawalo-API-V2.0)

Enterprise-grade Backend API for Kawalo Mobile built with **ElysiaJS**, **Prisma**, **MariaDB**, and **TypeScript**.

---

## 🛠️ Prasyarat (Prerequisites)

* **Node.js**: `>= 20.19.0`
* **MariaDB / MySQL**: `10.5+`
* **Redis**: `6.0+`
* **NPM**: `>= 10.0.0`

---

## 🚀 Panduan Memulai (Quick Start)

Ikuti langkah-langkah di bawah ini untuk menjalankan backend di mesin lokal:

### 1. Unduh Dependensi
```bash
npm install
```

### 2. Konfigurasi Environment
Salin template konfigurasi:
```bash
cp .env.example .env.dev
```
Pastikan `DATABASE_URL` dan `REDIS_URL` di dalam file `.env.dev` telah sesuai dengan konfigurasi lokal Anda.

### 3. Inisialisasi Database (Import data.sql)
Aplikasi menyertakan dump skema dan data awal pada berkas `data.sql`:
```bash
# Buat database jika belum ada
mysql -u root -e "CREATE DATABASE IF NOT EXISTS kawalo CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;"

# Import data awal
mysql -u root kawalo < data.sql
```

### 4. Sinkronisasi Skema Prisma
Generate Prisma Client:
```bash
npm run prisma:generate
```

### 5. Jalankan Server Development
```bash
npm run dev
```
Server akan aktif di `http://localhost:3000`.

---

## 📜 Perintah Tersedia (Available Scripts)

| Perintah | Deskripsi |
| :--- | :--- |
| `npm run dev` | Menjalankan server development dengan auto-reload (watch mode) |
| `npm run build` | Melakukan bundling kode produksi ke folder `dist/` |
| `npm start` | Menjalankan build produksi dari `dist/server.js` |
| `npm test` | Menjalankan seluruh test suite dengan Vitest |
| `npm run lint` | Melakukan static linting menggunakan Biome |
| `npm run prisma:generate` | Menghasilkan client Prisma dari `prisma/schema.prisma` |
| `npm run auth:rotate-key` | Menjalankan rotasi pasangan kunci RSA 4096-bit |

