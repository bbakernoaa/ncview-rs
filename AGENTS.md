# Agent Guidelines for ncview-rs

`ncview-rs` (`ncv`) is a terminal-native, high-performance scientific data viewer for NetCDF, GRIB2, and VirtualiZarr/Icechunk datasets built in Rust using `ratatui`, `crossterm`, `ndarray`, and `rayon`.

---

## 1. Test-Driven Development (TDD) Workflow

* **Practice TDD:** When adding new features, format parsers, or fixing bugs, write or update failing tests *first* before implementing the solution:
  1. **Red:** Write a failing test in `tests/` or a module `#[cfg(test)]` block that specifies the expected behavior, edge cases, or bug condition.
  2. **Green:** Implement the minimal code required to make the test pass cleanly.
  3. **Refactor:** Clean up the implementation while ensuring all unit and integration tests remain passing and clippy checks pass.
* **Test Isolation:** Ensure tests are deterministic, standalone, and cleanup temporary test artifacts using `tempfile`.

---

## 2. Testing Guidelines

* **Run Full Test Suite:** Before submitting changes, always run:
  ```bash
  cargo test
  ```
* **Run Specific Integration Tests:**
  ```bash
  cargo test --test <test_name>
  ```
* **Test Coverage:**
  * Any new feature, data reader, or bug fix must include automated tests in `tests/` or unit tests within module `mod tests`.
  * Test edge cases such as empty dimensions, boundary conditions, sub-region bounds, multi-chunk spanning, endianness, and missing chunks/fill values.

---

## 3. Formatting and Linting

* **Code Formatting:** Code must be formatted with `rustfmt`:
  ```bash
  cargo fmt --all -- --check
  ```
  Run `cargo fmt --all` to apply automatic formatting.

* **Clippy Lints:** Code must compile with zero warnings under strict clippy settings:
  ```bash
  cargo clippy --all-targets --all-features -- -D warnings
  ```
  Fix any warnings or explicitly document necessary lint overrides with `#[allow(...)]`.

---

## 4. Semantic Versioning, Commit, and PR Conventions

Commit messages and PR titles must adhere to [Conventional Commits](https://www.conventionalcommits.org/) to support semantic versioning (`MAJOR.MINOR.PATCH`):

* `feat: ...` – New features (triggers `MINOR` release bump)
* `fix: ...` – Bug fixes (triggers `PATCH` release bump)
* `docs: ...` – Documentation changes only
* `style: ...` – Code formatting or style adjustments (no logic change)
* `refactor: ...` – Code refactoring without behavioral change
* `perf: ...` – Performance improvements
* `test: ...` – Adding or updating tests
* `ci: ...` – CI/CD workflow updates
* `chore: ...` – Tooling or dependency maintenance
* `feat!: ...` or `BREAKING CHANGE:` – Breaking API changes (triggers `MAJOR` release bump)

### Examples:
* `feat: Add support for VirtualiZarr and Icechunk reference manifests`
* `fix: Correct multi-chunk stride calculation for sub-region slice requests`
* `docs: Add TDD, testing, and semantic versioning guidelines to AGENTS.md`

---

## 5. Best Practices for Rust Code in ncview-rs

1. **Safety and Robustness:**
   * Prefer safe Rust. Avoid `unsafe` blocks unless strictly necessary and thoroughly documented.
   * Avoid `panic!`, `unwrap()`, or `expect()` in production data decoding and UI event paths. Use `?` and propagate errors via `NcvError` (`crate::error::Result`).

2. **Concurrency and UI Isolation:**
   * Keep heavy I/O, network requests, and array decoding off the main UI rendering thread. Use background worker threads (`std::sync::mpsc` channels and `Arc<AtomicBool>` cancellation flags).

3. **Performance and Data Fidelity:**
   * Use `ndarray` for array manipulations and `rayon` for parallel CPU rasterization/projection calculations.
   * Preserve non-finite floating point values (`NaN`, `PosInf`, `NegInf`) and missing data masks so scientific diagnostics remain accurate.
