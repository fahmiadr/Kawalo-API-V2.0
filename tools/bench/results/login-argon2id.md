# Benchmark 2026-11-10

Servers: **rust-before** (http://127.0.0.1:3150, pid 98299) vs **rust-argon2** (http://127.0.0.1:3151, pid 98304)

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

| | rust-before | rust-argon2 |
|---|---|---|
| start to first /health (ms) | 361 | 353 |
| idle RSS (MB) | 6.5 | 6.5 |
| RSS after all runs (MB) | 29.3 | 247.4 |
| artifact size (MB) | 8.0 | 8.1 |

## HTTP scenarios

| scenario | c | rust-before rps | rust-before p50 ms | rust-before p90 ms | rust-before p99 ms | rust-before max ms | rust-before err % | rust-before RSS peak MB | rust-before CPU % | rust-argon2 rps | rust-argon2 p50 ms | rust-argon2 p90 ms | rust-argon2 p99 ms | rust-argon2 max ms | rust-argon2 err % | rust-argon2 RSS peak MB | rust-argon2 CPU % | rps rust-argon2/rust-before | p99 rust-before/rust-argon2 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| login | 1 | 218 | 4.48 | 4.96 | 6.36 | 9.2 | 0.00 | 12 | 72 | 49 | 20.24 | 21.51 | 23.50 | 24.9 | 0.00 | 31 | 86 | 0.22x | 0.27x |
| login | 16 | 1720 | 8.49 | 12.58 | 20.97 | 52.5 | 0.00 | 24 | 674 | 339 | 43.96 | 64.42 | 95.48 | 124.9 | 0.00 | 245 | 764 | 0.20x | 0.22x |
| login | 64 | 1830 | 34.36 | 42.08 | 55.79 | 102.2 | 0.00 | 32 | 671 | 352 | 178.70 | 210.77 | 250.43 | 320.9 | 0.00 | 257 | 761 | 0.19x | 0.22x |

Host load average (1/5/15 min) at start: 14.72 / 16.40 / 10.59; at end: 31.29 / 20.01 / 12.97. Other CPU-heavy work on the host (builds, other agents) skews results.

CPU % is process CPU time over wall time (100 = one full core). RSS sampled with `ps` every 500 ms. Raw repetitions are in the JSON.
