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

## Pre-feature implementation gate (2026-09-14)

Host: macOS arm64, Darwin 25.3.0, Rust 1.98.1, Cargo 1.98.1. Existing dirty source changes
were preserved. Commands and outcomes:

| Check | Result |
|---|---|
| `cargo build --locked --release --bin ncv` | pass, 0.60 s |
| `cargo test --locked --all-targets` | pass, 42 unit tests plus all integration suites and benchmark smoke target |
| `cargo fmt --all --check` | pass |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | pre-existing failure: `src/app.rs:1030` (`clippy::question_mark`) |
| `otool -L target/release/ncv` | only macOS system frameworks and libraries; no cloud/native TLS library present |
| `cargo bench --locked --bench interactive_paths -- --noplot` | pass; startup 584.81 µs, slice conversion 4.3451 µs, rasterization 91.449 µs, large viewport 2.1683 ms, map backdrop 2.0482 ms, normalization 1.2210 ns, mapping 2.0883 ns, KD-tree query 641.92 ns |

The benchmark’s Criterion percentage comparisons use local prior samples from a different build
state and are not used as a release regression claim. Linux x86_64, 30-run startup, RSS, and SSH
interactive measurements remain required release follow-up.

## Phase 4 automated evidence (2026-09-14)

These deterministic checks validate the bounded behavior and stale-result policy, but do not
replace the Linux reference benchmark required by SC-003, SC-004, or SC-009:

| Evidence | Observation | Source |
|---|---|---|
| NetCDF range access | A 16 MiB virtual object is read through two page-aligned requests; requested bytes remain below the object size and overlapping reads use the page cache. | `tests/remote_netcdf4.rs` |
| GRIB2 indexed access | The selected message is fetched from its validated exact range; transfer remains below the complete local fixture size. | `tests/remote_grib2.rs` |
| Working-set accounting | Decoded slices report value/mask/coordinate bytes; cache categories and current/pending rendered buffers expose retained usage. | `tests/navigation.rs`, `tests/remote_ranges.rs` |
| Superseded navigation | Time, level, zoom, pan, resize, and plot-generation changes reject stale results while retaining the previous slice. | `tests/remote_terminal.rs` |
| Cancellation | A cancelled remote open stops before provider I/O; delayed opening publishes progress before the provider response. | `tests/remote_terminal.rs` |

No RSS peak, 1 GiB transfer percentage, 100-action p95, or 99%-of-runs cancellation distribution is
claimed here; collecting those measurements remains T035/release follow-up.

## Remote streaming smoke evidence (2026-09-14)

The new Criterion target was run on the same macOS arm64 development host with a deterministic
in-memory object store. These are component measurements, not the required Linux reference-host
release gate:

| Benchmark | Median |
|---|---:|
| `remote_range_cache_lookup` | 99.7 ns |
| `remote_range_fetch` | 605 ns |
| `adjacent_range_coalescing` | 65.3 ns |

Command:

```bash
cargo bench --offline --locked --bench remote_streaming -- --noplot \
  --warm-up-time 0.1 --measurement-time 0.2
```

The range/cache benchmark completed successfully. The in-memory provider is not representative of
network latency, bandwidth, Linux RSS, SSH rendering, or a 1 GiB object; those measurements remain
explicit deployment validation items.

## Local macOS arm64 smoke run (2026-09-10)

`cargo bench --locked --bench interactive_paths -- --noplot` completed successfully under Rust
1.98.1. Criterion medians were: startup scaffold 18.497 ns, slice conversion 48.208 µs,
rasterization 221.55 µs, normalization 1.2351 ns, regular-grid mapping 2.1109 ns, and KD-tree
query 75.026 µs. These are component-level smoke measurements, not the full 30-run application
startup, resident-set, or SSH protocol gates.
