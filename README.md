# cursor-rules

[![Crate](https://img.shields.io/crates/v/cursor-rules.svg)](https://crates.io/crates/cursor-rules)
[![Documentation](https://docs.rs/cursor-rules/badge.svg)](https://docs.rs/cursor-rules)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

> 🚧 **Development in Progress** - Building toward v0.2.0 stable release

A CLI tool for managing Cursor rules from GitHub repositories.

## 📋 Development Progress

### Core Features
- ✅ **FR-1: Repository Discovery** - Auto-detect GitHub owner/repo with fallbacks
- ✅ **FR-2: Interactive Browser** - Terminal UI for browsing repository trees  
- ✅ **FR-3: Quick-Add Support** - Bulk copy rules via manifest files
- ⏳ **FR-4: Config & Authentication** - GitHub token management and settings
- ⏳ **FR-5: Copy Semantics** - File conflict resolution and overwrite handling
- ⏳ **FR-6: Offline Cache** - Local caching for improved performance
- ⏳ **FR-7: Telemetry** - Optional usage analytics

### Quality Assurance
- ⏳ **QA: CI/Testing/Release** - Automated testing and release pipeline

### Version Milestones
- **v0.1.3**: Quick-add functionality complete ✅ **(Current)**
- **v0.2.0**: First stable release with all core features **(Target)**

## Overview

`cursor-rules` is an interactive, cross-platform Rust CLI that allows developers to browse GitHub repositories named `cursor-rules` and copy selected `.mdc` rule files into their projects. It provides an easy way to share and manage Cursor IDE configuration rules across different projects.

## Installation

### From crates.io

```bash
cargo install cursor-rules
```

### From source

```bash
git clone https://github.com/tkozzer/cursor-rules-cli.git
cd cursor-rules-cli
cargo install --path .
```

## Usage

### Basic Usage

```bash
# Interactive browse mode (default)
cursor-rules

# Browse a specific owner's repository
cursor-rules --owner myorg

# Quick-add a specific manifest
cursor-rules quick-add QUICK_ADD_ALL.txt --owner myorg

# List available rules
cursor-rules list --owner myorg
```

### Commands

- `browse` - Interactive browser (default)
- `quick-add <ID>` - Apply a manifest (ID = filename or friendly slug)
- `list` - Print repo tree in JSON/YAML
- `config` - Show or modify saved config
- `cache` - Manage offline cache (list|clear)
- `completions` - Generate shell completions

### Options

- `--owner, -o` - GitHub owner to fetch rules from
- `--repo, -r` - Repository name (defaults to 'cursor-rules')
- `--branch, -b` - Branch to fetch from (defaults to 'main')
- `--out, -o` - Output directory (defaults to './.cursor/rules')
- `--dry-run` - Show what would be done without making changes
- `--force` - Force overwrite without prompting
- `--verbose, -v` - Verbose output

## Repository Structure

Your `cursor-rules` repository should follow this structure:

```
cursor-rules/
├── frontend/
│   ├── react/
│   │   ├── react-core.mdc
│   │   └── tailwind.mdc
│   └── vue/
│       └── vue-core.mdc
├── backend/
│   └── rust/
│       └── actix.mdc
├── quick-add/
│   └── fullstack.txt
└── QUICK_ADD_ALL.txt
```

## Development

### Test Coverage

This project uses `cargo-llvm-cov` for comprehensive test coverage reporting.

#### Installation

First, install the `cargo-llvm-cov` tool:

```bash
cargo install cargo-llvm-cov
```

#### Running Coverage Analysis

To check test coverage with a summary report:

```bash
cargo llvm-cov --summary-only
```

For detailed line-by-line coverage in HTML format:

```bash
cargo llvm-cov --html
```

This will generate an HTML report in `target/llvm-cov/html/index.html` that you can open in your browser.

#### Current Coverage Status

The project maintains excellent test coverage across core modules:

- **`copier.rs`**: 92.66% lines (file copying and progress tracking)
- **`github/tree.rs`**: 92.43% lines (repository tree handling)
- **`github/repo_locator.rs`**: 87.60% lines (GitHub repository discovery)
- **`github/manifests.rs`**: 81.64% lines (manifest parsing and validation)
- **`ui/viewport.rs`**: 96.88% lines (terminal viewport component)
- **`ui/inputs.rs`**: 85.71% lines (keyboard input handling)

**Overall: 73.65% line coverage with 77 passing tests**

Lower coverage in CLI entry points (`main.rs`) and interactive UI code is expected, as these components are primarily integration-tested through end-to-end scenarios.

### Running Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test module
cargo test github::manifests
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Development Status

This is currently a barebones implementation. Full functionality is coming soon!

**Current Version: 0.1.1 - Basic CLI structure only**

### Planned Features

- Interactive repository browsing
- Quick-add manifest support
- GitHub authentication
- Offline caching
- Shell completions
- And much more!

Stay tuned for updates! 