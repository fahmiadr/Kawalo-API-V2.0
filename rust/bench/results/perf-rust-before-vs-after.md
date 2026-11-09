# Benchmark 2026-11-09

Servers: **rust-before** (http://127.0.0.1:3140, pid 47870) vs **rust-after** (http://127.0.0.1:3141, pid 47872)

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

| | rust-before | rust-after |
|---|---|---|
| start to first /health (ms) | 31 | 21 |
| idle RSS (MB) | 8.5 | 10.9 |
| RSS after all runs (MB) | 18.2 | 45.9 |
| artifact size (MB) | 6.7 | 8.0 |

## HTTP scenarios

| scenario | c | rust-before rps | rust-before p50 ms | rust-before p90 ms | rust-before p99 ms | rust-before max ms | rust-before err % | rust-before RSS peak MB | rust-before CPU % | rust-after rps | rust-after p50 ms | rust-after p90 ms | rust-after p99 ms | rust-after max ms | rust-after err % | rust-after RSS peak MB | rust-after CPU % | rps rust-after/rust-before | p99 rust-before/rust-after |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| guests | 1 | 663 | 1.46 | 1.68 | 2.12 | 7.5 | 0.00 | 9 | 56 | 2055 | 0.47 | 0.53 | 0.74 | 4.9 | 0.00 | 10 | 41 | 3.10x | 2.85x |
| guests | 16 | 5465 | 2.78 | 3.80 | 5.12 | 18.1 | 0.00 | 13 | 602 | 9966 | 1.57 | 1.83 | 2.51 | 11.8 | 0.00 | 24 | 323 | 1.82x | 2.04x |
| guests | 64 | 5391 | 11.58 | 13.06 | 17.26 | 30.5 | 0.00 | 20 | 599 | 9862 | 6.35 | 7.01 | 9.17 | 20.2 | 0.00 | 30 | 324 | 1.83x | 1.88x |
| attendance | 1 | 676 | 1.45 | 1.60 | 2.04 | 7.8 | 0.00 | 9 | 60 | 1950 | 0.50 | 0.56 | 0.69 | 4.4 | 0.00 | 14 | 47 | 2.88x | 2.95x |
| attendance | 16 | 4804 | 3.22 | 4.07 | 5.32 | 9.4 | 0.00 | 14 | 551 | 8006 | 1.96 | 2.21 | 2.82 | 12.5 | 0.00 | 25 | 304 | 1.67x | 1.88x |
| attendance | 64 | 4703 | 13.32 | 14.74 | 19.30 | 35.4 | 0.00 | 20 | 543 | 7935 | 7.91 | 8.47 | 10.97 | 20.6 | 0.00 | 28 | 302 | 1.69x | 1.76x |
| incident-get | 1 | 795 | 1.24 | 1.38 | 1.65 | 5.0 | 0.00 | 10 | 56 | 3732 | 0.25 | 0.32 | 0.39 | 6.9 | 0.00 | 13 | 42 | 4.70x | 4.21x |
| incident-get | 16 | 7627 | 1.90 | 2.94 | 4.82 | 30.4 | 0.00 | 13 | 642 | 16118 | 0.98 | 1.13 | 1.42 | 10.8 | 0.00 | 23 | 311 | 2.11x | 3.39x |
| incident-get | 64 | 7787 | 8.02 | 9.23 | 11.65 | 39.8 | 0.00 | 17 | 662 | 16082 | 3.91 | 4.20 | 5.21 | 19.0 | 0.00 | 27 | 314 | 2.07x | 2.24x |
| login | 1 | 95 | 10.49 | 10.94 | 12.56 | 16.9 | 0.00 | 9 | 82 | 212 | 4.63 | 5.06 | 6.77 | 11.3 | 0.00 | 13 | 72 | 2.23x | 1.85x |
| login | 16 | 834 | 18.76 | 22.62 | 28.58 | 50.0 | 0.00 | 13 | 777 | 1983 | 7.33 | 11.25 | 23.29 | 87.9 | 0.00 | 29 | 759 | 2.38x | 1.23x |
| login | 64 | 862 | 73.36 | 81.92 | 93.44 | 116.0 | 0.00 | 18 | 803 | 2123 | 29.09 | 37.31 | 49.41 | 91.3 | 0.00 | 46 | 771 | 2.46x | 1.89x |

Host load average (1/5/15 min) at start: 23.73 / 13.11 / 9.75; at end: 26.54 / 17.04 / 12.65. Other CPU-heavy work on the host (builds, other agents) skews results.

CPU % is process CPU time over wall time (100 = one full core). RSS sampled with `ps` every 500 ms. Raw repetitions are in the JSON.
