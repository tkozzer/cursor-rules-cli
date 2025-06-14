# FR-7 – Telemetry (Opt-In)

Status: **Not started**

Anonymous usage statistics to help gauge adoption and improve UX while respecting user privacy.

## Goals

* Collect minimal data: command name, duration, success/failure
* Send only when user has explicitly opted in (`telemetry = true` in config)
* Respect `NO_TELEMETRY=1` env var and corporate networks
* Use non-blocking background task so CLI exit is not delayed

## Deliverables

1. `telemetry.rs` module with `emit(Event)` helper
2. Simple HTTP endpoint (GitHub gist or serverless) – out of scope, mock for now
3. README privacy section

## Technical Tasks

### 1. Event Schema

- [ ] 🛠 Define `TelemetryEvent` struct (`uuid`, `cmd`, `duration_ms`, `success`, `timestamp`, `owner_hash`, `version`)
- [ ] 🛠 SHA-256 hash owner to avoid PII

### 2. Opt-In Flow

- [ ] 🛠 On first run, ask `Opt-in to anonymous usage stats? (y/N)`
- [ ] 🛠 Persist answer in config
- [ ] 🛠 Provide `cursor-rules config telemetry {on|off}` command

### 3. Dispatch Mechanism

- [ ] 🛠 Spawn `tokio::spawn` task that sends POST request using `reqwest`
- [ ] 🛠 Timeout after 2 seconds to avoid hanging
- [ ] 🛠 Store unsent events in cache and retry next run

### 4. Unit & Integration Tests

- [ ] 🛠 Use `mockito` to assert correct payload
- [ ] 🛠 Verify no network calls when telemetry disabled

## Acceptance Criteria

* No telemetry sent unless user opted in
* CLI exits within <50 ms extra overhead even when endpoint down
* Clear docs on what is collected & how to disable

---

_Previous: [FR-6 – Offline Cache](fr6-offline-cache.md) • Next: [QA – CI / Testing / Release](qa-ci-testing-release.md)_ 