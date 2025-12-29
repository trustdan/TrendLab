//! HTML report generation - self-contained interactive reports.

use anyhow::{bail, Context, Result};
use maud::{html, Markup, PreEscaped, DOCTYPE};
use std::fs;
use std::path::PathBuf;

use trendlab_core::{RunManifest, SweepConfigResult};

/// CSS styles for the report (inline for self-contained HTML).
const REPORT_STYLES: &str = r##"
:root {
    --bg-primary: #1a1b26;
    --bg-secondary: #24283b;
    --text-primary: #c0caf5;
    --text-secondary: #a9b1d6;
    --accent-cyan: #7dcfff;
    --accent-green: #9ece6a;
    --accent-red: #f7768e;
    --accent-yellow: #e0af68;
    --accent-purple: #bb9af7;
    --border-color: #414868;
}

* {
    box-sizing: border-box;
    margin: 0;
    padding: 0;
}

body {
    font-family: 'JetBrains Mono', 'Fira Code', 'Consolas', monospace;
    background: var(--bg-primary);
    color: var(--text-primary);
    line-height: 1.6;
    padding: 2rem;
}

.container {
    max-width: 1400px;
    margin: 0 auto;
}

header {
    text-align: center;
    margin-bottom: 3rem;
    padding-bottom: 2rem;
    border-bottom: 2px solid var(--border-color);
}

header h1 {
    font-size: 2rem;
    color: var(--accent-cyan);
    margin-bottom: 0.5rem;
}

header .meta {
    color: var(--text-secondary);
    font-size: 0.9rem;
}

section {
    background: var(--bg-secondary);
    border-radius: 8px;
    padding: 1.5rem;
    margin-bottom: 2rem;
    border: 1px solid var(--border-color);
}

section h2 {
    color: var(--accent-cyan);
    margin-bottom: 1rem;
    padding-bottom: 0.5rem;
    border-bottom: 1px solid var(--border-color);
    font-size: 1.3rem;
}

.summary-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 1rem;
}

.metric-card {
    background: var(--bg-primary);
    border-radius: 6px;
    padding: 1rem;
    text-align: center;
}

.metric-card .label {
    color: var(--text-secondary);
    font-size: 0.85rem;
    margin-bottom: 0.3rem;
}

.metric-card .value {
    font-size: 1.5rem;
    font-weight: bold;
}

.metric-card .value.positive { color: var(--accent-green); }
.metric-card .value.negative { color: var(--accent-red); }
.metric-card .value.neutral { color: var(--accent-yellow); }

table {
    width: 100%;
    border-collapse: collapse;
}

th, td {
    padding: 0.75rem 1rem;
    text-align: left;
    border-bottom: 1px solid var(--border-color);
}

th {
    background: var(--bg-primary);
    color: var(--accent-cyan);
    font-weight: 600;
    text-transform: uppercase;
    font-size: 0.8rem;
    letter-spacing: 0.05em;
}

tr:hover {
    background: rgba(125, 207, 255, 0.05);
}

td.number {
    text-align: right;
    font-family: monospace;
}

td.positive { color: var(--accent-green); }
td.negative { color: var(--accent-red); }

.chart-container {
    height: 300px;
    position: relative;
    background: var(--bg-primary);
    border-radius: 6px;
    padding: 1rem;
    margin-top: 1rem;
}

.chart-placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--text-secondary);
}

footer {
    text-align: center;
    color: var(--text-secondary);
    font-size: 0.85rem;
    margin-top: 3rem;
    padding-top: 2rem;
    border-top: 1px solid var(--border-color);
}

.heatmap-container {
    overflow-x: auto;
}

.heatmap-container table th,
.heatmap-container table td {
    padding: 0.5rem;
    text-align: center;
    min-width: 50px;
}
"##;

/// JavaScript for interactive charts (inline for self-contained HTML).
const REPORT_SCRIPTS: &str = r##"
document.addEventListener('DOMContentLoaded', function() {
    const equityData = window.equityData || [];
    if (equityData.length > 0) {
        renderEquityChart(equityData);
    }

    const heatmapData = window.heatmapData || [];
    if (heatmapData.length > 0) {
        renderHeatmap(heatmapData);
    }
});

function renderEquityChart(data) {
    const container = document.getElementById('equity-chart');
    if (!container || data.length === 0) return;

    const width = container.clientWidth - 60;
    const height = 250;
    const padding = 40;

    const values = data.map(d => d.equity);
    const minVal = Math.min(...values);
    const maxVal = Math.max(...values);
    const range = maxVal - minVal || 1;

    const points = data.map((d, i) => {
        const x = padding + (i / (data.length - 1)) * (width - padding * 2);
        const y = height - padding - ((d.equity - minVal) / range) * (height - padding * 2);
        return x + ',' + y;
    }).join(' ');

    const gradientPoints = padding + ',' + (height - padding) + ' ' + points + ' ' + (width - padding) + ',' + (height - padding);

    const svg = '<svg width="' + width + '" height="' + height + '">' +
        '<defs>' +
            '<linearGradient id="equityGradient" x1="0" y1="0" x2="0" y2="1">' +
                '<stop offset="0%" stop-color="rgba(125, 207, 255, 0.3)"/>' +
                '<stop offset="100%" stop-color="rgba(125, 207, 255, 0)"/>' +
            '</linearGradient>' +
        '</defs>' +
        '<polygon points="' + gradientPoints + '" fill="url(#equityGradient)"/>' +
        '<polyline points="' + points + '" fill="none" stroke="#7dcfff" stroke-width="2"/>' +
        '<text x="' + padding + '" y="' + (height - 10) + '" fill="#a9b1d6" font-size="11">' + (data[0] && data[0].date || 'Start') + '</text>' +
        '<text x="' + (width - padding) + '" y="' + (height - 10) + '" fill="#a9b1d6" font-size="11" text-anchor="end">' + (data[data.length-1] && data[data.length-1].date || 'End') + '</text>' +
        '<text x="5" y="' + (padding - 5) + '" fill="#a9b1d6" font-size="11">$' + maxVal.toFixed(0) + '</text>' +
        '<text x="5" y="' + (height - padding) + '" fill="#a9b1d6" font-size="11">$' + minVal.toFixed(0) + '</text>' +
    '</svg>';

    container.innerHTML = svg;
}

function renderHeatmap(data) {
    const container = document.getElementById('sharpe-heatmap');
    if (!container) return;

    const entries = [...new Set(data.map(d => d.entry))].sort((a, b) => a - b);
    const exits = [...new Set(data.map(d => d.exit))].sort((a, b) => a - b);

    const sharpes = data.map(d => d.sharpe);
    const minSharpe = Math.min(...sharpes);
    const maxSharpe = Math.max(...sharpes);
    const range = maxSharpe - minSharpe || 1;

    let html = '<table><thead><tr><th>Exit / Entry</th>';
    entries.forEach(e => { html += '<th>' + e + '</th>'; });
    html += '</tr></thead><tbody>';

    exits.forEach(exit => {
        html += '<tr><th>' + exit + '</th>';
        entries.forEach(entry => {
            const d = data.find(x => x.entry === entry && x.exit === exit);
            if (d) {
                const norm = (d.sharpe - minSharpe) / range;
                const r = Math.round(247 * (1 - norm) + 158 * norm);
                const g = Math.round(118 * (1 - norm) + 206 * norm);
                const b = Math.round(142 * (1 - norm) + 106 * norm);
                html += '<td class="number" style="background:rgba(' + r + ',' + g + ',' + b + ',0.3)">' + d.sharpe.toFixed(2) + '</td>';
            } else {
                html += '<td>-</td>';
            }
        });
        html += '</tr>';
    });

    html += '</tbody></table>';
    container.innerHTML = html;
}
"##;

/// Generate a complete HTML report for a sweep run.
pub fn generate_html_report(manifest: &RunManifest, results: &[SweepConfigResult]) -> Markup {
    let best_sharpe = results
        .iter()
        .max_by(|a, b| a.metrics.sharpe.partial_cmp(&b.metrics.sharpe).unwrap())
        .map(|r| &r.metrics);

    let best_cagr = results
        .iter()
        .max_by(|a, b| a.metrics.cagr.partial_cmp(&b.metrics.cagr).unwrap())
        .map(|r| &r.metrics);

    let equity_js = generate_equity_js(results);
    let heatmap_js = generate_heatmap_js(results);

    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "TrendLab Report - " (manifest.sweep_id) }
                style { (PreEscaped(REPORT_STYLES)) }
            }
            body {
                div class="container" {
                    header {
                        h1 { "TrendLab Sweep Report" }
                        p class="meta" {
                            "Run ID: " (manifest.sweep_id) " | "
                            "Symbol: " (manifest.sweep_config.symbol) " | "
                            (manifest.sweep_config.start_date) " to " (manifest.sweep_config.end_date)
                        }
                    }

                    section {
                        h2 { "Summary" }
                        div class="summary-grid" {
                            @if let Some(m) = best_sharpe {
                                (metric_card("Best Sharpe", format!("{:.3}", m.sharpe), m.sharpe > 0.0))
                            }
                            @if let Some(m) = best_cagr {
                                (metric_card("Best CAGR", format!("{:.1}%", m.cagr * 100.0), m.cagr > 0.0))
                            }
                            (metric_card(
                                "Configurations",
                                format!("{}", results.len()),
                                true
                            ))
                            (metric_card(
                                "Profitable",
                                format!("{}/{}",
                                    results.iter().filter(|r| r.metrics.total_return > 0.0).count(),
                                    results.len()
                                ),
                                true
                            ))
                        }
                    }

                    section {
                        h2 { "Best Configuration Equity Curve" }
                        div class="chart-container" id="equity-chart" {
                            div class="chart-placeholder" { "Loading chart..." }
                        }
                    }

                    section {
                        h2 { "Parameter Heatmap (Sharpe Ratio)" }
                        div class="heatmap-container" id="sharpe-heatmap" {
                            div class="chart-placeholder" { "Loading heatmap..." }
                        }
                    }

                    section {
                        h2 { "Top 10 Configurations by Sharpe" }
                        (configurations_table(results, 10))
                    }

                    section {
                        h2 { "All Configurations" }
                        (configurations_table(results, results.len()))
                    }

                    footer {
                        p {
                            "Generated by TrendLab | "
                            (chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"))
                        }
                    }
                }

                script {
                    (PreEscaped(format!("window.equityData = {};\n", equity_js)))
                    (PreEscaped(format!("window.heatmapData = {};\n", heatmap_js)))
                }
                script { (PreEscaped(REPORT_SCRIPTS)) }
            }
        }
    }
}

fn metric_card(label: &str, value: String, positive: bool) -> Markup {
    let class = if positive { "positive" } else { "negative" };
    html! {
        div class="metric-card" {
            div class="label" { (label) }
            div class={"value " (class)} { (value) }
        }
    }
}

fn configurations_table(results: &[SweepConfigResult], limit: usize) -> Markup {
    let mut sorted: Vec<_> = results.iter().collect();
    sorted.sort_by(|a, b| b.metrics.sharpe.partial_cmp(&a.metrics.sharpe).unwrap());

    html! {
        table {
            thead {
                tr {
                    th { "#" }
                    th { "Entry" }
                    th { "Exit" }
                    th { "Sharpe" }
                    th { "CAGR" }
                    th { "Max DD" }
                    th { "Win Rate" }
                    th { "Trades" }
                    th { "Return" }
                }
            }
            tbody {
                @for (i, r) in sorted.iter().take(limit).enumerate() {
                    @let sharpe_class = if r.metrics.sharpe > 0.0 { "number positive" } else { "number negative" };
                    @let cagr_class = if r.metrics.cagr > 0.0 { "number positive" } else { "number negative" };
                    @let return_class = if r.metrics.total_return > 0.0 { "number positive" } else { "number negative" };
                    tr {
                        td class="number" { (i + 1) }
                        td class="number" { (r.config_id.entry_lookback) }
                        td class="number" { (r.config_id.exit_lookback) }
                        td class=(sharpe_class) {
                            (format!("{:.3}", r.metrics.sharpe))
                        }
                        td class=(cagr_class) {
                            (format!("{:.1}%", r.metrics.cagr * 100.0))
                        }
                        td class="number negative" {
                            (format!("{:.1}%", r.metrics.max_drawdown * 100.0))
                        }
                        td class="number" {
                            (format!("{:.1}%", r.metrics.win_rate * 100.0))
                        }
                        td class="number" { (r.metrics.num_trades) }
                        td class=(return_class) {
                            (format!("{:.1}%", r.metrics.total_return * 100.0))
                        }
                    }
                }
            }
        }
    }
}

fn generate_equity_js(results: &[SweepConfigResult]) -> String {
    let best = results
        .iter()
        .max_by(|a, b| a.metrics.sharpe.partial_cmp(&b.metrics.sharpe).unwrap());

    match best {
        Some(r) => {
            let points: Vec<String> = r
                .backtest_result
                .equity
                .iter()
                .map(|e| {
                    format!(
                        "{{\"date\":\"{}\",\"equity\":{:.2}}}",
                        e.ts.format("%Y-%m-%d"),
                        e.equity
                    )
                })
                .collect();
            format!("[{}]", points.join(","))
        }
        None => "[]".to_string(),
    }
}

fn generate_heatmap_js(results: &[SweepConfigResult]) -> String {
    let points: Vec<String> = results
        .iter()
        .map(|r| {
            format!(
                "{{\"entry\":{},\"exit\":{},\"sharpe\":{:.4}}}",
                r.config_id.entry_lookback, r.config_id.exit_lookback, r.metrics.sharpe
            )
        })
        .collect();
    format!("[{}]", points.join(","))
}

/// Execute HTML report generation.
pub fn execute_html_report(run_id: &str, open_browser: bool) -> Result<PathBuf> {
    let run_dir = PathBuf::from("reports/runs").join(run_id);

    if !run_dir.exists() {
        bail!("Run '{}' not found at {}", run_id, run_dir.display());
    }

    let manifest_path = run_dir.join("manifest.json");
    let manifest_json = fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read manifest: {}", manifest_path.display()))?;
    let manifest: RunManifest = serde_json::from_str(&manifest_json)?;

    let results_path = run_dir.join("results.json");
    let results_json = fs::read_to_string(&results_path)
        .with_context(|| format!("Failed to read results: {}", results_path.display()))?;
    let results: Vec<SweepConfigResult> = serde_json::from_str(&results_json)?;

    let html = generate_html_report(&manifest, &results);
    let output_path = run_dir.join("report.html");
    fs::write(&output_path, html.into_string())?;

    println!("HTML report generated: {}", output_path.display());

    if open_browser {
        #[cfg(target_os = "windows")]
        {
            let _ = std::process::Command::new("cmd")
                .args(["/C", "start", "", output_path.to_str().unwrap()])
                .spawn();
        }
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("open").arg(&output_path).spawn();
        }
        #[cfg(target_os = "linux")]
        {
            let _ = std::process::Command::new("xdg-open")
                .arg(&output_path)
                .spawn();
        }
    }

    Ok(output_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use trendlab_core::{
        BacktestConfig, BacktestResult, ConfigId, EquityPoint, Metrics, ResultPaths, SweepConfig,
        SweepGrid,
    };

    fn mock_manifest() -> RunManifest {
        RunManifest {
            sweep_id: "test_run".to_string(),
            sweep_config: SweepConfig {
                symbol: "TEST".to_string(),
                start_date: "2024-01-01".to_string(),
                end_date: "2024-12-31".to_string(),
                grid: SweepGrid {
                    entry_lookbacks: vec![10, 20],
                    exit_lookbacks: vec![5, 10],
                },
                backtest_config: BacktestConfig::default(),
            },
            data_version: "abc123".to_string(),
            started_at: chrono::Utc::now(),
            completed_at: chrono::Utc::now(),
            result_paths: ResultPaths::for_sweep("test_run"),
        }
    }

    fn mock_equity() -> Vec<EquityPoint> {
        vec![
            EquityPoint {
                ts: chrono::Utc::now(),
                cash: 10000.0,
                position_qty: 0.0,
                close: 100.0,
                equity: 10000.0,
            },
            EquityPoint {
                ts: chrono::Utc::now(),
                cash: 10500.0,
                position_qty: 0.0,
                close: 105.0,
                equity: 10500.0,
            },
        ]
    }

    fn mock_results() -> Vec<SweepConfigResult> {
        vec![
            SweepConfigResult {
                config_id: ConfigId {
                    entry_lookback: 10,
                    exit_lookback: 5,
                },
                backtest_result: BacktestResult {
                    fills: vec![],
                    trades: vec![],
                    pyramid_trades: vec![],
                    equity: mock_equity(),
                },
                metrics: Metrics {
                    sharpe: 1.5,
                    cagr: 0.15,
                    sortino: 2.0,
                    calmar: 1.2,
                    max_drawdown: 0.12,
                    total_return: 0.20,
                    win_rate: 0.55,
                    profit_factor: 1.8,
                    num_trades: 25,
                    turnover: 0.5,
                    max_consecutive_losses: 0,
                    max_consecutive_wins: 0,
                    avg_losing_streak: 0.0,
                },
            },
            SweepConfigResult {
                config_id: ConfigId {
                    entry_lookback: 20,
                    exit_lookback: 10,
                },
                backtest_result: BacktestResult {
                    fills: vec![],
                    trades: vec![],
                    pyramid_trades: vec![],
                    equity: mock_equity(),
                },
                metrics: Metrics {
                    sharpe: 0.8,
                    cagr: 0.08,
                    sortino: 1.2,
                    calmar: 0.6,
                    max_drawdown: 0.18,
                    total_return: 0.10,
                    win_rate: 0.45,
                    profit_factor: 1.2,
                    num_trades: 15,
                    turnover: 0.3,
                    max_consecutive_losses: 0,
                    max_consecutive_wins: 0,
                    avg_losing_streak: 0.0,
                },
            },
        ]
    }

    #[test]
    fn test_generate_html_report() {
        let manifest = mock_manifest();
        let results = mock_results();
        let html = generate_html_report(&manifest, &results);
        let html_str = html.into_string();

        assert!(html_str.contains("<!DOCTYPE html>"));
        assert!(html_str.contains("TrendLab Sweep Report"));
        assert!(html_str.contains("test_run"));
        assert!(html_str.contains("Summary"));
    }

    #[test]
    fn test_generate_heatmap_js() {
        let results = mock_results();
        let js = generate_heatmap_js(&results);

        assert!(js.contains("entry"));
        assert!(js.contains("exit"));
        assert!(js.contains("sharpe"));
    }

    #[test]
    fn test_generate_equity_js() {
        let results = mock_results();
        let js = generate_equity_js(&results);

        assert!(js.contains("equity"));
        assert!(js.contains("date"));
    }
}
