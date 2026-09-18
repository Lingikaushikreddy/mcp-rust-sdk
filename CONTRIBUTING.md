# Contributing to mcp-rust-sdk

Thank you for your interest in contributing to `mcp-rust-sdk`. This document provides guidelines and instructions for contributing.

## Development Setup

### Prerequisites

- Rust 1.87 or later (`rustup update stable`)
- Cargo (included with Rust)

### Building

```bash
# Build all workspace members
cargo build --workspace --all-features

# Run all tests
cargo test --workspace --all-features

# Run clippy lints
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Format code
cargo fmt --all
```

## Pull Request Process

1. Fork the repository and create a feature branch from `main`.
2. Ensure all tests pass: `cargo test --workspace --all-features`.
3. Ensure clippy is clean: `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
4. Ensure code is formatted: `cargo fmt --all -- --check`.
5. Add tests for any new functionality.
6. Update documentation for any public API changes.
7. Submit a pull request targeting the `main` branch.

## Code Standards

- All public items must have documentation comments (`///`).
- No `unwrap()` in library code; use `?` or explicit error handling.
- No `unsafe` in the public API.
- All types should derive `Debug` where possible.
- Use `thiserror` for error types.
- Use `tracing` for structured logging (never `println!` or `eprintln!` in library code).

## Testing

- Unit tests go in the module file with `#[cfg(test)]`.
- Integration tests go in `crates/<crate>/tests/`.
- Proc macro compile-fail tests go in `crates/mcp-macros/tests/ui/`.

## Commit Messages

Use conventional commit format:

```
feat: add SSE streaming support
fix: handle empty params in tools/call
docs: update quickstart guide
test: add protocol compliance tests
refactor: simplify transport trait
```

## License

By contributing, you agree that your contributions will be licensed under the MIT OR Apache-2.0 dual license.
