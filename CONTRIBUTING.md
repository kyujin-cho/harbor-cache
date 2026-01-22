# Contributing to Harbor Cache

Thank you for your interest in contributing to Harbor Cache! This document provides guidelines and instructions for contributing.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Project Structure](#project-structure)
- [Coding Standards](#coding-standards)
- [Testing](#testing)
- [Submitting Changes](#submitting-changes)
- [Pull Request Process](#pull-request-process)
- [Issue Guidelines](#issue-guidelines)

## Code of Conduct

This project follows the [Contributor Covenant](https://www.contributor-covenant.org/) code of conduct. By participating, you are expected to uphold this code. Please report unacceptable behavior to the project maintainers.

## Getting Started

### Prerequisites

Before you begin, ensure you have the following installed:

- **Rust 1.84+** - Install via [rustup](https://rustup.rs/)
- **Node.js 18+** - For frontend development
- **Docker** - For testing and running the test Harbor instance
- **Git** - For version control

### Fork and Clone

1. Fork the repository on GitHub
2. Clone your fork locally:
   ```bash
   git clone https://github.com/your-username/harbor-cache.git
   cd harbor-cache
   ```
3. Add the upstream remote:
   ```bash
   git remote add upstream https://github.com/lablup/harbor-cache.git
   ```

## Development Setup

### Backend (Rust)

```bash
# Build the project
cargo build

# Run in development mode
cargo run -- --config config/default.toml

# Check code without building
cargo check

# Format code
cargo fmt

# Run linter
cargo clippy
```

### Frontend (Vue.js)

```bash
cd frontend

# Install dependencies
npm install

# Run development server
npm run dev

# Build for production
npm run build

# Type check
npx vue-tsc --noEmit
```

### Test Harbor Instance

A local Harbor instance is required for testing:

```bash
cd harbor-setup/harbor

# Start Harbor
docker compose up -d

# Verify it's running
curl http://localhost:8880/api/v2.0/health

# Stop Harbor
docker compose down
```

**Harbor Credentials:**
- URL: http://localhost:8880
- Username: admin
- Password: Harbor12345

### Full Development Workflow

1. Start the test Harbor instance:
   ```bash
   cd harbor-setup/harbor && docker compose up -d
   ```

2. Build and run Harbor Cache:
   ```bash
   cargo run -- --config config/default.toml
   ```

3. Access the web UI at http://localhost:5001

4. Run tests:
   ```bash
   ./tests/e2e-test.sh
   ```

## Project Structure

```
harbor-cache/
├── crates/                     # Rust workspace crates
│   ├── harbor-cache/           # Main binary (CLI, server setup)
│   ├── harbor-core/            # Core business logic
│   │   └── src/
│   │       ├── cache/          # Cache manager and policies
│   │       └── registry.rs     # Registry service
│   ├── harbor-storage/         # Storage backends
│   │   └── src/
│   │       ├── local.rs        # Local filesystem storage
│   │       └── s3.rs           # S3 storage
│   ├── harbor-proxy/           # Upstream Harbor client
│   ├── harbor-api/             # Axum REST API routes
│   ├── harbor-auth/            # JWT and password handling
│   └── harbor-db/              # SQLite database layer
├── frontend/                   # Vue.js web UI
│   └── src/
│       ├── views/              # Page components
│       ├── api/                # API client
│       └── stores/             # Pinia state stores
├── config/                     # Configuration files
├── docs/                       # Documentation
├── tests/                      # End-to-end tests
└── harbor-setup/               # Test Harbor environment
```

### Crate Dependencies

```
harbor-cache (binary)
├── harbor-api
│   ├── harbor-core
│   │   ├── harbor-storage
│   │   ├── harbor-proxy
│   │   └── harbor-db
│   └── harbor-auth
└── (configuration, CLI)
```

## Coding Standards

### Rust

**Formatting:**
- Use `cargo fmt` before committing
- Follow the default rustfmt configuration

**Linting:**
- Run `cargo clippy` and address all warnings
- Use `#[allow(clippy::...)]` sparingly and with justification

**Code Style:**
- Use descriptive variable and function names
- Write documentation comments for public APIs
- Keep functions focused and reasonably sized
- Use `Result` for fallible operations
- Avoid `unwrap()` in library code; use proper error handling

**Example:**
```rust
/// Fetches a manifest from the cache or upstream registry.
///
/// # Arguments
/// * `name` - Repository name (e.g., "library/nginx")
/// * `reference` - Tag or digest
///
/// # Returns
/// The manifest bytes and content type, or an error.
pub async fn get_manifest(
    &self,
    name: &str,
    reference: &str,
) -> Result<(Bytes, String), RegistryError> {
    // Implementation
}
```

**Error Handling:**
- Define specific error types per crate
- Use `thiserror` for error definitions
- Provide context in error messages

### TypeScript/Vue

**Formatting:**
- Use 2-space indentation
- Single quotes for strings
- Semicolons at end of statements

**Code Style:**
- Use TypeScript strict mode
- Define proper types (avoid `any`)
- Use Composition API with `<script setup>`
- Keep components focused and reusable

**Example:**
```vue
<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { cacheApi, type CacheStats } from '../api/client'

const stats = ref<CacheStats | null>(null)
const loading = ref(true)

async function fetchStats() {
  loading.value = true
  try {
    const response = await cacheApi.getStats()
    stats.value = response.data
  } finally {
    loading.value = false
  }
}

onMounted(fetchStats)
</script>
```

### Commit Messages

Follow conventional commit format:

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting)
- `refactor`: Code changes that neither fix bugs nor add features
- `test`: Adding or modifying tests
- `chore`: Maintenance tasks

**Examples:**
```
feat(cache): add LFU eviction policy

Implements Least Frequently Used eviction as an alternative to LRU.
Tracks access counts per entry and evicts entries with lowest counts.

Closes #42
```

```
fix(api): handle empty manifest list correctly

Previously returned 500 error when manifest list had no manifests.
Now returns proper MANIFEST_UNKNOWN error.
```

## Testing

### Unit Tests

Run Rust unit tests:

```bash
cargo test
```

Run tests for a specific crate:

```bash
cargo test -p harbor-core
```

### End-to-End Tests

The e2e test suite requires Harbor and Harbor Cache running:

```bash
# Start Harbor
cd harbor-setup/harbor && docker compose up -d

# Start Harbor Cache (in another terminal)
cargo run -- --config config/default.toml

# Run all tests
./tests/e2e-test.sh

# Run specific test suite
./tests/e2e-test.sh basic
./tests/e2e-test.sh multiarch
./tests/e2e-test.sh cache
```

### Writing Tests

**Unit Tests:**
- Place in `#[cfg(test)]` module within the source file
- Test individual functions in isolation
- Mock external dependencies

**Integration Tests:**
- Add to `tests/e2e-test.sh` for API tests
- Document prerequisites and expected state

## Submitting Changes

### Branch Naming

Use descriptive branch names following this convention:

```
<type>/<short-description>
```

**Examples:**
- `feature/s3-storage-backend`
- `bugfix/manifest-list-handling`
- `docs/update-api-reference`
- `refactor/cache-manager`

### Creating a Pull Request

1. **Create a feature branch:**
   ```bash
   git checkout -b feature/your-feature
   ```

2. **Make your changes:**
   - Write code
   - Add tests
   - Update documentation

3. **Ensure quality:**
   ```bash
   cargo fmt
   cargo clippy
   cargo test
   ./tests/e2e-test.sh
   ```

4. **Commit your changes:**
   ```bash
   git add .
   git commit -m "feat(scope): description"
   ```

5. **Push to your fork:**
   ```bash
   git push origin feature/your-feature
   ```

6. **Open a Pull Request** on GitHub

## Pull Request Process

### PR Requirements

Before submitting a PR, ensure:

- [ ] All tests pass (`cargo test`)
- [ ] Code is formatted (`cargo fmt`)
- [ ] No clippy warnings (`cargo clippy`)
- [ ] E2E tests pass (if applicable)
- [ ] Documentation is updated
- [ ] Commit messages follow convention

### PR Template

When creating a PR, include:

```markdown
## Description
Brief description of changes

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation update

## Testing
How was this tested?

## Checklist
- [ ] Tests added/updated
- [ ] Documentation updated
- [ ] CHANGELOG updated (if applicable)
```

### Review Process

1. **Automated checks** run on all PRs
2. **Maintainer review** for code quality and design
3. **Address feedback** with additional commits
4. **Squash merge** to main branch

### Merge Strategy

We use **squash merge** for all pull requests to maintain a clean commit history on the main branch.

## Issue Guidelines

### Reporting Bugs

When reporting a bug, include:

1. **Environment:**
   - Harbor Cache version
   - Operating system
   - Docker version
   - Rust version (if building from source)

2. **Steps to reproduce:**
   - Exact commands or actions
   - Expected behavior
   - Actual behavior

3. **Logs:**
   - Relevant log output (with `RUST_LOG=debug`)
   - Error messages

4. **Configuration:**
   - Relevant config settings (redact secrets)

### Feature Requests

When requesting a feature:

1. **Use case:** Describe the problem you're solving
2. **Proposed solution:** How you envision the feature
3. **Alternatives:** Other ways you've considered
4. **Priority:** How important is this to you?

### Questions

For questions:

1. Check existing documentation first
2. Search closed issues for similar questions
3. Open an issue with the "question" label

## Getting Help

- **Documentation:** Start with the [README](README.md) and [docs/](docs/) folder
- **Issues:** Search existing issues or open a new one
- **Discussions:** Use GitHub Discussions for questions

## Recognition

Contributors are recognized in:

- GitHub contributors list
- Release notes (for significant contributions)

Thank you for contributing to Harbor Cache!
