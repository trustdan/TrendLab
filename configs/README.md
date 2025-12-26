# TrendLab Configs

This folder contains default configuration files for sweeps, strategies, and reports.

## Structure

```
configs/
├── sweeps/              # Parameter sweep grids
│   └── ma_cross.json    # Example: MA crossover sweep grid
├── strategies/          # Strategy default parameters
└── reports/             # Report templates
```

## Sweep Grid Format

```json
{
  "strategy_id": "ma_cross",
  "parameters": {
    "fast_period": [5, 10, 20],
    "slow_period": [50, 100, 200]
  },
  "constraints": [
    "fast_period < slow_period"
  ],
  "costs": {
    "fees_bps": 10,
    "slippage_bps": 0
  }
}
```

## Adding New Configs

1. Create a JSON file in the appropriate subfolder
2. Validate against the relevant schema in `schemas/`
3. Document any new config types in this README
