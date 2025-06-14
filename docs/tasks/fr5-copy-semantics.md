# FR-5 – Copy Semantics

Status: **Not started**

Implement the logic that downloads rule files from GitHub and writes them to the local destination, handling filename collisions gracefully.

## Goals

* Safe, atomic writes with temp files → rename
* Flexible overwrite behaviour (prompt, skip, rename, all, cancel)
* Path traversal protection (`..` outside dest dir)
* Progress reporting per file & overall

## Deliverables

1. `copier.rs` module with `copy_rule()` async function
2. Conflict resolution prompt workflow (shared with Quick-Add)
3. Unit tests using tempdir for filesystem safety
4. Integration test copying 100+ files concurrently

## Technical Tasks

### 1. Destination Path Resolution

- [ ] 🛠 Ensure destination dir exists; create recursively
- [ ] 🛠 Sanitize filenames (Windows reserved characters)
- [ ] 🛠 Reject paths containing `..` or absolute paths

### 2. Overwrite Strategy

- [ ] 🛠 Enum `OverwriteMode` (Prompt, Skip, Force, Rename, Cancel)
- [ ] 🛠 `--force` flag sets `Force`
- [ ] 🛠 In interactive prompt: present `(o)verwrite / (s)kip / (r)ename / (a)ll / (c)ancel`
- [ ] 🛠 Maintain `all` choice in `AppState` to avoid repeated prompts

### 3. Download & Write

- [ ] 🛠 Fetch file blob from GitHub via `octocrab.repos.get_content()`
- [ ] 🛠 Use temp file inside dest dir, write bytes, then `rename`
- [ ] 🛠 Parallel downloads with limited concurrency (respect GitHub API rate limits)

### 4. Progress Bars

- [ ] 🛠 Use `indicatif::MultiProgress` – one bar per file and global bar
- [ ] 🛠 Update ETA dynamically
- [ ] 🛠 Clear bars on completion and print summary

## Acceptance Criteria

* Copying aborts with clear error if path traversal attempt detected
* Overwrite prompt behaves correctly for each choice
* Copy 500 files in < N parallel tasks without exceeding GitHub rate limits

---

_Previous: [FR-4 – Config & Auth](fr4-config-auth.md) • Next: [FR-6 – Offline Cache](fr6-offline-cache.md)_ 