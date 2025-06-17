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
- ⏳ **FR-6: Offline Cache** - Local caching for improved performance
- ⏳ **FR-7: Telemetry** - Optional usage analytics

### Quality Assurance
- ⏳ **QA: CI/Testing/Release** - Automated testing and release pipeline

### Version Milestones
- **v0.1.4**: Config & authentication complete ✅
- **v0.1.5**: Copy semantics & file conflict resolution complete ✅ **(Current)**
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

# Configuration management
cursor-rules config                          # Show current config
cursor-rules config set owner myorg         # Set default owner
cursor-rules config set token ghp_xyz123    # Store GitHub token securely
cursor-rules config delete owner            # Remove config value
```

### Commands

- `browse` - Interactive browser (default)
- `quick-add <ID>` - Apply a manifest (ID = filename or friendly slug)
- `list` - Print repo tree in JSON/YAML
- `config` - Show or modify saved config
  - `config` - Display current configuration
  - `config set <key> <value>` - Set configuration value
  - `config delete <key>` - Remove configuration value
- `cache` - Manage offline cache (list|clear) *(coming soon)*
- `completions` - Generate shell completions *(coming soon)*

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

- **`github/tree.rs`**: 90.94% lines (repository tree handling)
- **`github/repo_locator.rs`**: 88.13% lines (GitHub repository discovery)  
- **`ui/viewport.rs`**: 96.88% lines (terminal viewport component)
- **`ui/inputs.rs`**: 85.71% lines (keyboard input handling)
- **`github/manifests.rs`**: 81.64% lines (manifest parsing and validation)
- **`copier.rs`**: 82.24% lines (file copying and progress tracking) ✨ *Enhanced in FR-5*
- **`ui/prompts.rs`**: 81.45% lines (interactive conflict resolution) ✨ *New in FR-5*
- **`config.rs`**: 84.67% lines (configuration and authentication)

**Overall: 74.06% line coverage with 140 passing tests**

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

**Current Version: 0.1.5 - Advanced copy semantics implemented**

FR-5 (Copy Semantics) has been completed with comprehensive test coverage and enterprise-grade file handling, bringing the project significantly closer to the v0.2.0 stable release target.

### Recently Implemented
- ✅ **Interactive repository browsing** - Terminal UI with tree navigation
- ✅ **Quick-add manifest support** - Bulk rule installation via manifest files  
- ✅ **GitHub authentication** - Secure token storage with keyring integration
- ✅ **Configuration management** - Persistent settings with XDG compliance
- ✅ **Advanced copy semantics** - Atomic file operations, conflict resolution, security validation ✨ *New in FR-5*
- ✅ **Interactive prompts** - Smart conflict handling with overwrite/skip/rename options ✨ *New in FR-5*

### Next Priorities
- ⏳ **Offline caching** - Local caching for improved performance
- ⏳ **Telemetry** - Optional usage analytics
- ⏳ **Shell completions** - Bash, Zsh, Fish support

The CLI now provides a robust foundation for managing Cursor rules across projects! 