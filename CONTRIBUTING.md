# Contributing to RSS Proxy

Thank you for your interest in contributing to RSS Proxy! This document provides guidelines and instructions for contributing to the project.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Running Tests](#running-tests)
- [Code Style](#code-style)
- [Submitting a Pull Request](#submitting-a-pull-request)
- [Adding New Plugins](#adding-new-plugins)

---

## Code of Conduct

This project adheres to a code of conduct that expects all contributors to be respectful, inclusive, and professional. Please be courteous and constructive in all interactions.

---

## Getting Started

1. Fork the repository on GitHub
2. Clone your fork locally:
   ```bash
   git clone https://github.com/<your-username>/rss-proxy.git
   cd rss-proxy
   ```
3. Add the upstream remote:
   ```bash
   git remote add upstream https://github.com/rust-rss-proxy/rss-proxy.git
   ```

---

## Development Setup

### Prerequisites

- Rust 1.75 or later ([rustup.rs](https://rustup.rs/))
- Cargo (included with Rust)
- A modern web browser for testing the proxy server

### Building from Source

```bash
# Build the project
cargo build

# Build for release
cargo build --release
```

### Running the Server Locally

```bash
# Start with default config
cargo run -- serve

# Start with custom config and port
cargo run -- serve --config config.yaml --port 3000 --host 127.0.0.1
```

---

## Running Tests

Before submitting a pull request, ensure all tests pass:

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run tests in release mode
cargo test --release

# Run specific test
cargo test <test_name>
```

### Testing Plugins

To test custom Rhai plugins:

```bash
# Start server with plugins directory
cargo run -- serve --config config.yaml

# Test plugin endpoints
curl http://localhost:8080/plugins
```

---

## Code Style

### Rust Formatting

We use `rustfmt` for code formatting. Please format your code before submitting:

```bash
cargo fmt --all
```

### Clippy Lints

Run clippy to catch common pitfalls and improve code quality:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

### Commit Message Format

We follow conventional commit messages:

```
<type>(<scope>): <subject>

<body>

<footer>
```

Types include:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting, etc.)
- `refactor`: Code refactoring
- `test`: Adding or modifying tests
- `chore`: Maintenance tasks

Example:
```
feat(filters): add max_age_days filter support

Implemented the max_age_days filter option in FeedFilterConfig
to allow filtering items by their publication age.

Closes #123
```

---

## Submitting a Pull Request

1. Ensure your branch is up-to-date with `upstream/main`:
   ```bash
   git fetch upstream
   git rebase upstream/main
   ```

2. Make sure all tests pass and code is formatted:
   ```bash
   cargo test
   cargo fmt --all
   cargo clippy --all-targets --all-features -- -D warnings
   ```

3. Push your branch to your fork:
   ```bash
   git push origin <branch-name>
   ```

4. Open a pull request on GitHub, referencing any related issues.

---

## Adding New Plugins

RSS Proxy supports two types of plugins:

### Built-in Plugins (Rust)

To add a new built-in plugin:

1. Create a new module in `src/plugin/builtin/`
2. Implement the `Plugin` trait from `src/plugin/mod.rs`
3. Register the plugin in `src/plugin/builtin/mod.rs`
4. Add tests for your plugin

### Rhai Script Plugins

To add a new Rhai script plugin:

1. Create a `.rhai` file in the `plugins/` directory
2. Export functions with the `export` keyword
3. Functions should accept `item` (feed item) and return `item` or `null` (to filter out)

Example plugin structure:

```rhai
// plugins/my_custom_filter.rhai

export function filter_spam(item) {
    let spam_words = ["viagra", "casino", "lottery"];
    for word in spam_words {
        if item.title.contains(word) || item.content.contains(word) {
            return null; // Filter out this item
        }
    }
    return item; // Keep this item
}
```

---

## Questions?

If you have questions or need help, please:

1. Check existing issues and pull requests
2. Open a new issue with the `question` label
3. Reach out to the maintainers

Thank you for contributing to RSS Proxy!
