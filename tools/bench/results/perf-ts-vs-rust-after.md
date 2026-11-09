# Benchmark 2026-11-09

Servers: **typescript** (http://127.0.0.1:3140, pid 13461) vs **rust-after** (http://127.0.0.1:3141, pid 13468)

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

Method: oha closed-loop, keep-alive, 8s per run after 3s warm-up (discarded), median of 3 runs; servers measured one at a time (interleaved per repetition) against the same MariaDB/Redis scratch instances. Load generator runs on the same machine.

## Process footprint

| | typescript | rust-after |
|---|---|---|
| start to first /health (ms) | 431 | 23 |
| idle RSS (MB) | 347.6 | 5.3 |
| RSS after all runs (MB) | 217.5 | 44.6 |
| artifact size (MB) | 374.9 (dist 1.3 + node_modules 373.6) | 8.0 |

## HTTP scenarios

| scenario | c | typescript rps | typescript p50 ms | typescript p90 ms | typescript p99 ms | typescript max ms | typescript err % | typescript RSS peak MB | typescript CPU % | rust-after rps | rust-after p50 ms | rust-after p90 ms | rust-after p99 ms | rust-after max ms | rust-after err % | rust-after RSS peak MB | rust-after CPU % | rps rust-after/typescript | p99 typescript/rust-after |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| guests | 1 | 841 | 1.14 | 1.35 | 2.39 | 6.8 | 0.00 | 336 | 72 | 2066 | 0.47 | 0.53 | 0.71 | 6.6 | 0.00 | 10 | 41 | 2.46x | 3.39x |
| guests | 16 | 1639 | 9.59 | 11.21 | 13.55 | 35.2 | 0.00 | 372 | 114 | 10080 | 1.56 | 1.81 | 2.26 | 11.7 | 0.00 | 23 | 324 | 6.15x | 6.00x |
| guests | 64 | 1549 | 40.72 | 43.54 | 50.86 | 151.7 | 0.00 | 377 | 114 | 9799 | 6.32 | 6.93 | 10.48 | 19.2 | 0.00 | 32 | 326 | 6.33x | 4.85x |
| attendance | 1 | 928 | 1.03 | 1.22 | 2.01 | 6.1 | 0.00 | 336 | 73 | 1938 | 0.50 | 0.57 | 0.69 | 4.0 | 0.00 | 14 | 47 | 2.09x | 2.92x |
| attendance | 16 | 1766 | 8.92 | 10.38 | 12.08 | 20.7 | 0.00 | 369 | 113 | 7970 | 1.97 | 2.22 | 3.09 | 10.2 | 0.00 | 25 | 302 | 4.51x | 3.91x |
| attendance | 64 | 1647 | 38.18 | 40.89 | 49.24 | 185.5 | 0.00 | 372 | 114 | 7909 | 7.93 | 8.61 | 11.10 | 24.9 | 0.00 | 28 | 303 | 4.80x | 4.44x |
| incident-get | 1 | 1360 | 0.70 | 0.86 | 1.37 | 8.3 | 0.00 | 308 | 63 | 3671 | 0.26 | 0.32 | 0.42 | 5.8 | 0.00 | 13 | 41 | 2.70x | 3.30x |
| incident-get | 16 | 3526 | 4.42 | 5.37 | 6.96 | 25.6 | 0.00 | 368 | 120 | 15959 | 0.98 | 1.14 | 1.57 | 11.8 | 0.00 | 23 | 309 | 4.53x | 4.45x |
| incident-get | 64 | 3373 | 18.76 | 20.60 | 23.84 | 88.5 | 0.00 | 375 | 120 | 16000 | 3.92 | 4.21 | 5.64 | 22.9 | 0.00 | 27 | 313 | 4.74x | 4.23x |
| login | 1 | 178 | 5.51 | 6.21 | 8.72 | 18.9 | 0.00 | 237 | 74 | 211 | 4.68 | 5.08 | 6.65 | 15.3 | 0.00 | 13 | 71 | 1.19x | 1.31x |
| login | 16 | 1090 | 14.33 | 15.20 | 21.62 | 37.8 | 0.00 | 351 | 431 | 1958 | 7.33 | 11.42 | 23.95 | 85.6 | 0.00 | 29 | 749 | 1.80x | 0.90x |
| login | 64 | 1098 | 57.83 | 59.19 | 66.66 | 136.8 | 0.00 | 363 | 436 | 2083 | 29.54 | 38.68 | 53.91 | 116.0 | 0.00 | 45 | 756 | 1.90x | 1.24x |

Host load average (1/5/15 min) at start: 6.94 / 7.78 / 7.67; at end: 23.73 / 13.11 / 9.75. Other CPU-heavy work on the host (builds, other agents) skews results.

CPU % is process CPU time over wall time (100 = one full core). RSS sampled with `ps` every 500 ms. Raw repetitions are in the JSON.
