# v0.2.0 – Personal-Access-Token Support & Authentication

Status: **Planned** (not part of v0.1.0 scope – browse + quick-add only)

---

## Motivation

* Support private `cursor-rules` repositories and higher GitHub rate-limits.
* Allow users to store a Personal-Access-Token (PAT) securely and reuse it across CLI invocations.
* Provide fallbacks for public-only usage (no token) and CI environments (env var).

## Goals

1. Transparent authentication when a token is present (automatic use).
2. Interactive prompt to save a PAT on first need.
3. Secure storage in OS keychain (macOS Keychain, Windows Credential Manager, Secret Service).
4. Respect `GITHUB_TOKEN` env-var (ephemeral) over stored credential.
5. `cursor-rules config auth` sub-command to view/remove token.
6. Clear messaging when a private repo is inaccessible without token.

## Functional Requirements

1. **Token Discovery Order**
   1. `--token <VAL>` CLI flag (session-only)
   2. `GITHUB_TOKEN` env-var
   3. Stored secret in keychain
   4. _None found_ → unauthenticated requests

2. **Saving a Token**
   * When GitHub returns HTTP 401/404 on a private repo, prompt:
     ```
     Private repository detected – provide a GitHub PAT? (y/N)
     ```
   * On `y`, open `inquire::Password` prompt (masked).
   * Validate token with a trivial call (`GET /user`), then write to keychain.

3. **Keychain Integration**
   * Use `keyring` crate.
   * Service name: `cursor-rules-cli`.
   * Account key: `<github_owner>` (_different token per owner_).
   * Store/retrieve/update transparently.

4. **Config Command**
   * `cursor-rules config auth` → shows whether a token is stored for default owner.
   * Flags: `--owner <login>` override, `--remove` to delete.

5. **CI / Non-Interactive Mode**
   * If `stdin` not a TTY and no token found → fail with clear message pointing to `GITHUB_TOKEN`.

6. **Security**
   * Never print token characters in logs.
   * Redact token in panic messages.

## Non-Goals (v0.2.0)

* OAuth device-flow login.
* Token generation wizards.
* Enterprise GitHub endpoints (will gracefully fall back).

## Implementation Notes

* Extend `github::repo_locator::build_octocrab` to accept an optional token string resolved by the discovery order above.
* Add `src/auth.rs` helper encapsulating keyring logic.
* Update FR-1, FR-2, FR-3 networking code to call `auth::resolve_token(owner)` before hitting GitHub.
* Unit-test keyring round-trips with an in-memory secret service mock.

---

_Related tasks_: [FR-4 – Config & Authentication](fr4-config-auth.md) will be merged with this plan at implementation time. 