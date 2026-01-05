"""
Configuration and parameter grid generation for GPU Mega-Sweep.

Mirrors Rust StrategyGridConfig and StrategyParams from
crates/trendlab-core/src/sweep.rs.
"""

from dataclasses import dataclass, field
from datetime import date
from itertools import product
from pathlib import Path
from typing import Any

import yaml


@dataclass
class BacktestConfig:
    """Backtest configuration parameters."""

    initial_cash: float = 100_000.0
    fees_bps: float = 10.0  # Basis points per side
    qty_per_trade: float = 100.0
    slippage_bps: float = 0.0


@dataclass
class DataConfig:
    """Data source configuration."""

    base_dir: Path = field(default_factory=lambda: Path("data/parquet"))
    timeframe: str = "1d"
    start_date: date | None = None
    end_date: date | None = None


# ---------------------------------------------------------------------------
# Strategy Parameter Grids
# ---------------------------------------------------------------------------


@dataclass
class DonchianParams:
    """Donchian/Turtle breakout parameter grid."""

    entry_lookbacks: list[int] = field(
        default_factory=lambda: [10, 20, 30, 40, 55, 89]
    )
    exit_lookbacks: list[int] = field(default_factory=lambda: [5, 10, 15, 20, 25])

    def generate_configs(self) -> list[dict[str, Any]]:
        """Generate all valid config combinations."""
        configs = []
        for entry, exit_ in product(self.entry_lookbacks, self.exit_lookbacks):
            if exit_ < entry:  # Exit must be shorter than entry
                configs.append({"entry_lookback": entry, "exit_lookback": exit_})
        return configs


@dataclass
class MACrossoverParams:
    """MA Crossover parameter grid."""

    fast_periods: list[int] = field(default_factory=lambda: [5, 10, 20, 50])
    slow_periods: list[int] = field(default_factory=lambda: [20, 50, 100, 200])
    ma_types: list[str] = field(default_factory=lambda: ["sma", "ema"])

    def generate_configs(self) -> list[dict[str, Any]]:
        """Generate all valid config combinations."""
        configs = []
        for fast, slow, ma_type in product(
            self.fast_periods, self.slow_periods, self.ma_types
        ):
            if fast < slow:  # Fast must be shorter than slow
                configs.append({
                    "fast_period": fast,
                    "slow_period": slow,
                    "ma_type": ma_type,
                })
        return configs


@dataclass
class SupertrendParams:
    """Supertrend parameter grid."""

    atr_periods: list[int] = field(default_factory=lambda: [7, 10, 14, 20])
    multipliers: list[float] = field(default_factory=lambda: [2.0, 2.5, 3.0, 3.5])

    def generate_configs(self) -> list[dict[str, Any]]:
        """Generate all valid config combinations."""
        return [
            {"atr_period": atr, "multiplier": mult}
            for atr, mult in product(self.atr_periods, self.multipliers)
        ]


@dataclass
class FiftyTwoWeekHighParams:
    """52-Week High parameter grid."""

    periods: list[int] = field(default_factory=lambda: [50, 126, 252])
    entry_pcts: list[float] = field(default_factory=lambda: [0.90, 0.95, 0.98])
    exit_pcts: list[float] = field(default_factory=lambda: [0.70, 0.80, 0.85])

    def generate_configs(self) -> list[dict[str, Any]]:
        """Generate all valid config combinations."""
        configs = []
        for period, entry_pct, exit_pct in product(
            self.periods, self.entry_pcts, self.exit_pcts
        ):
            if exit_pct < entry_pct:  # Exit threshold must be lower
                configs.append({
                    "period": period,
                    "entry_pct": entry_pct,
                    "exit_pct": exit_pct,
                })
        return configs


@dataclass
class ParabolicSarParams:
    """Parabolic SAR parameter grid."""

    af_starts: list[float] = field(default_factory=lambda: [0.01, 0.02, 0.03])
    af_steps: list[float] = field(default_factory=lambda: [0.01, 0.02])
    af_maxes: list[float] = field(default_factory=lambda: [0.1, 0.2, 0.3])

    def generate_configs(self) -> list[dict[str, Any]]:
        """Generate all valid config combinations."""
        return [
            {"af_start": start, "af_step": step, "af_max": max_}
            for start, step, max_ in product(
                self.af_starts, self.af_steps, self.af_maxes
            )
        ]


@dataclass
class LarryWilliamsParams:
    """Larry Williams range expansion parameter grid."""

    range_mults: list[float] = field(default_factory=lambda: [1.0, 1.5, 2.0])
    atr_stop_mults: list[float] = field(default_factory=lambda: [2.0, 2.5, 3.0])
    atr_periods: list[int] = field(default_factory=lambda: [10, 14, 20])

    def generate_configs(self) -> list[dict[str, Any]]:
        """Generate all valid config combinations."""
        return [
            {"range_mult": rm, "atr_stop_mult": asm, "atr_period": ap}
            for rm, asm, ap in product(
                self.range_mults, self.atr_stop_mults, self.atr_periods
            )
        ]


@dataclass
class KeltnerParams:
    """Keltner Channel parameter grid."""

    ema_periods: list[int] = field(default_factory=lambda: [10, 20, 30])
    atr_periods: list[int] = field(default_factory=lambda: [10, 14, 20])
    multipliers: list[float] = field(default_factory=lambda: [1.5, 2.0, 2.5])

    def generate_configs(self) -> list[dict[str, Any]]:
        """Generate all valid config combinations."""
        return [
            {"ema_period": ema, "atr_period": atr, "multiplier": mult}
            for ema, atr, mult in product(
                self.ema_periods, self.atr_periods, self.multipliers
            )
        ]


@dataclass
class StarcParams:
    """STARC Bands parameter grid."""

    sma_periods: list[int] = field(default_factory=lambda: [5, 10, 15])
    atr_periods: list[int] = field(default_factory=lambda: [10, 14, 20])
    multipliers: list[float] = field(default_factory=lambda: [1.5, 2.0, 2.5])

    def generate_configs(self) -> list[dict[str, Any]]:
        """Generate all valid config combinations."""
        return [
            {"sma_period": sma, "atr_period": atr, "multiplier": mult}
            for sma, atr, mult in product(
                self.sma_periods, self.atr_periods, self.multipliers
            )
        ]


@dataclass
class BollingerParams:
    """Bollinger Squeeze parameter grid."""

    periods: list[int] = field(default_factory=lambda: [10, 20, 30])
    std_mults: list[float] = field(default_factory=lambda: [1.5, 2.0, 2.5])
    squeeze_thresholds: list[float] = field(default_factory=lambda: [0.5, 1.0, 1.5])

    def generate_configs(self) -> list[dict[str, Any]]:
        """Generate all valid config combinations."""
        return [
            {"period": p, "std_mult": sm, "squeeze_threshold": st}
            for p, sm, st in product(
                self.periods, self.std_mults, self.squeeze_thresholds
            )
        ]


@dataclass
class AroonParams:
    """Aroon oscillator parameter grid."""

    periods: list[int] = field(default_factory=lambda: [14, 20, 25, 30])

    def generate_configs(self) -> list[dict[str, Any]]:
        """Generate all valid config combinations."""
        return [{"period": p} for p in self.periods]


@dataclass
class DmiAdxParams:
    """DMI + ADX parameter grid."""

    di_periods: list[int] = field(default_factory=lambda: [10, 14, 20])
    adx_periods: list[int] = field(default_factory=lambda: [10, 14, 20])
    adx_thresholds: list[float] = field(default_factory=lambda: [20.0, 25.0, 30.0])

    def generate_configs(self) -> list[dict[str, Any]]:
        """Generate all valid config combinations."""
        return [
            {"di_period": di, "adx_period": adx, "adx_threshold": thresh}
            for di, adx, thresh in product(
                self.di_periods, self.adx_periods, self.adx_thresholds
            )
        ]


@dataclass
class TsmomParams:
    """Time-series momentum parameter grid."""

    lookbacks: list[int] = field(default_factory=lambda: [21, 63, 126, 252])

    def generate_configs(self) -> list[dict[str, Any]]:
        """Generate all valid config combinations."""
        return [{"lookback": lb} for lb in self.lookbacks]


@dataclass
class HeikinAshiParams:
    """Heikin-Ashi parameter grid."""

    confirmation_bars: list[int] = field(default_factory=lambda: [1, 2, 3])

    def generate_configs(self) -> list[dict[str, Any]]:
        """Generate all valid config combinations."""
        return [{"confirmation_bars": cb} for cb in self.confirmation_bars]


@dataclass
class RsiParams:
    """RSI parameter grid."""

    periods: list[int] = field(default_factory=lambda: [7, 14, 21])
    overboughts: list[float] = field(default_factory=lambda: [70.0, 75.0, 80.0])
    oversolds: list[float] = field(default_factory=lambda: [20.0, 25.0, 30.0])

    def generate_configs(self) -> list[dict[str, Any]]:
        """Generate all valid config combinations."""
        return [
            {"period": p, "overbought": ob, "oversold": os}
            for p, ob, os in product(self.periods, self.overboughts, self.oversolds)
        ]


@dataclass
class MacdParams:
    """MACD parameter grid."""

    fast_periods: list[int] = field(default_factory=lambda: [8, 12, 16])
    slow_periods: list[int] = field(default_factory=lambda: [21, 26, 30])
    signal_periods: list[int] = field(default_factory=lambda: [7, 9, 12])

    def generate_configs(self) -> list[dict[str, Any]]:
        """Generate all valid config combinations."""
        configs = []
        for fast, slow, signal in product(
            self.fast_periods, self.slow_periods, self.signal_periods
        ):
            if fast < slow:  # Fast must be shorter than slow
                configs.append({
                    "fast_period": fast,
                    "slow_period": slow,
                    "signal_period": signal,
                })
        return configs


@dataclass
class StochasticParams:
    """Stochastic oscillator parameter grid."""

    k_periods: list[int] = field(default_factory=lambda: [5, 9, 14])
    d_periods: list[int] = field(default_factory=lambda: [3, 5])
    overboughts: list[float] = field(default_factory=lambda: [80.0])
    oversolds: list[float] = field(default_factory=lambda: [20.0])

    def generate_configs(self) -> list[dict[str, Any]]:
        """Generate all valid config combinations."""
        return [
            {"k_period": k, "d_period": d, "overbought": ob, "oversold": os}
            for k, d, ob, os in product(
                self.k_periods, self.d_periods, self.overboughts, self.oversolds
            )
        ]


# ---------------------------------------------------------------------------
# Strategy Grid Config Registry
# ---------------------------------------------------------------------------

STRATEGY_PARAMS_REGISTRY: dict[str, type] = {
    "donchian": DonchianParams,
    "ma_crossover": MACrossoverParams,
    "supertrend": SupertrendParams,
    "fifty_two_week": FiftyTwoWeekHighParams,
    "parabolic_sar": ParabolicSarParams,
    "larry_williams": LarryWilliamsParams,
    "keltner": KeltnerParams,
    "starc": StarcParams,
    "bollinger": BollingerParams,
    "aroon": AroonParams,
    "dmi_adx": DmiAdxParams,
    "tsmom": TsmomParams,
    "heikin_ashi": HeikinAshiParams,
    "rsi": RsiParams,
    "macd": MacdParams,
    "stochastic": StochasticParams,
}


@dataclass
class StrategyGridConfig:
    """
    Configuration for a single strategy's parameter sweep.

    Mirrors Rust StrategyGridConfig.
    """

    strategy_type: str
    enabled: bool = True
    params: Any = None  # One of the *Params dataclasses

    def generate_configs(self) -> list[dict[str, Any]]:
        """Generate all parameter combinations for this strategy."""
        if not self.enabled or self.params is None:
            return []
        return self.params.generate_configs()


# ---------------------------------------------------------------------------
# Full Sweep Configuration
# ---------------------------------------------------------------------------


@dataclass
class SweepConfig:
    """
    Complete sweep configuration loaded from YAML.

    Contains data config, backtest config, symbols, and strategy grids.
    """

    data: DataConfig = field(default_factory=DataConfig)
    backtest: BacktestConfig = field(default_factory=BacktestConfig)
    symbols: list[str] = field(default_factory=list)
    strategies: dict[str, StrategyGridConfig] = field(default_factory=dict)
    output_dir: Path = field(default_factory=lambda: Path("results/gpu_sweeps"))
    output_format: str = "parquet"
    include_equity_curves: bool = False

    @classmethod
    def from_yaml(cls, path: Path | str) -> "SweepConfig":
        """Load configuration from YAML file."""
        path = Path(path)
        with open(path) as f:
            raw = yaml.safe_load(f)

        # Parse data config
        data_raw = raw.get("data", {})
        data = DataConfig(
            base_dir=Path(data_raw.get("base_dir", "data/parquet")),
            timeframe=data_raw.get("timeframe", "1d"),
            start_date=_parse_date(data_raw.get("start_date")),
            end_date=_parse_date(data_raw.get("end_date")),
        )

        # Parse backtest config
        bt_raw = raw.get("backtest", {})
        backtest = BacktestConfig(
            initial_cash=bt_raw.get("initial_cash", 100_000.0),
            fees_bps=bt_raw.get("fees_bps", 10.0),
            qty_per_trade=bt_raw.get("qty_per_trade", 100.0),
            slippage_bps=bt_raw.get("slippage_bps", 0.0),
        )

        # Parse symbols
        symbols_raw = raw.get("symbols", {})
        if isinstance(symbols_raw, list):
            symbols = symbols_raw
        elif isinstance(symbols_raw, dict):
            if "file" in symbols_raw:
                symbols_file = Path(symbols_raw["file"])
                if not symbols_file.is_absolute():
                    symbols_file = path.parent / symbols_file
                with open(symbols_file) as f:
                    symbols = [line.strip() for line in f if line.strip()]
            else:
                symbols = symbols_raw.get("list", [])
        else:
            symbols = []

        # Parse strategy grids
        strategies = {}
        for strategy_name, strategy_raw in raw.get("strategies", {}).items():
            if strategy_name not in STRATEGY_PARAMS_REGISTRY:
                continue

            enabled = strategy_raw.get("enabled", True)
            if not enabled:
                continue

            # Build params from raw config
            params_cls = STRATEGY_PARAMS_REGISTRY[strategy_name]
            params_dict = {
                k: v for k, v in strategy_raw.items() if k != "enabled"
            }
            params = params_cls(**params_dict) if params_dict else params_cls()

            strategies[strategy_name] = StrategyGridConfig(
                strategy_type=strategy_name,
                enabled=enabled,
                params=params,
            )

        # Parse output config
        output_raw = raw.get("output", {})
        output_dir = Path(output_raw.get("dir", "results/gpu_sweeps"))
        output_format = output_raw.get("format", "parquet")
        include_equity_curves = output_raw.get("include_equity_curves", False)

        return cls(
            data=data,
            backtest=backtest,
            symbols=symbols,
            strategies=strategies,
            output_dir=output_dir,
            output_format=output_format,
            include_equity_curves=include_equity_curves,
        )

    def total_configs(self) -> int:
        """Calculate total number of configs across all strategies."""
        return sum(
            len(grid.generate_configs()) for grid in self.strategies.values()
        )

    def summary(self) -> str:
        """Generate a summary of the sweep configuration."""
        lines = [
            f"GPU Mega-Sweep Configuration",
            f"=============================",
            f"Data: {self.data.base_dir} ({self.data.timeframe})",
            f"Date range: {self.data.start_date} to {self.data.end_date}",
            f"Symbols: {len(self.symbols)}",
            f"Initial cash: ${self.backtest.initial_cash:,.0f}",
            f"Fees: {self.backtest.fees_bps} bps per side",
            f"",
            f"Strategies:",
        ]

        for name, grid in self.strategies.items():
            configs = grid.generate_configs()
            lines.append(f"  {name}: {len(configs)} configs")

        total = self.total_configs()
        lines.append(f"")
        lines.append(f"Total configs: {total:,}")
        lines.append(f"Total backtests: {total * len(self.symbols):,}")

        return "\n".join(lines)


def _parse_date(value: str | None) -> date | None:
    """Parse a date string to date object."""
    if value is None:
        return None
    if isinstance(value, date):
        return value
    return date.fromisoformat(str(value))
