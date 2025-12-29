# TrendLab Audit Quick Start Guide

## Overview

This guide provides a quick reference for executing the comprehensive audit plan in `AUDIT_PLAN.md`. Use this to get started quickly and track progress.

## Quick Reference: Entry Points

### Main Application Files (Start Here)

1. **CLI Mode**
   - Entry: `crates/trendlab-cli/src/main.rs`
   - Purpose: Command-line interface for data, sweep, report commands

2. **TUI Mode**
   - Entry: `crates/trendlab-tui/src/main.rs`
   - State: `crates/trendlab-tui/src/app.rs` (3842 lines - large!)
   - Worker: `crates/trendlab-tui/src/worker.rs`

3. **GUI Mode**
   - Entry: `apps/trendlab-gui/src-tauri/src/main.rs`
   - Commands: `apps/trendlab-gui/src-tauri/src/commands/`
   - Frontend: `apps/trendlab-gui/ui/src/`

4. **Launcher**
   - Entry: `crates/trendlab-launcher/src/main.rs`
   - Purpose: Unified entry point for TUI/GUI selection

## Critical Areas to Check First

### 1. Known TODOs (High Priority)

**Location**: `crates/trendlab-tui/src/worker.rs`
- Line 2107: `session_id: None, // TODO: Pass session_id from YoloState in Phase 1`
- Line 2163: `sector: None, // TODO: Look up sector from universe in Phase 2`
- Line 2169: `session_id: None, // TODO: Pass session_id from YoloState in Phase 1`

**Action**: Verify if these TODOs are blocking features or just incomplete metadata.

### 2. Error Handling (High Priority)

**Location**: Throughout codebase
- Found 1061 instances of `unwrap()`, `expect()`, `panic()`
- Many in GUI state access: `apps/trendlab-gui/src-tauri/src/commands/sweep.rs`

**Action**: Categorize:
- Test-only (safe)
- Safe in context (document)
- Should be replaced (fix)

### 3. Large Files (Medium Priority)

**Location**: `crates/trendlab-tui/src/app.rs`
- 3842 lines - consider refactoring
- Contains `#[allow(dead_code)]` - verify if code is actually dead

**Action**: Review for refactoring opportunities.

### 4. Indicator Caching (Low Priority)

**Location**: `crates/trendlab-core/src/indicator_cache.rs`
- Line 575: `// TODO: Implement caching for these if profiling shows benefit`

**Action**: Verify if profiling shows benefit needed.

## Testing Strategy

### Quick Smoke Tests

1. **TUI Mode**
   ```bash
   cargo run -p trendlab-tui --bin trendlab-tui
   ```
   - Test: Navigate all panels (1-6)
   - Test: Run a simple sweep
   - Test: View results and chart
   - Test: Keyboard shortcuts

2. **GUI Mode**
   ```bash
   cd apps/trendlab-gui/ui && npm install
   cargo tauri dev -c apps/trendlab-gui/src-tauri
   ```
   - Test: All panels load
   - Test: Data fetching
   - Test: Sweep execution
   - Test: Chart rendering
   - Test: Keyboard navigation

3. **CLI Mode**
   ```bash
   cargo run -p trendlab-cli -- data status --ticker SPY
   cargo run -p trendlab-cli -- sweep --strategy donchian --ticker SPY --start 2020-01-01 --end 2023-12-31
   ```

### BDD Tests

```bash
cargo test -p trendlab-bdd
```

Verify all feature files pass.

## Common Issues to Look For

### 1. State Management
- **Symptom**: State gets out of sync between panels
- **Check**: RwLock usage in GUI commands
- **Check**: Worker thread state updates in TUI

### 2. Error Handling
- **Symptom**: Errors are logged but not shown to user
- **Check**: Error display in TUI panels
- **Check**: Error display in GUI components

### 3. Data Flow
- **Symptom**: Selected items don't persist across panels
- **Check**: State persistence in TUI app.rs
- **Check**: State persistence in GUI Zustand slices

### 4. Type Mismatches
- **Symptom**: Runtime errors about missing fields
- **Check**: TypeScript types vs Rust types in GUI
- **Check**: Serialization/deserialization

### 5. Memory Leaks
- **Symptom**: Memory usage grows over time
- **Check**: Event listener cleanup in GUI
- **Check**: Chart instance cleanup
- **Check**: Worker thread resource cleanup

## Audit Execution Order

### Phase 1: Quick Wins (1-2 hours)
1. Review TODOs and fix obvious ones
2. Fix obvious error handling issues (unwrap() in production paths)
3. Fix documentation inconsistencies

### Phase 2: Core Functionality (4-6 hours)
1. Test all entry points
2. Test core workflows (data → strategy → sweep → results → chart)
3. Verify invariants (no lookahead, accounting identity)

### Phase 3: UI Issues (4-6 hours)
1. Test TUI all panels and shortcuts
2. Test GUI all panels and shortcuts
3. Compare GUI vs TUI behavior

### Phase 4: Deep Dive (8+ hours)
1. Review error handling comprehensively
2. Review state management
3. Review data flow
4. Review code quality

## Tools & Commands

### Code Search
```bash
# Find all TODOs
grep -r "TODO\|FIXME\|XXX\|HACK" --include="*.rs" --include="*.ts" --include="*.tsx"

# Find all unwrap/expect/panic
grep -r "unwrap(\|expect(\|panic(\|unreachable!" --include="*.rs"

# Find dead code markers
grep -r "#\[allow(dead_code)\]" --include="*.rs"
```

### Testing
```bash
# Run all tests
cargo test

# Run BDD tests
cargo test -p trendlab-bdd

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_name
```

### Linting
```bash
# Format code
cargo fmt

# Check for issues
cargo clippy --all-targets --all-features -D warnings
```

## Issue Tracking Template

For each issue found, document:

```markdown
### Issue #X: [Title]

**Severity**: Critical/High/Medium/Low
**Type**: Bug/Missing Feature/Inconsistency/Documentation/Technical Debt
**Phase**: [Phase number from audit plan]

**Location**: 
- File: `path/to/file.rs`
- Line: 123

**Description**: 
[What's wrong]

**Steps to Reproduce**:
1. [Step 1]
2. [Step 2]

**Expected Behavior**:
[What should happen]

**Actual Behavior**:
[What actually happens]

**Impact**:
[Who/what is affected]

**Fix Priority**: 
[When should this be fixed]

**Notes**:
[Additional context]
```

## Next Steps

1. Read `AUDIT_PLAN.md` for comprehensive details
2. Set up issue tracking (use template above)
3. Start with Phase 1 (Entry Points)
4. Work through phases systematically
5. Document all findings
6. Prioritize fixes

## Questions to Answer

As you audit, try to answer:

1. **What works?** (Document working features)
2. **What's broken?** (Document bugs)
3. **What's missing?** (Document missing features)
4. **What's inconsistent?** (Document inconsistencies)
5. **What's confusing?** (Document unclear code/docs)
6. **What's slow?** (Document performance issues)

## Getting Help

- Review `docs/debugging.md` for common issues
- Review `docs/architecture.md` for system design
- Review `docs/PROJECT_OVERVIEW.md` for project goals
- Check BDD tests for expected behavior examples

