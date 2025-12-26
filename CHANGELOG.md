# Changelog

All notable changes to TrendLab will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial project structure with workspace layout
- Core crate with domain types (Bar, Strategy, Metrics)
- CLI crate skeleton
- BDD test infrastructure with cucumber-rs
- Documentation: assumptions.md, schema.md, bdd-style.md
- CI pipeline with GitHub Actions
- cargo-deny configuration for dependency auditing

### Changed
- Nothing yet

### Deprecated
- Nothing yet

### Removed
- Nothing yet

### Fixed
- Nothing yet

### Security
- Nothing yet

## [0.1.0] - Unreleased

Initial release (planned).

### Planned Features
- Yahoo Finance data provider
- MA crossover strategy
- Basic metrics (CAGR, Sharpe, max drawdown)
- Parameter sweep capability
- Strategy artifact export

---

## Release Process

1. Update this CHANGELOG with release notes
2. Update version in `Cargo.toml` files
3. Create a git tag: `git tag -a v0.x.0 -m "Release v0.x.0"`
4. Push tag: `git push origin v0.x.0`
5. Create GitHub release with changelog excerpt
