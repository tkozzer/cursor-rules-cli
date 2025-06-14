# cursor-rules-cli – v1 Design Document

---

## 0. Table of Contents

1. [Project Overview](#1-project-overview)
2. [Personas & Use‑Cases](#2-personas--use‑cases)
3. [Functional Requirements](#3-functional-requirements)
4. [Cursor‑Rules Repository Standard (consumed by CLI)](#4-cursor‑rules-repository-standard)
5. [CLI User Experience](#5-cli-user-experience)
6. [Command‑Line Interface Specification](#6-command‑line-interface-specification-powered-by-clap-v5)
7. [Internal Architecture](#7-internal-architecture)
8. [Key Data Structures](#8-key-data-structures)
9. [Concurrency & Event Flow](#9-concurrency--event-flow)
10. [Security & Privacy Considerations](#10-security--privacy-considerations)
11. [Error Handling & Logging](#11-error-handling--logging)
12. [Third‑Party Crate Selection](#12-third‑party-crate-selection)
13. [Build, CI, & Release Strategy](#13-build-ci--release-strategy)
14. [Testing & QA Plan](#14-testing--qa-plan)
15. [Roadmap & Milestones](#15-roadmap--milestones)
16. [License & Governance](#16-license--governance)

---

## 1. Project Overview

* **Project name:** `cursor-rules-cli`
* **Repository:** `github.com/tkozzer/cursor-rules-cli` (fresh repo)
* **Purpose:** Provide an interactive, cross‑platform Rust CLI that allows developers to browse **any** GitHub repo named `cursor-rules` (their own or others’) and copy selected `.mdc` rule files—or pre‑defined “quick‑add” bundles—into the directory where the CLI is executed (typically `./.cursor/rules`).
* **Scope (v1):** Interactive browsing; quick‑add bundles; PAT‑based auth for private repos; local config; overwrite prompts; binary releases for Linux/macOS/Windows.
* **Non‑Goals (v1):** Editing rule files in place; pushing changes back; rule templating; GUI.

## 2. Personas & Use‑Cases

| Persona                    | Needs                                                                                                       | Scenario                                                                                              |
| -------------------------- | ----------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| **Solo Dev** (default)     | Quickly copy a handful of rule files they maintain in their private `cursor-rules` repo into a new project. | Runs `cursor-rules` in project root, presses <Enter> to accept own Git user, navigates, copies files. |
| **Team Member**            | Pull standardized rules curated by an org account.                                                          | `cursor-rules --owner myorg` → chooses quick‑add manifest.                                            |
| **Open Source Maintainer** | Offer plug‑and‑play rule bundles to the community.                                                          | Publishes manifests in public repo. Users run `cursor-rules quick-add linting` to apply.              |

## 3. Functional Requirements

1. **Repo Discovery**

   * Default owner = `git config user.name` (fallback to prompt).
   * Repo name hard‑coded to `cursor-rules` but overrideable via `--repo`.
2. **Interactive Browser**

   * Arrow‑key tree navigation (Dir > File).
   * Breadcrumb display + status bar.
   * Select `.mdc` file → copy with confirmation.
   * Select manifest → bulk copy.
3. **Quick‑Add Support**

   * Recognise manifest files by extension (`*.txt|*.yaml|*.yml|*.json`).
   * Parse relative paths; verify existence; skip invalid lines with warning.
   * `--dry-run` prints table of target files without writing.
4. **Config & Auth**

   * PAT stored via OS keyring; fallback env `GITHUB_TOKEN`.
   * Persist defaults in XDG‑compliant config file (`~/.config/cursor-rules-cli/config.toml`).
5. **Copy Semantics**

   * Default destination `./.cursor/rules`, overridable via `--out`.
   * On name collision prompt: (o)verwrite / (s)kip / (r)ename / (a)ll / (c)ancel.
   * `--force` = overwrite all.
6. **Offline Cache**

   * Repo tree & blobs cached in `~/.cache/cursor-rules-cli/OWNER_HASH/` with ETag; expires after 24h unless `--refresh`.
7. **Telemetry (opt‑in)**

   * Simple anonymous usage ping to gauge adoption; disabled by default.

## 4. Cursor‑Rules Repository Standard

### 4.1 File Types

| Item                   | Description                                                          |                                            |
| ---------------------- | -------------------------------------------------------------------- | ------------------------------------------ |
| `*.mdc`                | Cursor rule markdown files (leaf).                                   |                                            |
| `quick-add/` directory | Optional folder of manifests.                                        |                                            |
| Manifest file          | `.txt` (newline list), \`.yaml                                       | .yml`, `.json\` containing array of paths. |
| `QUICK_ADD_ALL.txt`    | Reserved filename – declares paths to *all* rules for one‑click add. |                                            |

### 4.2 Directory Freedom

The CLI treats directory hierarchy as opaque; depth unlimited. Hidden files & dirs (`.*`) are shown but greyed out unless `--all`.

### 4.3 Example Layout

```
cursor-rules/
  frontend/
    react/
      react-core.mdc
      tailwind.mdc
    vue/
      vue-core.mdc
  backend/
    rust/
      actix.mdc
  quick-add/
    fullstack.txt
  QUICK_ADD_ALL.txt
```

## 5. CLI User Experience

### 5.1 Typical Flow (ASCII screenshots)

```
$ cursor-rules
? GitHub owner to fetch rules from  (tkozzer)
  ↳ press <Enter>
Fetching repo tree…  ✓  (cached)
┌ cursor-rules (root)
│  frontend/
│  backend/
│  QUICK_ADD_ALL.txt   ⋯ manifest (37 files)
│  quick-add/
└─────────────────────────────
 ↑/↓ move   → enter dir/select   ← back   ? help   q quit
```

* Pressing → on `QUICK_ADD_ALL.txt` triggers **Quick‑Add Summary** table then progress bar copying files.

### 5.2 Non‑Interactive Quick‑Add

```
$ cursor-rules quick-add QUICK_ADD_ALL.txt --owner myorg --force
Copied 42 rules → ./project/.cursor/rules
```

## 6. Command‑Line Interface Specification (powered by `clap` v5)

```
USAGE:
  cursor-rules [COMMAND] [OPTIONS]

COMMANDS:
  browse           Interactive browser (default)
  quick-add <ID>   Apply a manifest (ID = filename or friendly slug)
  list             Print repo tree in JSON/YAML
  config           Show or modify saved config
  cache            Manage offline cache (list|clear)
  completions      Generate shell completions
```

Global options: `--owner`, `--repo`, `--branch`, `--token`, `--out`, `--dry-run`, `--refresh`, `--verbose`, `--json`, `--version`.

## 7. Internal Architecture

```
src/
  main.rs               // Clap dispatch
  app.rs                // High-level orchestration (mode enums)
  ui/                   // Terminal UI (ratatui)
    viewport.rs         // Tree view & scrolling logic
    inputs.rs           // Key event -> Action mapping
  github/
    mod.rs
    client.rs           // Octocrab wrapper
    tree.rs             // Lazy tree builder & cache
    manifests.rs        // Manifest parser/validator
  copier.rs             // Download & write blobs, progress bars
  config.rs             // Persistent settings & keyring helpers
  logging.rs            // Tracing subscriber setup
  errors.rs             // thiserror + anyhow façade
```

Top‑level `AppContext` carries references to config, cache dir, Octocrab client, and tracing span.

## 8. Key Data Structures

```rust
pub enum NodeKind { Dir, RuleFile, Manifest }

pub struct RepoNode {
    name:     String,
    path:     String, // repo‑relative
    kind:     NodeKind,
    children: Vec<RepoNode>, // empty for leaf
}

pub struct Manifest {
    name:        String,
    description: Option<String>,
    entries:     Vec<String>, // relative paths
}
```

## 9. Concurrency & Event Flow

* **Async runtime:** Tokio (multi‑thread).
* **UI thread:** Crossterm event loop → mpsc channel → app state updates.
* **I/O tasks:** Fetch tree & blobs via Octocrab async; copy operations spawned as tasks with progress reported over channel.
* **Cache writes** off‑thread using `tokio::fs`.

## 10. Security & Privacy Considerations

* **GitHub PAT Storage:**

  * Saved via `keyring` crate (macOS Keychain, wincred, secret service).
  * Cli never prints token; `--token` overrides but is discouraged.
* **Path traversal:** Validate that downloaded file paths don’t traverse (`..`) outside destination dir.
* **Symlink ignore:** Skip symlinks in repo to avoid surprises.
* **Telemetry opt‑in:** COLLECT (cmd, duration) with SHA‑256 anonymised owner; respect `NO_TELEMETRY=1`.

## 11. Error Handling & Logging

* `tracing` crate with `--verbose` toggling DEBUG level.
* Errors bubble up via `anyhow::Result`; printed in red with codes.
* Common categories: NetworkError, AuthError, ParseError, IOError, ConflictError.

## 12. Third‑Party Crate Selection

| Concern            | Crate                               | Rationale                                     |
| ------------------ | ----------------------------------- | --------------------------------------------- |
| TUI                | **ratatui**                         | Actively maintained successor to tui‑rs.      |
| Terminal backend   | **crossterm**                       | Pure Rust, Windows & Unix.                    |
| GitHub API         | **octocrab**                        | Typed models, async, good rate‑limit helpers. |
| Prompts (fallback) | **inquire**                         | Nice multiline prompt UX.                     |
| CLI parser         | **clap** v5                         | Feature‑rich, derive‑based.                   |
| Progress bars      | **indicatif**                       | Multi‑bar support.                            |
| Config file        | **config** + **dirs**               | XDG & Windows aware.                          |
| Cache              | **reqwest-cache** (optional)        | ETag / file cache.                            |
| Keyring            | **keyring**                         | PAT secure storage.                           |
| Logging            | **tracing**, **tracing‑subscriber** | Structured logs.                              |

## 13. Build, CI, & Release Strategy

* **CI**: GitHub Actions matrix: `ubuntu-latest`, `macos-14`, `windows-2022`.

  * Steps: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `cargo build --release`.
* **Cross‑compile** with `cross` for musl static binaries.
* **Release workflow**: Tag with `vX.Y.Z` → build artifacts → upload to GH Releases → update Homebrew tap (`brew install tkozzer/tap/cursor-rules-cli`) and Scoop manifest.

## 14. Testing & QA Plan

| Layer       | Tooling                                                             | Coverage                    |
| ----------- | ------------------------------------------------------------------- | --------------------------- |
| Unit        | `cargo test` with `mockito` for GitHub mocks                        | >90% logic crates           |
| Integration | `expectrl` scripted TTY sessions                                    | browse, quick‑add scenarios |
| E2E (CI)    | Runs against a fixture public repo (`tkozzer/cursor-rules-fixture`) | assures GitHub calls work   |
| Static      | `cargo audit`, `cargo deny`, supply chain checks                    | every push                  |

## 15. Roadmap & Milestones

| Milestone       | Deliverables                                    | ETA      |
| --------------- | ----------------------------------------------- | -------- |
| **0.1.0‑alpha** | Minimal browser, copy single rule, public repos | 1 week   |
| **0.2.0**       | Quick‑add manifest support, overwrite prompts   | +1 week  |
| **0.3.0**       | PAT auth, config file, cache                    | +1 week  |
| **1.0.0**       | Stable CLI, binary releases, docs site          | +2 weeks |
| **Post‑1.0**    | Rule templating, push‑back PRs, WASI pkg        | backlog  |

## 16. License & Governance

* **License:** MIT OR Apache‑2.0 dual (same as Rust ecosystem norms).
* **CLA:** Not required initially; revisit if external contributors grow.
* **Contribution Guide:** Conventional Commits + automatic release notes via `cargo-release`.

---

*End of document.*
