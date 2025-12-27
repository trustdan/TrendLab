# Contributing to TrendLab

Thank you for your interest in contributing to TrendLab!

## Development Setup

1. **Install Rust** (stable toolchain)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Clone and build**
   ```bash
   git clone https://github.com/your-org/trendlab.git
   cd trendlab
   cargo build
   ```

3. **Run tests**
   ```bash
   cargo test
   ```

## Development Workflow

### Before You Code

1. Check existing issues or create a new one describing your change
2. For significant changes, discuss the approach first
3. Create a feature branch from `main`

### While Coding

1. **Write tests first** (BDD for behavior, unit for logic)
   ```bash
   # Add scenarios to crates/trendlab-bdd/tests/features/
   # Run BDD tests
   cargo test -p trendlab-bdd
   ```

2. **Follow the invariants** (see [architecture.md](docs/architecture.md))
   - No lookahead bias
   - Deterministic outputs
   - Accounting identity

3. **Keep changes focused** - one feature/fix per PR

### Before Submitting

Run the full quality gate:

```bash
# Format
cargo fmt

# Lint (must pass with no warnings)
cargo clippy --all-targets --all-features -- -D warnings

# Test
cargo test

# Check dependencies
cargo deny check  # (install with: cargo install cargo-deny)
```

## Code Style

- Follow `rustfmt` defaults
- Prefer explicit types at API boundaries
- Keep functions pure when possible
- Document public APIs with `///` comments
- Use `thiserror` for error types

## BDD Scenarios

New features should include BDD scenarios in `crates/trendlab-bdd/tests/features/`.

See [bdd-style.md](docs/bdd-style.md) for conventions:
- Use `@wip` tag for work in progress
- Use `@slow` tag for long-running scenarios
- Include deterministic test data in `fixtures/`

## Commit Messages

Use conventional commits:

```
feat: add ATR-based trailing stop
fix: correct Sharpe ratio calculation for negative returns
docs: update fill model assumptions
test: add BDD scenario for MA crossover
refactor: extract indicator calculations
```

## Pull Request Process

1. Fill out the PR template completely
2. Ensure CI passes (fmt, clippy, tests, deny)
3. Request review
4. Address feedback
5. Squash and merge when approved

## Architecture Decisions

For significant changes, consider creating an ADR in `docs/adr/`. See the [ADR README](docs/adr/README.md) for the template.

## Code Ownership

**Sign your work.** We believe in craftsmanship and accountability.

### Module Stewardship

Each major module has a steward responsible for:

- Reviewing PRs that touch that module
- Maintaining documentation and invariants
- Guiding architectural decisions

| Module | Steward | Scope |
|--------|---------|-------|
| `trendlab-core` | TBD | Domain types, backtest kernel, strategies |
| `trendlab-cli` | TBD | CLI interface, orchestration |
| `trendlab-bdd` | TBD | BDD scenarios, fixtures, step definitions |
| Data pipeline | TBD | Providers, caching, normalization |

Stewardship is about responsibility, not gatekeeping. Anyone can contribute anywhere.

### Commit Attribution

- Use your real name and email in git config
- Co-authored commits are welcome for pair programming
- AI-assisted code should note this in the commit message

### Pride in Craft

From *The Pragmatic Programmer*:

> "Pragmatic Programmers don't shirk from responsibility. Instead, we rejoice in accepting challenges and in making our expertise well known."

When you contribute:

- Write code you'd be proud to show
- Leave the codebase better than you found it
- If you see a broken window, fix it or file an issue

---

## Questions?

Open an issue with the `question` label or start a discussion.
