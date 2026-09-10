# Performance baseline record

The original release gate specifies an x86_64 Linux reference host. For this local completion pass,
the executable and Criterion smoke suite were run on macOS arm64 with Rust 1.98.1. These local
measurements provide regression evidence for development but do not claim Linux/SSH performance.
The Linux reference run remains deployment follow-up rather than an implementation blocker.

Record raw observations, CPU/kernel/toolchain details, fixture checksum, resident-memory peak, and
the approved baseline here before release. A candidate fails if it exceeds a specification limit,
exceeds the 768 MiB routine working-set budget (excluding an explicit spatial index), or regresses
more than 10% from the approved baseline.

| Measurement | Baseline | Candidate | Limit / tolerance |
|---|---:|---:|---|
| Startup successful runs / 30 | not run | not run | at least 29 / 30 on Linux reference host |
| Startup p95 (ms) | not run | not run | 2,000 ms on Linux reference host |
| Navigation/render p95 (ms) | not run | not run | 200 ms on Linux reference host |
| Peak routine working set (MiB) | not run | not run | 768 MiB on Linux reference host |
| Regression against baseline | not applicable | not applicable | no more than 10% against approved host baseline |

## Local smoke observations (macOS arm64)

These are development measurements only and do not replace the required x86_64 Linux reference
host gate: startup scaffold 20.0 ns median, slice conversion 63.4 µs, rasterization 282.9 µs,
normalization 1.43 ns, regular-grid mapping 2.32 ns, and KD-tree query 117.8 µs. The KD-tree
comparison reported a 16.1% change against Criterion's local prior sample, so no regression claim
is made until the approved Linux baseline exists.

## Local macOS arm64 smoke run (2026-09-10)

`cargo bench --locked --bench interactive_paths -- --noplot` completed successfully under Rust
1.98.1. Criterion medians were: startup scaffold 18.497 ns, slice conversion 48.208 µs,
rasterization 221.55 µs, normalization 1.2351 ns, regular-grid mapping 2.1109 ns, and KD-tree
query 75.026 µs. These are component-level smoke measurements, not the full 30-run application
startup, resident-set, or SSH protocol gates.
