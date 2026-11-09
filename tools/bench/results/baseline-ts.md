# Benchmark 2026-11-09

Servers: **ts** (http://127.0.0.1:3100, pid 57484)

## Machine

| | |
|---|---|
| cpu | Apple M1 Pro |
| cores | 10 |
| ramGb | 16 |
| os | Darwin 24.6.0 arm64 |
| node | v24.6.0 |
| mariadb | mariadbd  Ver 12.3.3-MariaDB for osx10.20 on arm64 (Homebrew) |
| redis | Redis server v=8.2.2 sha=00000000:1 malloc=libc bits=64 build=b8c0c59a9db96eb1 |
| oha | oha 1.16.0 |

Method: oha closed-loop, keep-alive, 10s per run after 5s warm-up (discarded), median of 3 runs; servers measured one at a time (interleaved per repetition) against the same MariaDB/Redis scratch instances. Load generator runs on the same machine.

## Process footprint

| | ts |
|---|---|
| start to first /health (ms) | 418 |
| idle RSS (MB) | 48.2 |
| RSS after all runs (MB) | 35.1 |
| artifact size (MB) | 374.9 (dist 1.3 + node_modules 373.6) |

## HTTP scenarios

| scenario | c | ts rps | ts p50 ms | ts p90 ms | ts p99 ms | ts max ms | ts err % | ts RSS peak MB | ts CPU % |
|---|---|---|---|---|---|---|---|---|---|
| health | 1 | 12895 | 0.07 | 0.10 | 0.15 | 8.2 | 0.00 | 125 | 55 |
| health | 16 | 26443 | 0.54 | 0.91 | 1.25 | 14.7 | 0.00 | 175 | 93 |
| health | 64 | 26447 | 2.32 | 2.66 | 3.79 | 56.9 | 0.00 | 179 | 94 |
| health | 256 | 24769 | 10.05 | 11.12 | 13.66 | 1241.0 | 0.00 | 217 | 94 |
| jwks | 1 | 12414 | 0.07 | 0.10 | 0.15 | 6.4 | 0.00 | 191 | 57 |
| jwks | 16 | 25226 | 0.60 | 0.72 | 1.21 | 7.1 | 0.00 | 196 | 94 |
| jwks | 64 | 25089 | 2.46 | 2.80 | 3.58 | 47.1 | 0.00 | 200 | 94 |
| jwks | 256 | 24398 | 10.22 | 11.28 | 13.10 | 1325.8 | 0.00 | 223 | 94 |
| guests | 1 | 798 | 1.19 | 1.46 | 2.31 | 11.1 | 0.00 | 333 | 72 |
| guests | 16 | 1554 | 10.09 | 11.83 | 15.33 | 41.8 | 0.00 | 376 | 112 |
| guests | 64 | 1466 | 42.97 | 47.04 | 58.60 | 159.0 | 0.00 | 383 | 114 |
| guests | 256 | 1366 | 184.46 | 207.66 | 271.82 | 621.7 | 0.00 | 438 | 139 |
| information | 1 | 693 | 1.36 | 1.75 | 2.69 | 11.2 | 0.00 | 339 | 70 |
| information | 16 | 1457 | 10.80 | 12.69 | 17.09 | 42.3 | 0.00 | 383 | 113 |
| information | 64 | 1361 | 45.91 | 51.05 | 73.21 | 184.9 | 0.00 | 395 | 114 |
| information | 256 | 758 | 254.50 | 588.96 | 885.51 | 972.8 | 0.00 | 446 | 104 |
| attendance | 1 | 360 | 1.55 | 5.66 | 16.39 | 65.6 | 0.00 | 274 | 52 |
| attendance | 16 | 468 | 25.02 | 65.32 | 113.44 | 167.0 | 0.00 | 292 | 51 |
| attendance | 64 | 1562 | 39.68 | 43.56 | 76.16 | 123.5 | 0.00 | 389 | 111 |
| attendance | 256 | 1470 | 172.29 | 187.69 | 215.53 | 537.6 | 0.00 | 440 | 134 |
| operational | 1 | 222 | 4.40 | 4.86 | 5.76 | 16.9 | 0.00 | 414 | 25 |
| operational | 16 | 376 | 37.33 | 62.36 | 68.76 | 126.3 | 0.00 | 311 | 41 |
| operational | 64 | 355 | 182.29 | 201.08 | 234.17 | 272.2 | 0.00 | 334 | 41 |
| operational | 256 | 314 | 794.90 | 851.70 | 1037.25 | 1056.4 | 0.00 | 435 | 41 |
| incident-get | 1 | 1250 | 0.70 | 1.00 | 2.21 | 18.3 | 0.00 | 370 | 63 |
| incident-get | 16 | 3290 | 4.73 | 5.65 | 8.57 | 33.9 | 0.00 | 331 | 118 |
| incident-get | 64 | 2714 | 21.09 | 31.01 | 47.52 | 104.3 | 0.00 | 290 | 115 |
| incident-get | 256 | 2866 | 83.53 | 96.45 | 207.14 | 566.9 | 0.00 | 404 | 120 |
| incident-post | 1 | 1064 | 0.84 | 1.22 | 2.33 | 24.0 | 0.00 | 348 | 66 |
| incident-post | 16 | 2614 | 5.67 | 7.50 | 12.47 | 55.6 | 0.00 | 376 | 114 |
| incident-post | 64 | 2522 | 24.15 | 29.71 | 49.71 | 80.1 | 0.00 | 381 | 114 |
| incident-post | 256 | 2652 | 94.47 | 101.67 | 127.54 | 298.5 | 0.00 | 418 | 120 |
| login | 1 | 179 | 5.42 | 6.14 | 7.35 | 21.1 | 0.00 | 425 | 76 |
| login | 16 | 1058 | 14.45 | 16.64 | 24.01 | 82.9 | 0.00 | 346 | 429 |
| login | 64 | 1079 | 58.13 | 61.73 | 81.20 | 107.3 | 0.00 | 354 | 439 |
| login | 256 | 1092 | 230.91 | 242.55 | 271.26 | 402.2 | 0.00 | 369 | 449 |

## PTT WebSocket fan-out (1 speaker, N-1 listeners, 50 fps x 28s, 67-byte frames)

| clients | ts p50 ms | ts p90 ms | ts p99 ms | ts max ms | ts loss % | ts delivered/s | ts connect ms | ts RSS peak MB | ts CPU % |
|---|---|---|---|---|---|---|---|---|---|
| 50 | 0.65 | 1.17 | 2.93 | 8.0 | 0.00 | 2450 | 72 | 170 | 4 |
| 256 | 2.26 | 4.37 | 11.61 | 66.6 | 0.00 | 12750 | 538 | 190 | 12 |

Host load average (1/5/15 min) at start: 5.10 / 5.03 / 5.13; at end: 11.37 / 10.84 / 11.38. Other CPU-heavy work on the host (builds, other agents) skews results.

CPU % is process CPU time over wall time (100 = one full core). RSS sampled with `ps` every 500 ms. Raw repetitions are in the JSON.
Catatan baseline: dijalankan saat load average host 5 sampai 11+ (agen lain sedang build). Perulangan terpisah skenario `attendance` pada load 8 sampai 28 memberi angka c=1/c=16 yang sama lambatnya (167 dan 526 rps), jadi angka c=1/c=16 skenario itu kemungkinan sebagian derau. Ulangi di mesin tenang sebelum dipakai sebagai klaim.
