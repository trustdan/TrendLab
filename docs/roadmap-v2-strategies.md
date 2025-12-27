# V2 Strategy Implementation Roadmap

This roadmap tracks the implementation of Polars-native (V2) versions of all strategies.

## Status Overview

| Strategy | Status | Phase | PR/Commit |
|----------|--------|-------|-----------|
| Donchian | Done | - | Initial |
| MACrossover | Done | - | Initial |
| Tsmom | Done | - | Initial |
| FiftyTwoWeekHigh | Done | 1 | |
| BollingerSqueeze | Done | 1 | |
| Keltner | Done | 1 | |
| STARC | Done | 1 | |
| Supertrend | Done | 1 | |
| Aroon | Done | 2 | |
| DmiAdx | Done | 2 | |
| HeikinAshi | Done | 2 | |
| DarvasBox | Done | 3 | |
| LarryWilliams | Done | 3 | |
| OpeningRangeBreakout | Done | 4 | |
| ParabolicSar | Done | 4 | |
| Ensemble | Done | 5 | |

---

## Phase 1: Simple Strategies (~10 hours)

Strategies with straightforward indicator logic and simple band-crossing signals.

### FiftyTwoWeekHigh (~1.5h) ✅

- [x] Create `FiftyTwoWeekHighV2` struct in `strategy_v2.rs`
- [x] Use native `rolling_max()` for indicator
- [x] Signal: close within X% of period high (entry), close < Y% (exit)
- [x] Update `create_strategy_v2_from_config()`
- [ ] Verify in TUI

### BollingerSqueeze (~2h) ✅

- [x] Create `BollingerSqueezeV2` struct
- [x] Use existing `apply_bollinger_exprs()` from `indicators_polars.rs`
- [x] Signal: squeeze detection + breakout above upper band
- [x] Update `create_strategy_v2_from_config()`
- [ ] Verify in TUI

### Keltner (~2.5h) ✅

- [x] Create `keltner_channel_exprs()` in `indicators_polars.rs`
- [x] Create `KeltnerV2` struct
- [x] Signal: close > upper (entry), close < middle/lower (exit)
- [x] Update `create_strategy_v2_from_config()`
- [ ] Verify in TUI

### STARC (~2h) ✅

- [x] Create `starc_bands_exprs()` in `indicators_polars.rs`
- [x] Create `StarcV2` struct
- [x] Signal: close > upper (entry), close < lower (exit)
- [x] Update `create_strategy_v2_from_config()`
- [ ] Verify in TUI

### Supertrend (~2h) ✅

- [x] Create `supertrend_exprs()` in `indicators_polars.rs`
- [x] Create `SupertrendV2` struct
- [x] Signal: trend flip detection (is_uptrend state)
- [x] Update `create_strategy_v2_from_config()`
- [ ] Verify in TUI

---

## Phase 2: Moderate Strategies (~15 hours)

Strategies with crossover detection or multi-condition logic.

### Aroon (~3h) ✅

- [x] Create `AroonV2` struct
- [x] Use existing `apply_aroon_exprs()`
- [x] Signal: aroon_up crosses above aroon_down (use shift())
- [x] Update `create_strategy_v2_from_config()`
- [ ] Verify in TUI

### DmiAdx (~4h) ✅

- [x] Create `DmiAdxV2` struct
- [x] Use existing `apply_dmi_exprs()`
- [x] Signal: +DI > -DI AND ADX > threshold
- [x] Multiple exit conditions
- [x] Update `create_strategy_v2_from_config()`
- [ ] Verify in TUI

### HeikinAshi (~6-8h) ✅

- [x] Create `apply_heikin_ashi_exprs()` in `indicators_polars.rs`
- [x] Create `HeikinAshiV2` struct
- [x] Signal: consecutive bullish/bearish candle counting
- [x] Update `create_strategy_v2_from_config()`
- [ ] Verify in TUI

---

## Phase 3: Stateful Strategies (~18 hours)

Strategies requiring state tracking (box boundaries, entry prices).

### DarvasBox (~8-10h) ✅

- [x] Create `DarvasBoxV2` struct (uses rolling window approximation)
- [x] Signal: box formation + breakout detection
- [x] Update `create_strategy_v2_from_config()`
- [ ] Verify in TUI

### LarryWilliams (~8-10h) ✅

- [x] Create `LarryWilliamsV2` struct
- [x] Signal: open + k*prior_range breakout, ATR trailing stop
- [x] Update `create_strategy_v2_from_config()`
- [ ] Verify in TUI

---

## Phase 4: Complex Strategies (~20 hours)

Strategies requiring special Polars handling or architectural decisions.

### OpeningRangeBreakout (~8-10h) ✅

- [x] Create `apply_opening_range_exprs()` in `indicators_polars.rs`
- [x] Handle period-based windowing (weekly/monthly/rolling)
- [x] Create `OpeningRangeBreakoutV2` struct
- [x] Update `create_strategy_v2_from_config()`
- [ ] Verify in TUI

### ParabolicSar (~10-15h) ✅

- [x] Create `ParabolicSARV2` struct (uses ATR-based approximation)
- [x] Signal: trend flip detection
- [x] Update `create_strategy_v2_from_config()`
- [ ] Verify in TUI

---

## Phase 5: Meta-Strategy (~8 hours)

### Ensemble (~6-8h) ✅

- [x] Create `EnsembleV2` struct
- [x] Orchestrate child V2 strategies via `add_signals_to_lf()`
- [x] Aggregate signals via voting methods (Majority, WeightedByHorizon, UnanimousEntry)
- [x] Update `create_strategy_v2_from_config()`
- [ ] Verify in TUI

---

## Key Files

- `crates/trendlab-core/src/strategy_v2.rs` - V2 strategy implementations
- `crates/trendlab-core/src/indicators_polars.rs` - Polars indicator expressions
- `crates/trendlab-core/src/sweep.rs` - `create_strategy_v2_from_config()`
- `crates/trendlab-core/src/strategy.rs` - Legacy implementations (reference)
- `crates/trendlab-core/src/indicators.rs` - Sequential indicators (reference)

---

## Success Criteria

- All 15 strategies work in TUI without panicking
- Each V2 strategy produces identical signals to legacy implementation
- BDD scenarios pass for each strategy
- Sweep performance remains fast (Polars vectorization preserved)
