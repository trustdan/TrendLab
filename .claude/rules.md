# Operational Rules for Claude in TrendLab

## Agent Delegation

Delegate immediately when a task matches an agent's domain. Do not attempt complex domain work without consulting the appropriate agent first.

| Task Domain | Agent to Use |
|-------------|--------------|
| Crate structure, traits, APIs, Rust architecture | `rust-architect` |
| Polars pipelines, Parquet I/O, performance | `polars-expert` |
| Strategy design, parameter grids, robustness | `trend-following-expert` |
| Data ingestion, Yahoo Finance, caching | `data-provider-expert` |
| BDD scenarios, cucumber-rs, step definitions | `bdd-test-author` |
| Metrics calculations, ranking, cost sensitivity | `metrics-analyst` |
| StrategyArtifact format, Pine parity | `pine-artifact-writer` |

## Code Quality Gates

Before any PR or significant commit:
1. `cargo fmt` — must pass
2. `cargo clippy --all-targets --all-features -D warnings` — must pass
3. `cargo test` — must pass (including BDD tests)

Use `/dev:release-check` to run all gates and produce a summary.

## Test-First Development

1. Write or extend `.feature` scenarios BEFORE implementing behavior
2. Never weaken tests to make code pass
3. Use fixtures from `fixtures/` — keep them small (20-200 bars)
4. Add invariants: no lookahead, accounting identities, determinism

## Data Handling

- **Never** read market data with eager Polars methods — use `scan_parquet` (lazy)
- **Never** commit anything in `data/` — it's gitignored
- **Always** use fixtures for tests, not real market data
- **Always** log data provenance (provider, fetch timestamp, version)

## Assumptions Are Code

When changing any assumption (fill convention, timezone, adjusted prices), you must:
1. Update `docs/assumptions.md`
2. Add or update BDD scenarios that enforce the assumption
3. Review all code that depends on the assumption

## Pine Parity

Every strategy that might become a Pine script must:
1. Emit a `StrategyArtifact` JSON (schema in `schemas/`)
2. Include parity test vectors (timestamps + expected values)
3. Document fill convention and cost model explicitly

## Common Mistakes to Avoid

- Using eager Polars reads instead of `scan_parquet`
- Adding tests that depend on live market data
- Weakening tests to make failing code pass
- Changing assumptions without updating docs and tests
- Creating files in `data/` that should be in `fixtures/`
