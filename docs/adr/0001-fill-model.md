# ADR-0001: Fill Model

## Status

Accepted

## Context

When a strategy generates a signal on bar `i`, we need to decide at what price and time the resulting trade is filled. This is critical for realistic backtesting and avoiding lookahead bias.

Common fill models:
1. **Same-bar close**: Fill at Close[i] - unrealistic, implies trading at signal time
2. **Next-bar open**: Fill at Open[i+1] - most common, conservative
3. **Next-bar VWAP**: Fill at VWAP[i+1] - more realistic for larger orders
4. **Next-bar close**: Fill at Close[i+1] - too slow for most strategies

## Decision

Use **next-bar open** as the default fill model.

```
Signal generated: Bar[i].close
Fill executed: Bar[i+1].open + slippage
```

This means:
- A signal computed at the close of day T triggers a trade at the open of day T+1
- This is conservative and realistic for end-of-day strategies
- Slippage (if configured) is added as bps on top of open price

## Consequences

### Positive
- No lookahead bias - signal uses only past data
- Realistic for retail traders who can't trade at close
- Conservative - actual fills might be better
- Simple to understand and verify

### Negative
- May underestimate performance for liquid markets where close fills are achievable
- One-bar delay affects fast strategies (not our target use case)
- Gap risk not explicitly modeled (open can gap significantly from prior close)

### Mitigation
- Allow configurable fill model for advanced users
- Add gap analysis in metrics
- Document assumptions clearly

## Alternatives Considered

**Same-bar close**: Rejected because it requires trading at the exact moment the signal is generated, which is unrealistic.

**VWAP fill**: Deferred - requires intraday data we don't have in Phase 1.

## Related

- [assumptions.md](../assumptions.md) - Fill convention section
- [schema.md](../schema.md) - Fill event schema
