//! Performance metrics calculations.

use serde::{Deserialize, Serialize};

/// Performance metrics for a backtest run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metrics {
    /// Compound annual growth rate
    pub cagr: f64,

    /// Annualized Sharpe ratio (assuming 252 trading days)
    pub sharpe: f64,

    /// Annualized Sortino ratio
    pub sortino: f64,

    /// Maximum drawdown (as a positive percentage, e.g., 0.20 = 20%)
    pub max_drawdown: f64,

    /// Calmar ratio (CAGR / Max Drawdown)
    pub calmar: f64,

    /// Win rate (winning trades / total trades)
    pub win_rate: f64,

    /// Profit factor (gross profit / gross loss)
    pub profit_factor: f64,

    /// Total number of trades
    pub num_trades: u32,

    /// Annual turnover (as multiple of capital)
    pub turnover: f64,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            cagr: 0.0,
            sharpe: 0.0,
            sortino: 0.0,
            max_drawdown: 0.0,
            calmar: 0.0,
            win_rate: 0.0,
            profit_factor: 0.0,
            num_trades: 0,
            turnover: 0.0,
        }
    }
}

/// Calculate CAGR from initial and final values over a number of years.
pub fn calculate_cagr(initial: f64, final_value: f64, years: f64) -> f64 {
    if initial <= 0.0 || years <= 0.0 {
        return 0.0;
    }
    (final_value / initial).powf(1.0 / years) - 1.0
}

/// Calculate annualized Sharpe ratio from daily returns.
///
/// Assumes 252 trading days per year and risk-free rate of 0.
pub fn calculate_sharpe(daily_returns: &[f64]) -> f64 {
    if daily_returns.is_empty() {
        return 0.0;
    }

    let n = daily_returns.len() as f64;
    let mean = daily_returns.iter().sum::<f64>() / n;
    let variance = daily_returns
        .iter()
        .map(|r| (r - mean).powi(2))
        .sum::<f64>()
        / n;
    let std_dev = variance.sqrt();

    if std_dev == 0.0 {
        return 0.0;
    }

    // Annualize: multiply mean by 252, std by sqrt(252)
    (mean * 252.0) / (std_dev * 252.0_f64.sqrt())
}

/// Calculate maximum drawdown from an equity curve.
pub fn calculate_max_drawdown(equity_curve: &[f64]) -> f64 {
    if equity_curve.is_empty() {
        return 0.0;
    }

    let mut peak = equity_curve[0];
    let mut max_dd = 0.0;

    for &equity in equity_curve {
        if equity > peak {
            peak = equity;
        }
        let dd = (peak - equity) / peak;
        if dd > max_dd {
            max_dd = dd;
        }
    }

    max_dd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cagr() {
        // $100 -> $200 in 5 years = ~14.87% CAGR
        let cagr = calculate_cagr(100.0, 200.0, 5.0);
        assert!((cagr - 0.1487).abs() < 0.001);
    }

    #[test]
    fn test_max_drawdown() {
        let equity = vec![100.0, 110.0, 105.0, 120.0, 90.0, 100.0];
        let dd = calculate_max_drawdown(&equity);
        // Peak was 120, trough was 90 -> 25% drawdown
        assert!((dd - 0.25).abs() < 0.001);
    }
}
