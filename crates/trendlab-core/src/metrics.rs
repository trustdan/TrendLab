//! Performance metrics calculations.

use crate::backtest::BacktestResult;
use serde::{Deserialize, Serialize};

/// Performance metrics for a backtest run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metrics {
    /// Total return (as decimal, e.g., 0.25 = 25%)
    pub total_return: f64,

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
            total_return: 0.0,
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

/// Compute all metrics from a BacktestResult.
pub fn compute_metrics(result: &BacktestResult, initial_cash: f64) -> Metrics {
    if result.equity.is_empty() {
        return Metrics::default();
    }

    let equity_curve: Vec<f64> = result.equity.iter().map(|e| e.equity).collect();
    let last_equity = equity_curve.last().copied().unwrap_or(initial_cash);

    // Total return
    let total_return = if initial_cash > 0.0 {
        (last_equity - initial_cash) / initial_cash
    } else {
        0.0
    };

    // Calculate years from first to last bar
    let years = if result.equity.len() >= 2 {
        let first_ts = result.equity.first().unwrap().ts;
        let last_ts = result.equity.last().unwrap().ts;
        let duration = last_ts.signed_duration_since(first_ts);
        duration.num_days() as f64 / 365.25
    } else {
        0.0
    };

    // CAGR
    let cagr = calculate_cagr(initial_cash, last_equity, years);

    // Max drawdown
    let max_drawdown = calculate_max_drawdown(&equity_curve);

    // Calmar ratio (CAGR / Max Drawdown)
    let calmar = if max_drawdown > 0.0 {
        cagr / max_drawdown
    } else {
        0.0
    };

    // Daily returns for Sharpe calculation
    let daily_returns: Vec<f64> = equity_curve
        .windows(2)
        .map(|w| (w[1] - w[0]) / w[0])
        .collect();

    let sharpe = calculate_sharpe(&daily_returns);
    let sortino = calculate_sortino(&daily_returns);

    // Trade-based metrics
    let num_trades = result.trades.len() as u32;
    let winning_trades = result.trades.iter().filter(|t| t.net_pnl > 0.0).count();
    let win_rate = if num_trades > 0 {
        winning_trades as f64 / num_trades as f64
    } else {
        0.0
    };

    // Profit factor
    let gross_profit: f64 = result
        .trades
        .iter()
        .filter(|t| t.net_pnl > 0.0)
        .map(|t| t.net_pnl)
        .sum();
    let gross_loss: f64 = result
        .trades
        .iter()
        .filter(|t| t.net_pnl < 0.0)
        .map(|t| t.net_pnl.abs())
        .sum();
    let profit_factor = if gross_loss > 0.0 {
        gross_profit / gross_loss
    } else if gross_profit > 0.0 {
        f64::INFINITY
    } else {
        0.0
    };

    // Turnover (total traded notional / average capital)
    let total_traded: f64 = result.fills.iter().map(|f| (f.qty * f.price).abs()).sum();
    let avg_capital = (initial_cash + last_equity) / 2.0;
    let turnover = if years > 0.0 && avg_capital > 0.0 {
        (total_traded / avg_capital) / years
    } else {
        0.0
    };

    Metrics {
        total_return,
        cagr,
        sharpe,
        sortino,
        max_drawdown,
        calmar,
        win_rate,
        profit_factor,
        num_trades,
        turnover,
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

/// Calculate annualized Sortino ratio from daily returns.
///
/// Like Sharpe but only penalizes downside volatility.
pub fn calculate_sortino(daily_returns: &[f64]) -> f64 {
    if daily_returns.is_empty() {
        return 0.0;
    }

    let n = daily_returns.len() as f64;
    let mean = daily_returns.iter().sum::<f64>() / n;

    // Downside deviation: only consider returns below zero
    let downside_variance = daily_returns
        .iter()
        .map(|r| if *r < 0.0 { r.powi(2) } else { 0.0 })
        .sum::<f64>()
        / n;
    let downside_dev = downside_variance.sqrt();

    if downside_dev == 0.0 {
        return 0.0;
    }

    // Annualize
    (mean * 252.0) / (downside_dev * 252.0_f64.sqrt())
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
