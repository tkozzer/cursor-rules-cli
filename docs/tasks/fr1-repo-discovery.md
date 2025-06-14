# FR-1 – Repo Discovery

Status: **Not started**

This workstream covers everything required so that the CLI can discover which GitHub repository to read rules from before any network requests are made.

## Goals

* Derive sensible defaults (owner = current Git user, repo = `cursor-rules`, branch = `main`)
* Provide flexible overrides via CLI flags and configuration file
* Prompt interactively only when necessary (non-interactive mode must never block)
* Fail fast with actionable error messages when the repository cannot be resolved

## Deliverables

1. Command-line parsing logic (already stubbed) fully wired up to `AppContext`
2. `github::RepoLocator` utility that resolves `owner/repo@branch`
3. Unit tests that cover 95 % of branches (happy, override, invalid owner, private repo, etc.)
4. Documentation & examples in `README.md`

## Technical Tasks

### 1. Owner Resolution

- [ ] 🛠 Read `git config --get user.username` (fallback to `user.name`)
- [ ] 🛠 If Git is not installed or no user configured → return `OwnerSource::Prompt`
- [ ] 🛠 Unit test mocking `Command` output and error paths

### 2. Prompt Fallback

- [ ] 🛠 Use `inquire::Text` to ask for owner only in interactive TTY
- [ ] 🛠 Respect `--owner` CLI flag (highest precedence)
- [ ] 🛠 Respect `config.owner` (second precedence)
- [ ] 🛠 Persist successful prompt answer into config (optional)

### 3. Repo & Branch Defaults

- [ ] 🛠 Hard-code default repo = `cursor-rules`
- [ ] 🛠 Allow `--repo` and `--branch` overrides
- [ ] 🛠 Validate repo name against GitHub naming regex

### 4. Public vs Private Detection

- [ ] 🛠 Call `octocrab.repos(owner, repo).get()`
  * Handle `404` → treat as non-existent
  * Handle `403` with missing token → prompt for PAT
- [ ] 🛠 Cache `visibility` result in offline cache for 24 h

### 5. Error Surface & Logging

- [ ] 🛠 Return `RepoDiscoveryError` variants:
  * `GitNotConfigured`
  * `OwnerPromptCancelled`
  * `RepoNotFound`
  * `NetworkError(anyhow)`
- [ ] 🛠 Emit DEBUG logs for each resolution step (`tracing`)

## Acceptance Criteria

* CLI launches in offline mode without hitting the network if defaults suffice
* Running `cursor-rules --owner someoneelse --repo other --branch dev` hits no prompts
* Suitable error when repo does not exist or is private without token

---

_Next: [FR-2 – Interactive Browser](fr2-interactive-browser.md)_ 