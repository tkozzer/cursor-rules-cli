# cursor-rules

[![Crate](https://img.shields.io/crates/v/cursor-rules.svg)](https://crates.io/crates/cursor-rules)
[![Documentation](https://docs.rs/cursor-rules/badge.svg)](https://docs.rs/cursor-rules)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A CLI tool for managing Cursor rules from GitHub repositories.

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

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Development Status

This is currently a barebones implementation. Full functionality is coming soon!

**Current Version: 0.1.0 - Basic CLI structure only**

### Planned Features

- Interactive repository browsing
- Quick-add manifest support
- GitHub authentication
- Offline caching
- Shell completions
- And much more!

Stay tuned for updates! 