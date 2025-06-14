# FR-4 – Config & Authentication

Status: **Not started**

Handle persistent CLI configuration and secure storage of GitHub Personal Access Token (PAT).

## Goals

* XDG-compliant config file (`~/.config/cursor-rules-cli/config.toml`)
* PAT stored in OS keyring (macOS Keychain, wincred, secret-service)
* CLI flags always override config values
* Non-interactive mode must never prompt for token

## Deliverables

1. `config.rs` module with load/save helpers
2. `keyring` integration behind a small abstraction
3. Sub-command `cursor-rules config` (view, set, unset)
4. Comprehensive error messages & troubleshooting tips in README

## Technical Tasks

### 1. Config File Schema

- [ ] 🛠 Define `Config` struct (`owner`, `repo`, `token_alias`, `out_dir`, `telemetry`)
- [ ] 🛠 Use `dirs::config_dir()` to find path; create dir if missing
- [ ] 🛠 Parse & write using `config` crate with TOML backend

### 2. Keyring Wrapper

- [ ] 🛠 Implement `SecretStore` trait with `get_token()`, `set_token()`, `delete_token()`
- [ ] 🛠 Handle keyring locked errors gracefully (especially on Linux)
- [ ] 🛠 Optional env override `GITHUB_TOKEN`

### 3. CLI Sub-command

- [ ] 🛠 `cursor-rules config` → print table of current values
- [ ] 🛠 `cursor-rules config set owner myorg`
- [ ] 🛠 `cursor-rules config delete token` with confirmation prompt

### 4. Token Validation Flow

- [ ] 🛠 When API returns 401, prompt to save new token (interactive only)
- [ ] 🛠 Validate scopes (`repo`, `read:org`) once after save

## Acceptance Criteria

* Persistent config survives restarts across OSes
* `cursor-rules --token xxxx` never writes token to file or logs
* Clear instructions printed when PAT is missing or invalid

---

_Previous: [FR-3 – Quick-Add Support](fr3-quick-add-support.md) • Next: [FR-5 – Copy Semantics](fr5-copy-semantics.md)_ 