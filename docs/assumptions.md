# TrendLab Assumptions

This document defines the core assumptions that underpin TrendLab's backtest model. **Changing any of these requires updating BDD tests and reviewing all dependent code.**

---

## Fill Convention

**Default: Next Bar Open**

- Signal is computed on bar close (time T)
- Order is filled at next bar's open price (time T+1)
- This prevents lookahead bias: we can't trade on information we don't have yet

**Configurable options (future):**
- `next_open` (default)
- `next_close`
- `same_close` (for research comparison only — introduces lookahead)

---

## Price Adjustment Policy

**Default: Split-Adjusted, Dividend-Adjusted Closes**

- Use adjusted close for all backtesting calculations
- Raw/unadjusted prices cached separately for reference
- Yahoo Finance `Adj Close` is the source of truth for Phase 1

**Rationale:** Adjusted prices give accurate total returns including dividends and prevent spurious signals from splits.

---

## Timezone Policy

**All timestamps are UTC**

- Data stored with UTC timestamps (milliseconds since epoch)
- No local timezone conversions in core logic
- Display formatting can convert to local time at presentation layer

---

## Missing Bars Policy

**Market holidays and weekends are NOT filled**

- If a bar is missing (market closed), it simply doesn't exist in the data
- Indicators that require N bars look back N *trading* days, not calendar days
- Data quality checks flag unexpected gaps (should be exchange holidays only)

---

## Position Sizing (Phase 1)

**Fixed notional sizing**

- Each trade uses a fixed dollar amount (e.g., $10,000)
- No volatility targeting or Kelly sizing in Phase 1
- Position size is independent of account equity (no compounding effect in sizing)

---

## Fees and Slippage

**Fees: Basis points per trade**

- Default: 10 bps (0.10%) round-trip
- Applied at trade execution (buy and sell)
- Configurable per-run

**Slippage: Optional basis points**

- Default: 0 bps (no slippage)
- When enabled, adds to execution cost symmetrically
- Does not model market impact or adverse selection

---

## Long-Only Constraint (Phase 1)

- Phase 1 supports long-only strategies
- Short selling, margin, and leverage are out of scope
- Position states: `flat` or `long` (no `short`)

---

## Indicator Calculation

**All indicators use adjusted close prices**

- Moving averages, ATR, etc. computed on adjusted close
- Volume indicators use raw volume (not adjusted)
- Indicators are computed bar-by-bar with no lookahead

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2024-12-26 | Initial assumptions document |
