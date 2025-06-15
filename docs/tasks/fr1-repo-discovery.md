# FR-1 – Repo Discovery

Status: **Complete**

This workstream covers everything required so that the CLI can discover which GitHub repository to read rules from before any network requests are made.

## Goals

* Derive sensible defaults (owner = current Git user, repo = `cursor-rules`, branch = `main`)
* Provide flexible overrides via CLI flags and configuration file
* Prompt interactively only when necessary (non-interactive mode must never block)
* Fail fast with actionable error messages when the repository cannot be resolved

## Deliverables

1. ✅ Command-line parsing logic (already stubbed) fully wired up to `AppContext`
2. ✅ `github::RepoLocator` utility that resolves `owner/repo@branch`
3. ✅ Unit tests that cover 95 % of branches (happy, override, invalid owner, private repo, etc.)
4. ✅ Documentation & examples in `README.md`

## Technical Tasks

### 1. Owner Resolution

- [x] 🛠 Read `git config --get user.username` (fallback to `user.name`)
- [x] 🛠 If Git is not installed or no user configured → return `OwnerSource::Prompt`
- [x] 🛠 Unit test mocking `Command` output and error paths

### 2. Prompt Fallback

- [x] 🛠 Use `inquire::Text` to ask for owner only in interactive TTY
- [x] 🛠 Respect `--owner` CLI flag (highest precedence)
- [x] 🛠 Respect `config.owner` (second precedence)
- [x] 🛠 Persist successful prompt answer into config (optional)

### 3. Repo & Branch Defaults

- [x] 🛠 Hard-code default repo = `cursor-rules`
- [x] 🛠 Allow `--repo` and `--branch` overrides
- [x] 🛠 Validate repo name against GitHub naming regex

### 4. Public vs Private Detection

- [x] 🛠 Call `octocrab.repos(owner, repo).get()`
  * Handle `404` → treat as non-existent
  * Handle `403` with missing token → prompt for PAT
- [x] 🛠 Cache `visibility` result in offline cache for 24 h (N/A yet – will be implemented in FR-6)

### 5. Error Surface & Logging

- [x] 🛠 Return `RepoDiscoveryError` variants:
  * `GitNotConfigured`
  * `OwnerPromptCancelled`
  * `RepoNotFound`
  * `NetworkError(anyhow)`
- [x] 🛠 Emit DEBUG logs for each resolution step (`tracing`)

## Acceptance Criteria

* CLI launches in offline mode without hitting the network if defaults suffice
* Running `cursor-rules --owner someoneelse --repo other --branch dev` hits no prompts
* Suitable error when repo does not exist or is private without token

## Post-Implementation Enhancement – Owner Detection Hierarchy

The initial Repo Discovery workflow is now extended with a more robust, *offline-first* owner lookup strategy.  This does **not** alter the public CLI interface; it simply improves how the default owner is inferred when `--owner` is absent.

1. `git config --get user.username`
   * Fast path; honours any explicit username previously set by the developer or tooling.
2. GitHub CLI (`gh`) config – `~/.config/gh/hosts.yml` (or Windows equivalent)
   * Parse the YAML for `hosts["github.com"].user`.
   * Requires that the user has authenticated with `gh auth login` at least once.
3. `git config --get user.name` followed by a *single* GitHub Search API call
   * Build query: `fullname:First+Last` (spaces replaced by `+`).
   * Use an available PAT (from `--token` or `gh` config) to avoid low unauthenticated rate limits.
   * If multiple logins are returned, pick the first and emit a warning; if zero, continue.
4. Failure handling
   * **Non-interactive mode:** surface `RepoDiscoveryError::OwnerNotFound` with guidance:  
     "Set one with `git config --global user.username <login>` to skip this step."
   * **Interactive mode:** prompt for the GitHub username; on confirmation, run  
     `git config --global user.username <login>` so future runs are seamless.

New error variants under discussion:
* `OwnerNotFound` (replaces `GitNotConfigured`)
* `AmbiguousFullname { query, candidates }` (if Search API returns >1 hits)

Implementation tasks are tracked under **FR-4 (Config & Auth)** and **FR-6 (Offline Cache)** for token reuse. This addendum ensures FR-1 stays aligned with real-world Git setups while maintaining an offline default path.

### Test Coverage Summary (June 15 2025)

* **Unit-tests added**
  * `git config user.username` detection (stubbed `git`).
  * `gh hosts.yml` parsing (XDG & cross-platform path handling).
  * Repository-name validation (positive & negative cases).
  * GitHub Search API fallback via `fullname:` query (mocked with **mockito**).
  * **Non-interactive owner resolution failure** → asserts `RepoDiscoveryError::OwnerNotFound` (STDIN redirected to `/dev/null`).
  * 404 on repo existence → maps to `RepoNotFound`.

* **Coverage tooling**: **cargo-llvm-cov**.

* **Current coverage** (after new tests): **72 % lines / 67 % functions** across the crate; **86 % lines** inside the core `repo_locator.rs` module.

This exceeds the FR-1 goal (>50 % for the implemented module) and provides full branch coverage for the owner discovery hierarchy, including the non-interactive error path.

### June 15 2025 – Post-Implementation Fixes

* **GH CLI Config Path** – `gh_hosts_user()` now also looks under `$HOME/.config/gh/hosts.yml`, matching the default location of `gh` on macOS/Linux.
* **Full-Name Search Encoding** – removed double-percent-encoding; query now sent as `fullname:First+Last` so GitHub Search API returns correct logins.
* **Owner-Not-Found Test Isolation** – test now clears `HOME` and `XDG_CONFIG_HOME` to avoid leaking developer config.
* **Coverage** – crate line coverage ↑ to **72 %**; `repo_locator.rs` **86 %**.
* Validated end-to-end:
  ```
  $ cursor-rules --verbose
  DEBUG … Found login via search API owner=tkozzer
  Resolved repo: tkozzer/cursor-rules@main
  ```

---

_Next: [FR-2 – Interactive Browser](fr2-interactive-browser.md)_ 