//! Terminal formatting and inline charts.

use colored::Colorize;

use trendlab_core::{Metrics, SweepConfigResult};

/// Color a value based on whether it's positive or negative.
pub fn color_value(value: f64, format_str: String, invert: bool) -> String {
    let is_positive = if invert { value < 0.0 } else { value > 0.0 };
    if is_positive {
        format_str.green().to_string()
    } else if value == 0.0 {
        format_str.yellow().to_string()
    } else {
        format_str.red().to_string()
    }
}

/// Format metrics with colors for terminal display.
#[allow(dead_code)]
pub fn format_metrics_colored(metrics: &Metrics) -> String {
    let mut output = String::new();

    output.push_str(&format!(
        "{:<20} {}\n",
        "Sharpe Ratio:".cyan(),
        color_value(metrics.sharpe, format!("{:.3}", metrics.sharpe), false)
    ));

    output.push_str(&format!(
        "{:<20} {}\n",
        "CAGR:".cyan(),
        color_value(metrics.cagr, format!("{:.2}%", metrics.cagr * 100.0), false)
    ));

    output.push_str(&format!(
        "{:<20} {}\n",
        "Sortino:".cyan(),
        color_value(metrics.sortino, format!("{:.3}", metrics.sortino), false)
    ));

    output.push_str(&format!(
        "{:<20} {}\n",
        "Calmar:".cyan(),
        color_value(metrics.calmar, format!("{:.3}", metrics.calmar), false)
    ));

    output.push_str(&format!(
        "{:<20} {}\n",
        "Max Drawdown:".cyan(),
        color_value(
            metrics.max_drawdown,
            format!("{:.2}%", metrics.max_drawdown * 100.0),
            true // Invert: lower is better
        )
    ));

    output.push_str(&format!(
        "{:<20} {}\n",
        "Total Return:".cyan(),
        color_value(
            metrics.total_return,
            format!("{:.2}%", metrics.total_return * 100.0),
            false
        )
    ));

    output.push_str(&format!(
        "{:<20} {:.2}%\n",
        "Win Rate:".cyan(),
        metrics.win_rate * 100.0
    ));

    output.push_str(&format!(
        "{:<20} {:.2}\n",
        "Profit Factor:".cyan(),
        metrics.profit_factor
    ));

    output.push_str(&format!(
        "{:<20} {}\n",
        "Number of Trades:".cyan(),
        metrics.num_trades
    ));

    output.push_str(&format!(
        "{:<20} {:.2}\n",
        "Turnover:".cyan(),
        metrics.turnover
    ));

    output
}

/// Render a sparkline for an equity curve.
pub fn sparkline(values: &[f64]) -> String {
    if values.is_empty() {
        return String::new();
    }

    let blocks = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;

    if range == 0.0 {
        return blocks[3].to_string().repeat(values.len().min(40));
    }

    // Sample if too many values
    let step = if values.len() > 40 {
        values.len() / 40
    } else {
        1
    };

    values
        .iter()
        .step_by(step)
        .take(40)
        .map(|v| {
            let normalized = ((v - min) / range * 7.0) as usize;
            blocks[normalized.min(7)]
        })
        .collect()
}

/// Render a larger ASCII chart for equity curve.
#[allow(dead_code)]
pub fn render_equity_chart(equity: &[f64], width: usize, height: usize) -> String {
    if equity.is_empty() {
        return "No equity data available".to_string();
    }

    let min = equity.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = equity.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;

    if range == 0.0 {
        return format!("Flat equity at ${:.0}", min);
    }

    // Sample data to fit width
    let step = if equity.len() > width {
        equity.len() / width
    } else {
        1
    };

    let sampled: Vec<f64> = equity.iter().step_by(step).take(width).cloned().collect();

    let mut lines: Vec<String> = Vec::new();

    // Create chart lines
    for row in 0..height {
        let threshold = max - (range * (row as f64) / (height as f64 - 1.0));
        let mut line = String::new();

        for &value in &sampled {
            if value >= threshold {
                line.push('█');
            } else {
                line.push(' ');
            }
        }

        lines.push(line);
    }

    // Add y-axis labels
    let mut output = String::new();
    output.push_str(&format!("${:>8.0} │", max));
    if !lines.is_empty() {
        output.push_str(&lines[0]);
    }
    output.push('\n');

    for line in lines.iter().skip(1).take(height - 2) {
        output.push_str(&format!("{:>10}│{}\n", "", line));
    }

    output.push_str(&format!("${:>8.0} │", min));
    if lines.len() > 1 {
        output.push_str(lines.last().unwrap_or(&String::new()));
    }
    output.push('\n');

    output
}

/// Format a sweep results table with colors.
pub fn format_sweep_table_colored(results: &[SweepConfigResult], top_n: usize) -> String {
    let mut sorted: Vec<_> = results.iter().collect();
    sorted.sort_by(|a, b| {
        b.metrics
            .sharpe
            .partial_cmp(&a.metrics.sharpe)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut output = String::new();

    // Header
    output.push_str(&format!(
        "{:<4} {:>6} {:>6} {:>8} {:>8} {:>8} {:>6} {:>40}\n",
        "Rank".cyan(),
        "Entry".cyan(),
        "Exit".cyan(),
        "Sharpe".cyan(),
        "CAGR%".cyan(),
        "MaxDD%".cyan(),
        "Trades".cyan(),
        "Equity".cyan()
    ));
    output.push_str(&format!("{}\n", "-".repeat(90).dimmed()));

    for (i, r) in sorted.iter().take(top_n).enumerate() {
        // Extract equity values from backtest result
        let equity_values: Vec<f64> = r.backtest_result.equity.iter().map(|e| e.equity).collect();
        let spark = sparkline(&equity_values);

        let sharpe_str = color_value(r.metrics.sharpe, format!("{:.3}", r.metrics.sharpe), false);
        let cagr_str = color_value(
            r.metrics.cagr,
            format!("{:.1}%", r.metrics.cagr * 100.0),
            false,
        );
        let dd_str = color_value(
            r.metrics.max_drawdown,
            format!("{:.1}%", r.metrics.max_drawdown * 100.0),
            true,
        );

        output.push_str(&format!(
            "{:<4} {:>6} {:>6} {:>8} {:>8} {:>8} {:>6} {}\n",
            format!("#{}", i + 1).white(),
            r.config_id.entry_lookback,
            r.config_id.exit_lookback,
            sharpe_str,
            cagr_str,
            dd_str,
            r.metrics.num_trades,
            spark
        ));
    }

    output
}

/// Print a summary header box.
#[allow(dead_code)]
pub fn print_summary_header(title: &str, run_id: &str, symbol: &str, date_range: &str) {
    let width = 70;
    let border_char = "═";
    let corner_tl = "╔";
    let corner_tr = "╗";
    let corner_bl = "╚";
    let corner_br = "╝";
    let side = "║";

    println!(
        "{}{}{}",
        corner_tl,
        border_char.repeat(width - 2),
        corner_tr
    );
    println!(
        "{} {:^width$} {}",
        side,
        title.cyan().bold(),
        side,
        width = width - 4
    );
    println!(
        "{}{}{}",
        corner_bl,
        border_char.repeat(width - 2),
        corner_br
    );
    println!();
    println!("  {} {}", "Run ID:".dimmed(), run_id.white());
    println!("  {} {}", "Symbol:".dimmed(), symbol.white());
    println!("  {} {}", "Period:".dimmed(), date_range.white());
    println!();
}

/// Print a horizontal separator.
pub fn print_separator() {
    println!("{}", "-".repeat(70).dimmed());
}

/// Print a section header.
pub fn print_section(title: &str) {
    println!("\n{}", title.cyan().bold());
    print_separator();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sparkline_empty() {
        assert_eq!(sparkline(&[]), "");
    }

    #[test]
    fn test_sparkline_constant() {
        let result = sparkline(&[100.0, 100.0, 100.0, 100.0]);
        assert!(!result.is_empty());
        // All same value should produce same blocks
    }

    #[test]
    fn test_sparkline_increasing() {
        let result = sparkline(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        assert!(!result.is_empty());
        // First char should be lowest block, last should be highest
        assert!(result.starts_with('▁'));
        assert!(result.ends_with('█'));
    }

    #[test]
    fn test_color_value_positive() {
        let result = color_value(1.5, "1.5".to_string(), false);
        assert!(result.contains("1.5"));
    }

    #[test]
    fn test_color_value_negative() {
        let result = color_value(-1.5, "-1.5".to_string(), false);
        assert!(result.contains("-1.5"));
    }

    #[test]
    fn test_color_value_inverted() {
        // With invert=true, negative values should be green (good)
        let result = color_value(-0.1, "-10%".to_string(), true);
        assert!(result.contains("-10%"));
    }

    #[test]
    fn test_render_equity_chart() {
        let equity = vec![10000.0, 10500.0, 11000.0, 10800.0, 12000.0];
        let chart = render_equity_chart(&equity, 40, 5);
        assert!(!chart.is_empty());
        assert!(chart.contains("$"));
    }

    #[test]
    fn test_render_equity_chart_empty() {
        let result = render_equity_chart(&[], 40, 5);
        assert_eq!(result, "No equity data available");
    }
}
