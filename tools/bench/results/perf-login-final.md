# Benchmark 2026-11-10

Servers: **typescript** (http://127.0.0.1:3140, pid 84046) vs **rust-after** (http://127.0.0.1:3141, pid 84048)

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
| start to first /health (ms) | 395 | 23 |
| idle RSS (MB) | 227.8 | 11.1 |
| RSS after all runs (MB) | 333.3 | 34.1 |
| artifact size (MB) | 374.9 (dist 1.3 + node_modules 373.6) | 8.0 |

## HTTP scenarios

| scenario | c | typescript rps | typescript p50 ms | typescript p90 ms | typescript p99 ms | typescript max ms | typescript err % | typescript RSS peak MB | typescript CPU % | rust-after rps | rust-after p50 ms | rust-after p90 ms | rust-after p99 ms | rust-after max ms | rust-after err % | rust-after RSS peak MB | rust-after CPU % | rps rust-after/typescript | p99 typescript/rust-after |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| login | 1 | 178 | 5.56 | 6.12 | 7.50 | 11.4 | 0.00 | 259 | 76 | 214 | 4.65 | 5.05 | 5.86 | 9.3 | 0.00 | 11 | 71 | 1.20x | 1.28x |
| login | 16 | 1107 | 14.30 | 15.07 | 16.37 | 21.7 | 0.00 | 339 | 437 | 2011 | 7.33 | 10.67 | 15.41 | 36.7 | 0.00 | 24 | 780 | 1.82x | 1.06x |
| login | 64 | 1110 | 57.49 | 58.15 | 62.95 | 88.6 | 0.00 | 333 | 435 | 2071 | 30.32 | 35.60 | 43.74 | 75.0 | 0.00 | 33 | 810 | 1.87x | 1.44x |

Host load average (1/5/15 min) at start: 4.76 / 11.36 / 11.65; at end: 10.59 / 10.40 / 11.09. Other CPU-heavy work on the host (builds, other agents) skews results.

CPU % is process CPU time over wall time (100 = one full core). RSS sampled with `ps` every 500 ms. Raw repetitions are in the JSON.
