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
- ✅ **FR-4: Config & Authentication** - GitHub token management and settings
- ✅ **FR-5: Copy Semantics** - File conflict resolution and overwrite handling
- ✅ **FR-6: Offline Cache** - Local caching for improved performance ✨ *Complete*
- ⏳ **FR-7: Telemetry** - Optional usage analytics

### Quality Assurance
- ⏳ **QA: CI/Testing/Release** - Automated testing and release pipeline

### Version Milestones
- **v0.1.4**: Config & authentication complete ✅
- **v0.1.5**: Copy semantics & file conflict resolution complete ✅
- **v0.1.6**: Offline cache & code quality improvements complete ✅ **(Current)**
- **v0.2.0**: First stable release with all core features **(Target - 1 feature remaining)**

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

# Configuration management
cursor-rules config                          # Show current config
cursor-rules config set owner myorg         # Set default owner
cursor-rules config set token ghp_xyz123    # Store GitHub token securely
cursor-rules config delete owner            # Remove config value

# Cache management (offline support)
cursor-rules cache list                     # List all cached repositories
cursor-rules cache clear                    # Clear all cached data
cursor-rules --refresh browse               # Force refresh cache
```

### Commands

- `browse` - Interactive browser (default)
- `quick-add <ID>` - Apply a manifest (ID = filename or friendly slug)
- `list` - Print repo tree in JSON/YAML
- `config` - Show or modify saved config
  - `config` - Display current configuration
  - `config set <key> <value>` - Set configuration value
  - `config delete <key>` - Remove configuration value
- `cache` - Manage offline cache (list|clear)
- `completions` - Generate shell completions *(coming soon)*

### Options

- `--owner, -o` - GitHub owner to fetch rules from
- `--repo, -r` - Repository name (defaults to 'cursor-rules')
- `--branch, -b` - Branch to fetch from (defaults to 'main')
- `--out, -o` - Output directory (defaults to './.cursor/rules')
- `--dry-run` - Show what would be done without making changes
- `--force` - Force overwrite without prompting
- `--verbose, -v` - Verbose output
- `--refresh` - Force refresh cache and bypass local data

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

- **`ui/viewport.rs`**: 96.88% lines (terminal viewport component) ⭐
- **`github/cache.rs`**: 92.45% lines (persistent caching system) ✨ *Complete in FR-6*
- **`github/repo_locator.rs`**: 88.13% lines (GitHub repository discovery) ⭐
- **`ui/inputs.rs`**: 85.71% lines (keyboard input handling) ⭐
- **`config.rs`**: 84.67% lines (configuration and authentication) ⭐
- **`copier.rs`**: 82.37% lines (file copying and progress tracking) ✨ *Enhanced in FR-5*
- **`github/tree.rs`**: 82.13% lines (repository tree handling) ✨ *Enhanced in FR-6*
- **`ui/prompts.rs`**: 81.45% lines (interactive conflict resolution) ✨ *New in FR-5*
- **`github/manifests.rs`**: 80.57% lines (manifest parsing and validation) ⭐

**Overall: 75.73% line coverage with 164 passing tests (163 unit + 1 integration)**

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

**Current Version: 0.1.6 - Offline caching and code quality improvements**

FR-6 (Offline Cache) has been completed with exceptional test coverage and production-ready caching infrastructure. The project is now very close to the v0.2.0 stable release with only telemetry remaining.

### Recently Implemented
- ✅ **Interactive repository browsing** - Terminal UI with tree navigation
- ✅ **Quick-add manifest support** - Bulk rule installation via manifest files  
- ✅ **GitHub authentication** - Secure token storage with keyring integration
- ✅ **Configuration management** - Persistent settings with XDG compliance
- ✅ **Advanced copy semantics** - Atomic file operations, conflict resolution, security validation ✨ *FR-5*
- ✅ **Interactive prompts** - Smart conflict handling with overwrite/skip/rename options ✨ *FR-5*
- ✅ **Offline caching system** - Local repository tree and blob caching with XDG compliance ✨ *FR-6*
- ✅ **Cache management** - List, clear, and refresh cached repositories ✨ *FR-6*
- ✅ **Rate limit handling** - Smart GitHub API rate limiting with exponential backoff ✨ *FR-6*

### Next Priorities
- ⏳ **Telemetry** - Optional usage analytics *(Final feature for v0.2.0)*
- ⏳ **Shell completions** - Bash, Zsh, Fish support
- ⏳ **Release automation** - CI/CD pipeline for stable releases

The CLI now provides a robust, production-ready foundation for managing Cursor rules across projects with comprehensive caching and excellent performance! 