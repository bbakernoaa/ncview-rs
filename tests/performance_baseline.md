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

## Raw-dimension slice smoke evidence (2026-10-01)

The optional `raw_300x600_fixed_dimension_slice` Criterion target was run against the supplied local
VIIRS product. Host: macOS 25.3.0, arm64 (Apple T8132); Rust 1.98.1. Input SHA-256:
`0525f3bfffdc87bdd0dd50a2a4edff9c8a6ed424ca5fcc7b8c85151c26f5abdf`. With 10 samples and a
0.5-second requested measurement period, Criterion reported `[22.858 ms, 30.620 ms, 35.067 ms]`
(lower estimate, median, upper estimate). This is slice-read latency only; it is not the full UI
view-load or dimension-change p95, and no RSS measurement was collected. The approved Linux
reference-host performance gate remains outstanding.

Command:

```bash
NCVIEW_RAW_DIMENSION_BENCH_FILE="$PWD/VIIRS_BRDF_LSA_NBAR_2025057_h19v19.nc" \
  cargo bench --bench interactive_paths -- raw_300x600_fixed_dimension_slice \
  --noplot --warm-up-time 0.1 --measurement-time 0.5 --sample-size 10
```

## Local macOS arm64 smoke run (2026-09-10)

`cargo bench --locked --bench interactive_paths -- --noplot` completed successfully under Rust
1.98.1. Criterion medians were: startup scaffold 18.497 ns, slice conversion 48.208 µs,
rasterization 221.55 µs, normalization 1.2351 ns, regular-grid mapping 2.1109 ns, and KD-tree
query 75.026 µs. These are component-level smoke measurements, not the full 30-run application
startup, resident-set, or SSH protocol gates.

## Palette chooser interaction smoke evidence (2026-10-01)

The `palette_picker_64_open_focus_preview` Criterion case opens the picker, moves focus in a
64-choice catalog, renders the focused preview and list into a 100×30 Ratatui `TestBackend`, and
cancels. Host: macOS 25.3.0, arm64 Apple T8132, Rust 1.98.1, Cargo 1.98.1. Criterion used 100
samples, a 0.1-second warmup, and a 0.5-second requested measurement. The raw per-sample average
latencies yield p50 44.764 µs and p95 152.5 µs (linear interpolation of the sorted sample values);
Criterion's slope estimate was [44.269 µs, 44.862 µs, 45.800 µs], with 11/100 outliers. The p95 is
below the feature's 100 ms smoke target. This is a TestBackend component benchmark, not physical
terminal latency or an x86_64 Linux release-gate result.

Command:

```bash
cargo bench --bench interactive_paths -- palette_picker_64_open_focus_preview \
  --noplot --warm-up-time 0.1 --measurement-time 0.5 --sample-size 100
```

## Interaction speed and UX pre-change smoke baseline (2026-10-02)

Host: macOS 25.3.0, arm64 Apple T8132, Darwin kernel `xnu-12377.91.3~2`; Rust/Cargo 1.98.1;
10 logical CPUs. The reusable deterministic fixture builders are in `benches/support/fixtures.rs`
(SHA-256 `f291f3aeecd552396ce66190d4b54f2f1d0a9b03466afb6e06e4dfd1afcfd459`). These short
10-sample runs were collected before implementation with a 0.1-second warmup and requested
0.2-second measurement. p50/p95 below are linearly interpolated from Criterion's raw per-iteration
sample values. They are local development baselines, not stable release measurements.

| Existing component benchmark | p50 | p95 |
|---|---:|---:|
| Large viewport rasterization | 78.9 µs | 84.3 µs |
| Palette picker (64 choices) | 45.0 µs | 45.9 µs |
| Remote range cache lookup | 76.6 ns | 77.9 ns |
| Remote range fetch (in-memory provider) | 427 ns | 435 ns |
| Adjacent range coalescing | 41.6 ns | 43.3 ns |

Commands:

```bash
cargo bench --locked --bench interactive_paths -- --noplot --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10
cargo bench --locked --bench remote_streaming -- --noplot --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10
```

Peak RSS could not be measured in this run. The 2048×1024 interaction-specific selection,
10,000-variable search, 256-palette search, cache-size/category mix, and repeated-coordinate
benchmarks are reported in the post-change results below. The Linux x86_64 reference host remains
unavailable in this environment.

## Interaction speed and UX post-change smoke results (2026-10-02)

Same local host, fixture generators, and Criterion arguments as the pre-change run. The short
10-sample result is a development comparison, with p50/p95 linearly interpolated from Criterion's
raw per-iteration samples. Comparisons against component references are directional and do not
replace the approved Linux release baseline.

| Workload | Pre-change p50 | Candidate p50 | Candidate p95 | Change / gate |
|---|---:|---:|---:|---|
| Selection-only on 2048×1024 field | 491.4 µs (full raster rebuild) | 20.5 µs (cell marker overlay) | 21.3 µs | 95.8% lower; below 100 ms |
| Range lookup, 4,096 mixed-category blocks | 1.271 µs | 80 ns | 82 ns | 93.7% lower |
| Repeated curvilinear slice read | 3.203 µs | 3.147 µs | 3.272 µs | no significant latency change; repeated slices now share the same `Arc<CoordinateGrid>` |
| Variable search, 10,000 entries | uncached reference: 366.7 µs | indexed: 160.6 µs | 163.0 µs | 56.2% lower; below 100 ms |
| Palette search, 256 entries | not captured before implementation | 8.47 µs | 8.83 µs | below 100 ms |

The variable-search comparison runs the former lowercase-and-sort algorithm alongside the indexed
candidate on the same fixture and host. Coordinate memory/RSS was unavailable; pointer-identity
coverage verifies repeated regular-axis and curvilinear slice reads share their coordinate-grid
storage. The renderer's retained-byte counters cover encoded image buffers but do not provide
process peak RSS. Kitty/Sixel/iTerm2 manual overlay checks and Linux reference-host measurements
remain outstanding.

The 320×160 RGB payload base64 component measured p50 13.8 µs and p95 14.2 µs on the short run.
It measures payload serialization, not complete terminal-protocol image encoding.

The full quickstart Criterion suites also completed with their default 100-sample settings. Their
candidate p95 values were 29.6 µs for selection-only, 232.4 µs for 10,000-variable search,
18.2 µs for 256-palette search, and 4.14 µs for repeated curvilinear reads. The final indexed
4,096-entry cache lookup was rerun after the full suite with the short paired settings above;
repeated hits measured 79.8 ns p50 / 81.8 ns p95. Some unrelated component benches reported
regressions against Criterion's retained local prior samples; because those prior samples represent
different builds and long-run CPU conditions, only same-command paired feature workloads are used
for this feature's comparison. The formal no-more-than-10% release comparison still requires the
approved reference host.

## Bounded zoom resolution smoke result (2026-10-06)

The menu-bound mapping component was measured on macOS arm64 using deterministic regular geographic
axes with 1,440 longitude samples and 721 latitude samples at 0.25-degree spacing. The benchmark
maps an inclusive 50-degree x extent and 40-degree y extent to source index ranges. Rust/Cargo
1.98.1; Criterion used 100 samples, a 0.1-second warmup, and a 0.5-second requested measurement.
Raw sample averages yield p50 4.446 µs and p95 4.901 µs (linear interpolation over the sorted
sample values); Criterion's slope estimate was [4.498 µs, 4.535 µs, 4.574 µs]. This isolates
coordinate-to-index mapping and excludes source I/O, worker scheduling, and terminal rendering.

Command:

```bash
cargo bench --bench interactive_paths -- menu_numeric_bounds_1440x721_regular_geographic \
  --noplot --warm-up-time 0.1 --measurement-time 0.5 --sample-size 100
```

## Feature quickstart checks (2026-10-02)

| Quickstart command | Result |
|---|---|
| `cargo test --locked --all-targets` | pass, all unit/integration suites and Criterion smoke targets |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | pass |
| `cargo bench --locked --bench interactive_paths -- --noplot` | pass, default 100-sample suite |
| `cargo bench --locked --bench remote_streaming -- --noplot` | pass, default 100-sample suite |
| `git diff --check` | pass |

The available terminal environment reports `TERM=xterm-256color`; Kitty, Sixel, and iTerm2 are not
installed, so protocol-hosted image dismissal could not be manually inspected. TestBackend covers
cell-buffer dismissal for 100 open/close cycles per popup and constrained popup rendering. Only
`aarch64-apple-darwin` is installed in rustup, so the Linux x86_64, Linux musl, and Windows CI
targets were not built locally. Peak process RSS was unavailable.
