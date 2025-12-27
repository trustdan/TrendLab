# Debugging Guide

Common failure modes and how to diagnose them.

---

## BDD Scenarios Failing

### Fixture not found

```
Failed to open fixture "fixtures/synth/foo.csv": No such file or directory
```

**Cause:** Fixture path is relative to `crates/trendlab-bdd/`.

**Fix:** Ensure the path in your step uses `../../fixtures/synth/foo.csv` or adjust `load_fixture_csv()` in `bdd.rs`.

### Timestamp parse error

```
Failed to parse ts '2024-01-01' in fixture: input contains invalid characters
```

**Cause:** Timestamps must be RFC3339 format with timezone.

**Fix:** Use `2024-01-01T00:00:00Z`, not `2024-01-01`.

### Assertion mismatch on floating point

```
expected 100.0 ≈ 100.00000001 (diff 0.00000001 > eps 1e-10)
```

**Cause:** Floating point accumulation error.

**Fix:** Use appropriate epsilon for the assertion. For financial calculations, `1e-8` is usually sufficient.

---

## Backtest Errors

### "initial_cash must be > 0"

**Cause:** `BacktestConfig::initial_cash` is zero or negative.

**Fix:** Set a positive initial cash value.

### "exit fill without an entry fill"

**Cause:** Strategy emitted `ExitLong` while position was `Flat`.

**Fix:** Check strategy logic — ensure `ExitLong` only fires when `current_position == Position::Long`.

### Accounting identity failure

```
Accounting identity failed at equity index 5: expected 100500.0 ≈ 100499.99
```

**Cause:** Cash or position tracking is out of sync with fills.

**Fix:** Verify that:
1. Entry fills deduct `qty * price + fees` from cash
2. Exit fills add `qty * price - fees` to cash
3. Position quantity updates match fill quantity

---

## Indicator Issues

### SMA returns all `None`

**Cause:** Window size is larger than bar count, or window is 0.

**Fix:** Ensure `window > 0` and `bars.len() >= window`.

### Lookahead detected

If the no-lookahead BDD scenario fails:

```
SMA mismatch at 15: 102.5 vs 1025.0
```

**Cause:** Indicator is using bars beyond the current index.

**Fix:** Audit the indicator loop. Values at index `t` must only use `bars[0..=t]`.

---

## CI Failures

### cargo fmt --check fails

```
Diff in src/foo.rs
```

**Fix:** Run `cargo fmt` locally before committing.

### clippy warnings treated as errors

```
error: this could be simplified
```

**Fix:** Address the clippy lint or, if justified, add `#[allow(clippy::...)]` with a comment explaining why.

### Tests pass locally but fail in CI

**Possible causes:**
1. **Path separators:** Windows uses `\`, Linux uses `/`. Use `std::path::Path` or `PathBuf`.
2. **Timezone:** CI runners may use UTC. Ensure fixtures use explicit UTC timestamps.
3. **File ordering:** `fs::read_dir` order is not guaranteed. Sort if determinism matters.

---

## Data Issues

### Parquet scan returns empty DataFrame

**Cause:** Predicate pushdown filtered everything, or wrong partition path.

**Fix:**
1. Check the filter predicates
2. Verify the partition structure matches the scan path
3. Try eager `read_parquet` temporarily to confirm data exists

### Duplicate timestamps in normalized data

**Cause:** Raw data had duplicates that weren't deduplicated.

**Fix:** The data quality report should flag this. Dedupe by `(symbol, ts)` keeping the last occurrence (or first, depending on policy).

---

## Rubber Duck Checklist

When stuck, ask yourself:

1. **What exactly did I expect to happen?**
2. **What actually happened?**
3. **What's the smallest reproduction case?**
4. **Did I read the error message completely?**
5. **Is this the same code I think it is?** (check branch, check saved files)
6. **What changed since it last worked?**

---

## Getting Help

1. Check if there's a BDD scenario covering this case
2. Add a failing test that demonstrates the bug
3. Open an issue with:
   - Steps to reproduce
   - Expected vs actual behavior
   - Relevant error output
   - TrendLab version / commit hash
