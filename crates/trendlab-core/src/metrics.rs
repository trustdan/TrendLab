//! Performance metrics calculations.

use crate::backtest::{BacktestResult, Trade};
use serde::{Deserialize, Deserializer, Serialize};

/// Deserialize a field that may be null as the default value.
fn deserialize_null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    let opt = Option::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

/// Performance metrics for a backtest run.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Metrics {
    /// Total return (as decimal, e.g., 0.25 = 25%)
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub total_return: f64,

    /// Compound annual growth rate
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub cagr: f64,

    /// Annualized Sharpe ratio (assuming 252 trading days)
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub sharpe: f64,

    /// Annualized Sortino ratio
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub sortino: f64,

    /// Maximum drawdown (as a positive percentage, e.g., 0.20 = 20%)
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub max_drawdown: f64,

    /// Calmar ratio (CAGR / Max Drawdown)
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub calmar: f64,

    /// Win rate (winning trades / total trades)
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub win_rate: f64,

    /// Profit factor (gross profit / gross loss)
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub profit_factor: f64,

    /// Total number of trades
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub num_trades: u32,

    /// Annual turnover (as multiple of capital)
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub turnover: f64,

    /// Maximum consecutive losing trades
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub max_consecutive_losses: u32,

    /// Maximum consecutive winning trades
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub max_consecutive_wins: u32,

    /// Average length of losing streaks
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub avg_losing_streak: f64,
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
    // Guard against division by zero if equity ever hits zero
    let daily_returns: Vec<f64> = equity_curve
        .windows(2)
        .map(|w| {
            if w[0].abs() > 1e-10 {
                (w[1] - w[0]) / w[0]
            } else {
                0.0
            }
        })
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
    // Profit factor: cap at 999.99 when gross_loss is 0 to avoid JSON null (INFINITY → null)
    let profit_factor = if gross_loss > 0.0 {
        (gross_profit / gross_loss).min(999.99)
    } else if gross_profit > 0.0 {
        999.99 // Capped: no losses means "infinite" profit factor
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

    // Consecutive streak metrics
    let (max_consecutive_losses, max_consecutive_wins, avg_losing_streak) =
        calculate_streaks(&result.trades);

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
        max_consecutive_losses,
        max_consecutive_wins,
        avg_losing_streak,
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
        // Guard against division by zero if peak is somehow zero
        let dd = if peak.abs() > 1e-10 {
            (peak - equity) / peak
        } else {
            0.0
        };
        if dd > max_dd {
            max_dd = dd;
        }
    }

    max_dd
}

/// Calculate consecutive win/loss streaks from trades.
///
/// Returns: (max_consecutive_losses, max_consecutive_wins, avg_losing_streak)
pub fn calculate_streaks(trades: &[Trade]) -> (u32, u32, f64) {
    if trades.is_empty() {
        return (0, 0, 0.0);
    }

    let mut max_losses = 0u32;
    let mut max_wins = 0u32;
    let mut current_losses = 0u32;
    let mut current_wins = 0u32;

    // Track all losing streaks for average calculation
    let mut losing_streaks: Vec<u32> = Vec::new();

    for trade in trades {
        if trade.net_pnl > 0.0 {
            // Winner
            current_wins += 1;
            if current_losses > 0 {
                losing_streaks.push(current_losses);
                max_losses = max_losses.max(current_losses);
                current_losses = 0;
            }
        } else {
            // Loser (including breakeven as loss for conservative counting)
            current_losses += 1;
            if current_wins > 0 {
                max_wins = max_wins.max(current_wins);
                current_wins = 0;
            }
        }
    }

    // Handle final streak
    if current_losses > 0 {
        losing_streaks.push(current_losses);
        max_losses = max_losses.max(current_losses);
    }
    if current_wins > 0 {
        max_wins = max_wins.max(current_wins);
    }

    // Calculate average losing streak
    let avg_losing_streak = if losing_streaks.is_empty() {
        0.0
    } else {
        losing_streaks.iter().sum::<u32>() as f64 / losing_streaks.len() as f64
    };

    (max_losses, max_wins, avg_losing_streak)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backtest::{Fill, Side, TradeDirection};
    use chrono::Utc;

    /// Helper to create a trade with just net_pnl (for streak testing)
    fn make_trade(net_pnl: f64) -> Trade {
        let fill = Fill {
            ts: Utc::now(),
            side: Side::Buy,
            qty: 1.0,
            price: 100.0,
            fees: 0.0,
            raw_price: 100.0,
            atr_at_fill: None,
        };
        Trade {
            entry: fill.clone(),
            exit: fill,
            gross_pnl: net_pnl,
            net_pnl,
            direction: TradeDirection::Long,
        }
    }

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

    #[test]
    fn test_streaks_empty() {
        let trades: Vec<Trade> = vec![];
        let (max_losses, max_wins, avg_losing) = calculate_streaks(&trades);
        assert_eq!(max_losses, 0);
        assert_eq!(max_wins, 0);
        assert!((avg_losing - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_streaks_all_winners() {
        let trades = vec![make_trade(100.0), make_trade(50.0), make_trade(200.0)];
        let (max_losses, max_wins, avg_losing) = calculate_streaks(&trades);
        assert_eq!(max_losses, 0);
        assert_eq!(max_wins, 3);
        assert!((avg_losing - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_streaks_all_losers() {
        let trades = vec![make_trade(-100.0), make_trade(-50.0), make_trade(-200.0)];
        let (max_losses, max_wins, avg_losing) = calculate_streaks(&trades);
        assert_eq!(max_losses, 3);
        assert_eq!(max_wins, 0);
        assert!((avg_losing - 3.0).abs() < 0.001); // One streak of 3
    }

    #[test]
    fn test_streaks_alternating() {
        // W, L, W, L, W, L
        let trades = vec![
            make_trade(100.0),
            make_trade(-50.0),
            make_trade(100.0),
            make_trade(-50.0),
            make_trade(100.0),
            make_trade(-50.0),
        ];
        let (max_losses, max_wins, avg_losing) = calculate_streaks(&trades);
        assert_eq!(max_losses, 1);
        assert_eq!(max_wins, 1);
        assert!((avg_losing - 1.0).abs() < 0.001); // Three streaks of 1
    }

    #[test]
    fn test_streaks_mixed() {
        // W, W, L, L, L, W, L, L
        let trades = vec![
            make_trade(100.0),  // W
            make_trade(50.0),   // W (2 win streak)
            make_trade(-100.0), // L
            make_trade(-50.0),  // L
            make_trade(-200.0), // L (3 loss streak)
            make_trade(150.0),  // W
            make_trade(-75.0),  // L
            make_trade(-25.0),  // L (2 loss streak)
        ];
        let (max_losses, max_wins, avg_losing) = calculate_streaks(&trades);
        assert_eq!(max_losses, 3);
        assert_eq!(max_wins, 2);
        // Two losing streaks: 3 and 2, average = 2.5
        assert!((avg_losing - 2.5).abs() < 0.001);
    }

    #[test]
    fn test_streaks_breakeven_as_loss() {
        // Breakeven (0.0) counts as loss for conservative counting
        let trades = vec![
            make_trade(100.0), // W
            make_trade(0.0),   // L (breakeven)
            make_trade(-50.0), // L
        ];
        let (max_losses, max_wins, _avg_losing) = calculate_streaks(&trades);
        assert_eq!(max_losses, 2); // Breakeven + loss = 2 streak
        assert_eq!(max_wins, 1);
    }
}
