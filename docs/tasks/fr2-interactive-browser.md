# FR-2 – Interactive Browser (TUI)

Status: **Not started**

Build an interactive terminal UI that allows users to navigate a remote GitHub repository tree, preview rule files, and select items for copying.

## Goals

* Smooth, key-driven navigation similar to a file explorer (↑ ↓ ← →)
* Breadcrumb path and status bar with helpful hints
* Real-time feedback for network latency (loading spinners)
* Accessibility: keyboard only, colour-blind friendly palette

## Deliverables

1. `ui/viewport.rs` – virtualised tree with scrolling support
2. `ui/inputs.rs` – key event → `AppAction` mapping layer
3. `AppState` updates handled over an `mpsc` channel
4. Integration tests scripted via `expectrl`
5. Screenshot GIFs for README

## Technical Tasks

### 1. Ratatui Setup

- [ ] 🛠 Initialise `ratatui` backend with `crossterm`
- [ ] 🛠 Define colour scheme constants (normal, selected, hidden)
- [ ] 🛠 Enable alternate screen + raw mode on start, restore on panic

### 2. Tree Rendering

- [ ] 🛠 Render `RepoNode` hierarchy lazily; fetch children only when expanded
- [ ] 🛠 Show folder icon 📁 for directories, 📄 for files, 📦 for manifests
- [ ] 🛠 Grey-out hidden entries unless `--all` flag is set
- [ ] 🛠 Display `[37 files]` bubble next to manifest files

### 3. Scrolling & Viewport

- [ ] 🛠 Keep selected item visible (auto-scroll)
- [ ] 🛠 Sticky breadcrumb at top (`owner/repo/path/...`)
- [ ] 🛠 Footer hint bar (`↑/↓ move → enter ← back q quit ? help`)

### 4. Input Handling

- [ ] 🛠 Map ArrowKeys & Vim keys (j/k/h/l) to actions
- [ ] 🛠 `Enter` → expand dir / select file
- [ ] 🛠 `Space` → mark for copy (multi-select future feature)
- [ ] 🛠 `?` → open inline help modal

### 5. File Preview (Optional stretch)

- [ ] 🛠 Show right-hand panel preview of `.mdc` file under cursor
- [ ] 🛠 Syntax highlight markdown with `syntect` (optional)

### 6. Progress & Error States

- [ ] 🛠 Inline spinner while loading children
- [ ] 🛠 Render error message overlay if GitHub API call fails

## Acceptance Criteria

* Arrow navigation never panics even with >5,000 nodes
* UI exits cleanly on Ctrl-C without leaving terminal in raw mode
* Selecting a `.mdc` file triggers a `CopyRequest` event (to be handled by copier)

---

_Previous: [FR-1 – Repo Discovery](fr1-repo-discovery.md) • Next: [FR-3 – Quick-Add Support](fr3-quick-add-support.md)_ 