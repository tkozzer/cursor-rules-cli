# QA – CI / Testing / Release

Status: **Not started**

Ensure high code quality, security, and reliable releases.

## Goals

* Automated linting, formatting, testing on GitHub Actions matrix (Linux/macOS/Windows)
* >90% unit test coverage on core logic crates
* Secure dependency supply chain (audit & deny checks)
* One-click release workflow that builds cross-platform binaries and publishes to crates.io & GitHub Releases

## Deliverables

1. `.github/workflows/ci.yml` – build, test, lint
2. `.github/workflows/release.yml` – tag trigger → build + upload binaries
3. `cargo deny` + `cargo audit` config files
4. Code coverage badge in README

## Technical Tasks

### 1. Lint & Format

- [ ] 🛠 Run `cargo fmt --check`
- [ ] 🛠 Run `cargo clippy -- -D warnings`
- [ ] 🛠 Fail CI if warnings present

### 2. Test Matrix

- [ ] 🛠 Run on `ubuntu-latest`, `macos-14`, `windows-2022`
- [ ] 🛠 Use cache `actions/cache@v4` for dependencies

### 3. Security Checks

- [ ] 🛠 `cargo audit` for vuln scan
- [ ] 🛠 `cargo deny --ban licenses` for license compliance

### 4. Release Workflow

- [ ] 🛠 Trigger on semver tag `vX.Y.Z`
- [ ] 🛠 Use `cross` to build static binaries (musl for Linux)
- [ ] 🛠 Upload artifacts to GitHub Release
- [ ] 🛠 `cargo publish` to crates.io (dry-run first)

### 5. Coverage

- [ ] 🛠 Use `cargo llvm-cov` to generate report
- [ ] 🛠 Upload to `codecov` or GitHub summary

## Acceptance Criteria

* All PRs require CI green before merge
* New releases appear on crates.io and GitHub Releases with binaries
* Vulnerability scan passes with 0 high severity issues

---

_Previous: [FR-7 – Telemetry](fr7-telemetry.md)_ 