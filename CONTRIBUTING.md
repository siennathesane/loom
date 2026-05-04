# Contributing to Loom

Thank you for your interest in contributing to Loom! Loom is a high-performance cryptocurrency arbitrage detection engine for Coinbase, written in Rust.

## License Notice

Loom is licensed under **AGPL-3.0-or-later**. By submitting a contribution, you agree that your work will be licensed under the same terms. The AGPL has strong copyleft requirements — code run as a network service must make its source available to users of that service. Please understand the implications before contributing. See [LICENSE](LICENSE) for details.

## Getting Started

### Prerequisites

- Rust 1.75 or later (see `rust-version` in `Cargo.toml`)
- A C toolchain (required for `rusqlite` bundled SQLite)
- Git
- For integration tests: Coinbase sandbox API credentials (optional — most tests are unit and protocol-level and run without credentials)

### Build the Workspace

```bash
git clone https://github.com/siennathesane/loom
cd loom
cargo build --workspace
```

### Run Tests

```bash
cargo test --workspace
```

Run tests for a specific crate:

```bash
cargo test -p loom-fix
cargo test -p loom-graph
cargo test -p loom-engine
```

- **loom-fix/tests/**: Protocol compliance, market data roundtrip, and session integration tests
- **loom-harness/tests/**: Full FIX protocol integration tests and Coinbase compliance validation
- **loom-daemon/tests/**: Daemon-level integration tests

For more detail, see the [Testing](README.md#testing) section in README.md.

## Workspace Layout

Loom is organized as a 10-crate Rust workspace:

- **loomctl**: CLI tool for interacting with the daemon via gRPC
- **loom-engine**: Core arbitrage detection engine with gravitational gating
- **loom-fix**: FIX 5.0 SP2 protocol client for Coinbase
- **loom-graph**: Graph data structure and cycle detection (Bellman-Ford, Dijkstra)
- **loom-harness**: FIX protocol test harness and compliance tests
- **loom-proto**: Protocol buffer definitions for gRPC
- **loom-server**: gRPC server for daemon control and event streaming
- **loom-store**: SQLite-backed opportunity logging
- **loom-config**: Configuration management (TOML parsing)
- **loom-daemon**: Main binary; connects to Coinbase FIX and runs the engine

For a deep architectural overview, see [ARCHITECTURE.md](ARCHITECTURE.md).

## Development Workflow

### Branching

- Branch from `main`
- Use descriptive branch names: `fix/...`, `feat/...`, `docs/...`
- Keep PRs focused — one logical change per PR

### Code Style

Before committing, run:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

Fix any clippy warnings and formatting issues.

### Testing

Add tests for new behavior:
- Unit tests: colocate with the code (`#[cfg(test)]` modules)
- Protocol tests: add to `loom-fix/tests/` or `loom-harness/tests/`
- Integration tests: use `loom-harness/tests/` for end-to-end scenarios

Verify all tests pass:

```bash
cargo test --workspace
```

### Commit Messages

- Use imperative mood: "Add cycle detection benchmark", not "Added"
- Keep the first line under 72 characters
- Explain *why*, not *what* in the body

Example:

```
Add gravitational gating safety margin check

Prevent false positives in cycle detection by validating that
potential arbitrage margins exceed the configured safety threshold.
Fixes #42.
```

### File Headers

Every Rust source file (`*.rs`) must include the SPDX license header:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024 Your Name
```

Update the copyright year if you modify existing files.

## Areas Where Help Is Welcome

- **Exchange backends**: Currently Coinbase-only via loom-fix. Additional exchange support is valuable.
- **Performance optimization**: We already hit our targets, but there's always room for improvement.
- **Cycle detection algorithms**: Additional or optimized algorithms beyond Bellman-Ford.
- **Production hardening**: Monitoring, alerting, error recovery, deployment automation.
- **Documentation**: Architecture guides, troubleshooting, operator runbooks.
- **Testing**: Additional edge cases, fuzz testing, stress testing.

## Reporting Issues

Open an issue at https://github.com/siennathesane/loom/issues with:

- Rust version: `rustc --version`
- OS and version
- Steps to reproduce
- Relevant logs (redact API credentials and secrets)
- Expected vs. actual behavior

## Security

If you discover a security vulnerability, **do not** open a public issue. Email security concerns to the project maintainer (contact details to be added).

## Code of Conduct

Be respectful. Disagreements about technical trade-offs are welcome; harassment is not.
