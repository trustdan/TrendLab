# Plan: Strategy Variants & Cleanup

## Summary
Add variants of top 3 strategies with different entry/exit criteria (not lookback periods), then disable 6 worst performers.

---

## Part 1: Disable Worst Performers

Set `enabled: false` for these 6 in `MultiStrategyGrid` defaults:
- DMI/ADX (0.04 Sharpe)
- Keltner (0.08)
- TurtleS2 (0.10)
- DarvasBox (0.11)
- BollingerSqueeze (0.12)
- TurtleS1 (0.11)

**File**: `crates/trendlab-core/src/sweep.rs`

---

## Part 2: Supertrend Variants (4 new strategy types)

### 2.1 `SupertrendConfirmed`
Require N consecutive trend bars before entry.
- New param: `confirmation_bars: [2, 3, 5]`
- Filters whipsaws from brief trend flips

### 2.2 `SupertrendVolume`
Only enter when volume > X% of average.
- New params: `volume_lookback: [20]`, `volume_threshold_pct: [1.2, 1.5]`
- Confirms breakout strength

### 2.3 `SupertrendAsymmetric`
Different ATR multiplier for exits vs entries.
- New params: `entry_multiplier`, `exit_multiplier` (exit > entry)
- Gives trades more room to breathe

### 2.4 `SupertrendCooldown`
Minimum bars between trades.
- New param: `cooldown_bars: [5, 10, 20]`
- Prevents churn after exit

---

## Part 3: FiftyTwoWeekHigh Variants (2 new, strategy-specific)

### 3.1 `FiftyTwoWeekHighMomentum`
Only enter if price is accelerating toward high (positive ROC).
- New params: `momentum_period: [10, 20]`, `momentum_threshold: [0.0, 0.02]`
- Filters choppy consolidation near highs

### 3.2 `FiftyTwoWeekHighTrailing`
Replace fixed exit_pct with trailing stop from entry.
- New param: `trailing_stop_pct: [0.05, 0.10, 0.15]`
- Locks in gains on strong moves

---

## Part 4: ParabolicSAR Variants (2 new, strategy-specific)

### 4.1 `ParabolicSarFiltered`
Only act on SAR flips when price is above long-term MA.
- New param: `trend_ma_period: [50, 200]`
- Adds trend context to filter ranging-market whipsaws

### 4.2 `ParabolicSarDelayed`
Wait N bars after SAR flip before entering.
- New param: `delay_bars: [1, 2, 3]`
- Confirms flip isn't a false signal

---

## Architecture Decision

**Variants as NEW StrategyTypeId entries** (not extensions):
- Each gets own leaderboard row
- Independent enable/disable
- Cleaner parameter grids (no combinatorial explosion)
- Follows TurtleS1/S2 pattern

---

## Files to Modify

| File | Changes |
|------|---------|
| `crates/trendlab-core/src/sweep.rs` | Add 8 StrategyTypeId entries, StrategyParams variants, StrategyConfigId variants; disable 6 strategies |
| `crates/trendlab-core/src/strategy_v2.rs` | Add 8 StrategySpec variants + V2 implementations |
| `crates/trendlab-core/src/indicators_polars.rs` | Add volume SMA expr, consecutive count helper |
| `crates/trendlab-core/src/lib.rs` | Export new types |
| `crates/trendlab-bdd/tests/features/` | BDD scenarios for each variant |

---

## Implementation Order

1. **Disable 6 underperformers** (quick win)
2. **SupertrendVolume** (simplest - pure Polars)
3. **SupertrendConfirmed** (rolling window logic)
4. **ParabolicSarFiltered** (MA filter - straightforward)
5. **ParabolicSarDelayed** (bars since flip tracking)
6. **FiftyTwoWeekHighMomentum** (ROC filter)
7. **SupertrendAsymmetric** (dual indicators)
8. **SupertrendCooldown** (stateful - most complex)
9. **FiftyTwoWeekHighTrailing** (stateful trailing stop)

---

## Default Grid Sizes (Standard depth)

| Variant | Configs |
|---------|---------|
| SupertrendConfirmed | 8 |
| SupertrendVolume | 6 |
| SupertrendAsymmetric | 6 |
| SupertrendCooldown | 9 |
| FiftyTwoWeekHighMomentum | 12 |
| FiftyTwoWeekHighTrailing | 8 |
| ParabolicSarFiltered | 6 |
| ParabolicSarDelayed | 6 |
| **Total new** | **61** |

---

## Testing Strategy

Each variant gets BDD feature file with scenarios for:
- Basic signal generation with new params
- Edge cases (filter blocks entry, filter allows entry)
- Warmup period handling
- Determinism verification
