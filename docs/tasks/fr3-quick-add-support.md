# FR-3 – Quick-Add Support

Status: **Not started**

Enable bulk application of rule manifests so that users can copy many rules with a single command.

## Goals

* Recognise manifest files (`*.txt`, `*.yaml`, `*.yml`, `*.json`)
* Provide both interactive selection and non-interactive `quick-add <ID>` workflow
* Validate manifest entries and warn about missing files
* Support `--dry-run` mode for preview

## Deliverables

1. `github::manifests.rs` parser with format detection
2. CLI sub-command `quick-add` fully wired
3. Copy progress bar using `indicatif`
4. Unit tests for each manifest format parser

## Technical Tasks

### 1. Manifest Parsing

- [ ] 🛠 Parse `.txt` line-based manifests (ignore blank lines & comments starting with `#`)
- [ ] 🛠 Parse YAML/JSON array manifests into `Vec<String>`
- [ ] 🛠 Validate each path: must be `.mdc` and exist in repo tree
- [ ] 🛠 Return `Manifest` struct with `entries`, `errors`, `warnings`

### 2. CLI Interface

- [ ] 🛠 `cursor-rules quick-add <ID>` where `<ID>` can be:
  * Exact filename in `quick-add/` dir
  * Friendly slug (basename without extension)
- [ ] 🛠 `--owner`, `--repo`, etc. still respected
- [ ] 🛠 Interactive browse mode shortcut: press `Enter` on manifest → same code path

### 3. Dry-Run Mode

- [ ] 🛠 Collect copy plan (source → destination)
- [ ] 🛠 Render as table to stdout (col: source, dest, overwrite?)
- [ ] 🛠 Exit with code `0` when plan ok, `2` when validation errors

### 4. Copy Execution

- [ ] 🛠 Spawn tasks for each file; limit concurrency to 4 using Tokio semaphore
- [ ] 🛠 Display combined progress bar (`indicatif::MultiProgress`)
- [ ] 🛠 Honour `--force` overwrite behaviour

## Acceptance Criteria

* Running `cursor-rules quick-add fullstack` copies all files listed in `quick-add/fullstack.txt`
* Dry-run prints plan and makes no filesystem changes
* Validation errors clearly listed and process aborts if any fatal

---

_Previous: [FR-2 – Interactive Browser](fr2-interactive-browser.md) • Next: [FR-4 – Config & Auth](fr4-config-auth.md)_ 