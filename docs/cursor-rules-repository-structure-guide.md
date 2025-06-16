# Cursor Rules Repository Structure Guide

This document provides instructions for AI agents on how to structure a `cursor-rules` repository for end-to-end testing of the `cursor-rules-cli` tool, particularly the quick-add functionality.

## Repository Overview

A `cursor-rules` repository should contain:
1. **Rule files** (`.mdc` extension) - Cursor IDE configuration rules
2. **Manifest files** - Collections of rule files for bulk operations
3. **Directory structure** - Organized by technology, framework, or use case

## Required Directory Structure

```
cursor-rules/
├── README.md                           # Repository documentation
├── frontend/                           # Frontend technology rules
│   ├── react/
│   │   ├── react-core.mdc
│   │   ├── react-hooks.mdc
│   │   └── tailwind-react.mdc
│   ├── vue/
│   │   ├── vue-core.mdc
│   │   └── vue-composition.mdc
│   └── general/
│       ├── html-best-practices.mdc
│       └── css-modern.mdc
├── backend/                            # Backend technology rules
│   ├── rust/
│   │   ├── rust-general.mdc
│   │   ├── actix-web.mdc
│   │   └── tokio-async.mdc
│   ├── python/
│   │   ├── python-general.mdc
│   │   ├── fastapi.mdc
│   │   └── django.mdc
│   └── node/
│       ├── node-general.mdc
│       ├── express.mdc
│       └── typescript-node.mdc
├── devops/                             # DevOps and tooling rules
│   ├── docker.mdc
│   ├── kubernetes.mdc
│   └── ci-cd.mdc
├── quick-add/                          # Manifest files for bulk operations
│   ├── fullstack-react.txt             # Text manifest
│   ├── fullstack-vue.yaml              # YAML manifest
│   ├── backend-rust.json               # JSON manifest
│   ├── frontend-only.txt
│   ├── devops-complete.yaml
│   └── starter-pack.txt
└── QUICK_ADD_ALL.txt                   # Special manifest for all rules
```

## File Content Examples

### 1. Rule Files (.mdc)

Rule files should contain Cursor IDE configuration in markdown format. Here are examples:

**`frontend/react/react-core.mdc`**
```markdown
# React Core Development Rules

You are an expert React developer focused on writing clean, performant, and maintainable code.

## Core Principles
- Use functional components with hooks
- Prefer composition over inheritance
- Follow React best practices and patterns
- Use TypeScript for type safety

## Code Style
- Use arrow functions for components
- Destructure props at the component level
- Use meaningful component and variable names
- Keep components small and focused

## Performance
- Use React.memo for expensive components
- Implement proper dependency arrays in useEffect
- Avoid inline object/function creation in JSX
- Use useMemo and useCallback judiciously

## Error Handling
- Implement error boundaries for component trees
- Use proper error states in components
- Handle loading and error states explicitly
```

**`backend/rust/rust-general.mdc`**
```markdown
# Rust Development Best Practices

You are an expert Rust developer focused on writing safe, efficient, and idiomatic code.

## Core Principles
- Leverage Rust's ownership system effectively
- Use pattern matching extensively
- Prefer explicit error handling with Result<T, E>
- Write comprehensive tests

## Code Style
- Follow Rust naming conventions (snake_case, PascalCase)
- Use clippy and rustfmt consistently
- Document public APIs with /// comments
- Prefer iterators over manual loops

## Error Handling
- Use ? operator for error propagation
- Create custom error types when appropriate
- Use anyhow for application errors, thiserror for library errors
- Handle all Result and Option types explicitly

## Performance
- Prefer borrowing over cloning when possible
- Use Vec::with_capacity when size is known
- Consider using Cow<str> for string handling
- Profile before optimizing
```

**`devops/docker.mdc`**
```markdown
# Docker Best Practices

You are an expert in containerization with Docker, focused on creating efficient, secure, and maintainable containers.

## Dockerfile Best Practices
- Use multi-stage builds to reduce image size
- Use specific version tags, avoid 'latest'
- Run containers as non-root user
- Use .dockerignore to exclude unnecessary files

## Security
- Scan images for vulnerabilities
- Use minimal base images (alpine, distroless)
- Don't include secrets in images
- Use secrets management for sensitive data

## Performance
- Optimize layer caching
- Minimize the number of layers
- Use COPY instead of ADD when possible
- Clean up package manager caches
```

### 2. Text Manifest Files (.txt)

Text manifests list one rule file path per line. Comments start with `#`.

**`quick-add/fullstack-react.txt`**
```
# Fullstack React Development Stack
# Frontend rules
frontend/react/react-core.mdc
frontend/react/react-hooks.mdc
frontend/react/tailwind-react.mdc
frontend/general/html-best-practices.mdc

# Backend rules
backend/node/node-general.mdc
backend/node/express.mdc
backend/node/typescript-node.mdc

# DevOps basics
devops/docker.mdc
```

**`quick-add/frontend-only.txt`**
```
# Frontend Development Only
frontend/react/react-core.mdc
frontend/vue/vue-core.mdc
frontend/general/html-best-practices.mdc
frontend/general/css-modern.mdc
```

**`QUICK_ADD_ALL.txt`**
```
# Complete Cursor Rules Collection
# All available rules for comprehensive setup

# Frontend
frontend/react/react-core.mdc
frontend/react/react-hooks.mdc
frontend/react/tailwind-react.mdc
frontend/vue/vue-core.mdc
frontend/vue/vue-composition.mdc
frontend/general/html-best-practices.mdc
frontend/general/css-modern.mdc

# Backend
backend/rust/rust-general.mdc
backend/rust/actix-web.mdc
backend/rust/tokio-async.mdc
backend/python/python-general.mdc
backend/python/fastapi.mdc
backend/python/django.mdc
backend/node/node-general.mdc
backend/node/express.mdc
backend/node/typescript-node.mdc

# DevOps
devops/docker.mdc
devops/kubernetes.mdc
devops/ci-cd.mdc
```

### 3. YAML Manifest Files (.yaml/.yml)

YAML manifests use structured format with name, description, and rules array.

**`quick-add/fullstack-vue.yaml`**
```yaml
name: "Fullstack Vue Development"
description: "Complete Vue.js fullstack development setup with Node.js backend and modern tooling"
rules:
  - "frontend/vue/vue-core.mdc"
  - "frontend/vue/vue-composition.mdc"
  - "frontend/general/html-best-practices.mdc"
  - "frontend/general/css-modern.mdc"
  - "backend/node/node-general.mdc"
  - "backend/node/express.mdc"
  - "backend/node/typescript-node.mdc"
  - "devops/docker.mdc"
```

**`quick-add/devops-complete.yaml`**
```yaml
name: "Complete DevOps Setup"
description: "Comprehensive DevOps and infrastructure rules for modern development workflows"
rules:
  - "devops/docker.mdc"
  - "devops/kubernetes.mdc"
  - "devops/ci-cd.mdc"
```

### 4. JSON Manifest Files (.json)

JSON manifests follow the same schema as YAML but in JSON format.

**`quick-add/backend-rust.json`**
```json
{
  "name": "Rust Backend Development",
  "description": "Complete Rust backend development setup with async programming and web frameworks",
  "rules": [
    "backend/rust/rust-general.mdc",
    "backend/rust/actix-web.mdc",
    "backend/rust/tokio-async.mdc",
    "devops/docker.mdc"
  ]
}
```

## Manifest Priority Resolution

When multiple manifest files have the same basename (e.g., `config.txt`, `config.yaml`, `config.json`), the priority is:

1. **`.txt`** (highest priority)
2. **`.yaml`** / **`.yml`**
3. **`.json`** (lowest priority)

Example: If you have `starter.txt`, `starter.yaml`, and `starter.json`, the CLI will use `starter.txt`.

## Testing Scenarios

### Basic Testing
```bash
# Test manifest listing (should show available manifests)
cursor-rules --owner testorg quick-add nonexistent

# Test dry-run mode
cursor-rules --dry-run --owner testorg quick-add fullstack-react

# Test actual execution
cursor-rules --owner testorg quick-add frontend-only --force
```

### Priority Testing
Create multiple manifests with same basename to test priority resolution:

**`quick-add/test.txt`**
```
frontend/react/react-core.mdc
```

**`quick-add/test.yaml`**
```yaml
name: "Test YAML"
description: "Should not be used if .txt exists"
rules:
  - "backend/rust/rust-general.mdc"
```

**`quick-add/test.json`**
```json
{
  "name": "Test JSON",
  "description": "Should not be used if .txt or .yaml exists",
  "rules": ["devops/docker.mdc"]
}
```

### Error Testing
Include some invalid entries to test validation:

**`quick-add/error-test.txt`**
```
# Valid entries
frontend/react/react-core.mdc

# Invalid entries (should generate warnings/errors)
nonexistent/file.mdc
frontend/invalid.js
backend/rust/missing.mdc
```

## Repository Setup Checklist

When creating a test repository, ensure:

- [ ] All `.mdc` files contain valid Cursor rule content
- [ ] All manifest files reference existing `.mdc` files
- [ ] `quick-add/` directory exists with manifest files
- [ ] At least one `.txt`, one `.yaml`, and one `.json` manifest
- [ ] `QUICK_ADD_ALL.txt` references all available rules
- [ ] Repository is public or CLI has appropriate access
- [ ] Directory structure follows hierarchical organization
- [ ] File paths in manifests use forward slashes (`/`)
- [ ] No trailing whitespace in manifest entries
- [ ] Comments in `.txt` manifests start with `#`

## Best Practices for AI Agents

1. **Content Quality**: Make rule content realistic and useful, not just placeholder text
2. **Path Consistency**: Use consistent directory naming and path structures
3. **Manifest Variety**: Create manifests of different sizes (small, medium, large)
4. **Error Cases**: Include some invalid references for testing error handling
5. **Documentation**: Add clear README.md explaining the repository purpose
6. **Naming**: Use descriptive names for manifests and rule files
7. **Organization**: Group related rules logically by technology/domain

This structure provides comprehensive testing coverage for the `cursor-rules-cli` quick-add functionality while maintaining realistic, usable content. 