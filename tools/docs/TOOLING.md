# Pinned Documentation Tooling

Single source of truth for the docs pipeline binaries (task T004). Consumed by
`.github/workflows/docs.yml` (install step) and `specs/006-mdbook-docs-site/quickstart.md`
(Prerequisites). Versions are pinned with published SHA256 checksums; the CI install
step MUST verify checksums before use (no unverified downloads).

## mdBook — v0.5.4

| Platform | Asset | SHA256 |
| --- | --- | --- |
| CI: x86_64-linux-gnu | `mdbook-v0.5.4-x86_64-unknown-linux-gnu.tar.gz` | `3f28de05dafca9d0f2eab99c662116b0e37b89b1d96a08f8f430b9eeae958cd7` |
| Local: arm64 macOS | `mdbook-v0.5.4-aarch64-apple-darwin.tar.gz` | `03e8a6d8b13a2971e0b3280affd03b388373c1485e26f73407c3a76b0b1838df` |

Base URL: `https://github.com/rust-lang/mdBook/releases/download/v0.5.4/`

```bash
# macOS arm64 (local) — verify then extract into tools/bin/
curl -sL -o mdbook.tar.gz \
  https://github.com/rust-lang/mdBook/releases/download/v0.5.4/mdbook-v0.5.4-aarch64-apple-darwin.tar.gz
echo "03e8a6d8b13a2971e0b3280affd03b388373c1485e26f73407c3a76b0b1838df  mdbook.tar.gz" | shasum -a 256 -c -
tar xzf mdbook.tar.gz -C tools/bin   # yields ./mdbook
```

## mdbook-mermaid — v0.17.1

| Platform | Asset | SHA256 |
| --- | --- | --- |
| CI: x86_64-linux-gnu | `mdbook-mermaid-v0.17.1-x86_64-unknown-linux-gnu.tar.gz` | `9afcfa5b8463afe606d48595a7ae338564302903e626ea5b6edb8007d29393a5` |
| Local: arm64 macOS | `mdbook-mermaid-v0.17.1-aarch64-apple-darwin.tar.gz` | `5be76bfffefd36efe5abf876c52530ba03c16df5f44c94164cc9ca88d16df1e1` |

Base URL: `https://github.com/badboy/mdbook-mermaid/releases/download/v0.17.1/`

## Usage

- `docs/book.toml` registers `[preprocessor.mermaid] command = "mdbook-mermaid"`, so the
  binary must be on `PATH` when running `mdbook build docs` locally. Either export
  `PATH="$PWD/tools/bin:$PATH"` or `cargo install mdbook-mermaid --version 0.17.1`.
- The CI workflow installs both binaries into `$GITHUB_PATH` after checksum verification.
- `mdbook-mermaid install docs` (run once at setup, task T003) vendors `mermaid.min.js`
  and `mermaid-init.js` into the tracked `docs/` tree; `mermaid-init.js` is pinned to the
  `neutral` theme. Re-run the install command only to refresh vendored assets.
- Python tooling (`tools/docs/*.py`) requires Python 3.11+ and uses the standard library
  only.
