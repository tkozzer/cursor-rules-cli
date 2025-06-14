# FR-6 – Offline Cache

Status: **Not started**

Implement caching layer for repo tree & blobs to minimise GitHub API traffic and enable offline browsing.

## Goals

* Cache directory per `OWNER_REPO_HASH` in `~/.cache/cursor-rules-cli/`
* Store ETag and Last-Modified headers to validate freshness
* Automatically expire after 24 h unless `--refresh` flag is used
* Work seamlessly with async GitHub client

## Deliverables

1. `github::cache.rs` module with `fetch_or_cache()` helper
2. Cache invalidation logic (time-based + `--refresh`)
3. Unit tests mocking GitHub responses (`mockito`)

## Technical Tasks

### 1. Directory Layout

- [ ] 🛠 Compute SHA-1 of `owner/repo` for dir name
- [ ] 🛠 Sub-dirs: `tree/` (JSON), `blobs/` (raw)
- [ ] 🛠 Write `meta.json` with `fetched_at` & `etag`

### 2. Tree Caching

- [ ] 🛠 On first request, fetch full tree, write JSON
- [ ] 🛠 Subsequent runs: if `<24 h` and no `--refresh`, read from disk
- [ ] 🛠 If `--refresh` or stale, send `If-None-Match` header; update on `200`

### 3. Blob Caching

- [ ] 🛠 Save each blob as `{sha}.mdc` in `blobs/`
- [ ] 🛠 Before fetching, check if already on disk
- [ ] 🛠 Honour GitHub `X-RateLimit-Remaining` to back-off

### 4. Concurrency & Locks

- [ ] 🛠 Use file lock (advisory) to avoid concurrent writes from multiple instances
- [ ] 🛠 Release lock promptly after writes

## Acceptance Criteria

* Running twice in a row hits zero GitHub API calls (when cache fresh)
* `--refresh` forces revalidation
* Corrupted cache files auto-remove and re-download

---

_Previous: [FR-5 – Copy Semantics](fr5-copy-semantics.md) • Next: [FR-7 – Telemetry](fr7-telemetry.md)_ 