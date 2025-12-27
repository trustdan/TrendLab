//! Artifact command implementation - export and validation for Pine parity.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::PathBuf;

use trendlab_core::{
    create_donchian_artifact, read_parquet, CostModel, RunManifest, StrategyArtifact,
    SweepConfigResult,
};

/// Get the reports base directory.
fn reports_dir() -> PathBuf {
    PathBuf::from("reports/runs")
}

/// Get the artifacts output directory.
fn artifacts_dir() -> PathBuf {
    PathBuf::from("artifacts")
}

/// Get the parquet data directory.
fn parquet_dir() -> PathBuf {
    PathBuf::from("data/parquet")
}

/// Load sweep results from a run directory.
fn load_sweep_results(run_id: &str) -> Result<(RunManifest, Vec<SweepConfigResult>)> {
    let run_dir = reports_dir().join(run_id);

    if !run_dir.exists() {
        bail!("Run '{}' not found at {}", run_id, run_dir.display());
    }

    // Load manifest
    let manifest_path = run_dir.join("manifest.json");
    if !manifest_path.exists() {
        bail!("Manifest not found at {}", manifest_path.display());
    }

    let manifest_json = fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read manifest: {}", manifest_path.display()))?;
    let manifest: RunManifest = serde_json::from_str(&manifest_json)
        .with_context(|| format!("Failed to parse manifest: {}", manifest_path.display()))?;

    // Load results (JSON format)
    let results_path = run_dir.join("results.json");
    if !results_path.exists() {
        bail!("Results not found at {}", results_path.display());
    }

    let results_json = fs::read_to_string(&results_path)
        .with_context(|| format!("Failed to read results: {}", results_path.display()))?;
    let results: Vec<SweepConfigResult> = serde_json::from_str(&results_json)
        .with_context(|| format!("Failed to parse results: {}", results_path.display()))?;

    Ok((manifest, results))
}

/// Export a strategy artifact for a specific configuration.
pub fn execute_export(run_id: &str, config_id: &str) -> Result<PathBuf> {
    println!("Exporting artifact...");
    println!("  Run ID:    {}", run_id);
    println!("  Config ID: {}", config_id);
    println!();

    // Load sweep results
    let (manifest, results) = load_sweep_results(run_id)?;

    // Parse config_id to get entry/exit lookbacks
    // Expected format: "entry_X_exit_Y"
    let (entry_lookback, exit_lookback) = parse_config_id(config_id)?;

    // Find matching result
    let config_result = results
        .iter()
        .find(|r| {
            r.config_id.entry_lookback == entry_lookback
                && r.config_id.exit_lookback == exit_lookback
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Config not found: entry={}, exit={}",
                entry_lookback,
                exit_lookback
            )
        })?;

    println!("Found configuration:");
    println!("  Entry lookback: {}", entry_lookback);
    println!("  Exit lookback:  {}", exit_lookback);
    println!("  Sharpe:         {:.3}", config_result.metrics.sharpe);
    println!(
        "  CAGR:           {:.1}%",
        config_result.metrics.cagr * 100.0
    );
    println!();

    // Load bars from Parquet
    let symbol = &manifest.sweep_config.symbol;
    let bars = load_bars_for_artifact(
        symbol,
        &manifest.sweep_config.start_date,
        &manifest.sweep_config.end_date,
    )?;

    println!("Loaded {} bars for {}", bars.len(), symbol);

    // Create cost model from manifest
    let cost_model = CostModel {
        fees_bps_per_side: manifest
            .sweep_config
            .backtest_config
            .cost_model
            .fees_bps_per_side,
        slippage_bps: manifest
            .sweep_config
            .backtest_config
            .cost_model
            .slippage_bps,
    };

    // Create the artifact
    let artifact = create_donchian_artifact(
        &bars,
        entry_lookback,
        exit_lookback,
        cost_model,
        &config_result.backtest_result,
    )?;

    // Write artifact to file
    let output_dir = artifacts_dir().join(run_id);
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("Failed to create directory: {}", output_dir.display()))?;

    let output_path = output_dir.join(format!("{}.json", config_id));
    let json =
        serde_json::to_string_pretty(&artifact).context("Failed to serialize artifact to JSON")?;

    fs::write(&output_path, &json)
        .with_context(|| format!("Failed to write artifact: {}", output_path.display()))?;

    println!("Artifact exported to: {}", output_path.display());

    // Print artifact summary
    print_artifact_summary(&artifact);

    Ok(output_path)
}

/// Validate an artifact file against the schema.
pub fn execute_validate(path: &str) -> Result<()> {
    println!("Validating artifact: {}", path);
    println!();

    // Read the file
    let json =
        fs::read_to_string(path).with_context(|| format!("Failed to read artifact: {}", path))?;

    // Parse as StrategyArtifact
    let artifact: StrategyArtifact =
        serde_json::from_str(&json).with_context(|| "Failed to parse artifact JSON")?;

    // Basic validation
    println!("Schema version:  {}", artifact.schema_version);
    println!("Strategy ID:     {}", artifact.strategy_id);
    println!("Symbol:          {}", artifact.symbol);
    println!("Timeframe:       {}", artifact.timeframe);
    println!("Indicators:      {}", artifact.indicators.len());
    println!("Parity vectors:  {}", artifact.parity_vectors.vectors.len());

    // Check for required fields
    let mut issues = Vec::new();

    if artifact.schema_version.is_empty() {
        issues.push("Missing schema_version");
    }
    if artifact.strategy_id.is_empty() {
        issues.push("Missing strategy_id");
    }
    if artifact.symbol.is_empty() {
        issues.push("Missing symbol");
    }
    if artifact.indicators.is_empty() {
        issues.push("No indicators defined");
    }
    if artifact.parity_vectors.vectors.is_empty() {
        issues.push("No parity vectors");
    }

    // Check parity vectors have signals
    let signals_count = artifact
        .parity_vectors
        .vectors
        .iter()
        .filter(|v| v.signal.is_some())
        .count();

    if signals_count == 0 {
        issues.push("No signals in parity vectors");
    }

    println!();
    if issues.is_empty() {
        println!("Validation: PASSED");
        println!(
            "  {} parity vectors with {} signals",
            artifact.parity_vectors.vectors.len(),
            signals_count
        );
    } else {
        println!("Validation: FAILED");
        for issue in &issues {
            println!("  - {}", issue);
        }
        bail!("Artifact validation failed with {} issues", issues.len());
    }

    Ok(())
}

/// Parse config_id string to extract entry and exit lookbacks.
fn parse_config_id(config_id: &str) -> Result<(usize, usize)> {
    // Expected format: "entry_X_exit_Y"
    let parts: Vec<&str> = config_id.split('_').collect();

    if parts.len() >= 4 && parts[0] == "entry" && parts[2] == "exit" {
        let entry = parts[1]
            .parse::<usize>()
            .with_context(|| format!("Invalid entry lookback in config_id: {}", config_id))?;
        let exit = parts[3]
            .parse::<usize>()
            .with_context(|| format!("Invalid exit lookback in config_id: {}", config_id))?;
        return Ok((entry, exit));
    }

    // Try alternative format: "X_Y" (entry_exit)
    if parts.len() == 2 {
        let entry = parts[0]
            .parse::<usize>()
            .with_context(|| format!("Invalid entry lookback in config_id: {}", config_id))?;
        let exit = parts[1]
            .parse::<usize>()
            .with_context(|| format!("Invalid exit lookback in config_id: {}", config_id))?;
        return Ok((entry, exit));
    }

    bail!(
        "Invalid config_id format: '{}'. Expected 'entry_X_exit_Y' or 'X_Y'",
        config_id
    )
}

/// Load bars from Parquet for the artifact.
fn load_bars_for_artifact(
    symbol: &str,
    start_date: &str,
    end_date: &str,
) -> Result<Vec<trendlab_core::Bar>> {
    // Parse dates
    let start = chrono::NaiveDate::parse_from_str(start_date, "%Y-%m-%d")
        .with_context(|| format!("Invalid start date: {}", start_date))?;
    let end = chrono::NaiveDate::parse_from_str(end_date, "%Y-%m-%d")
        .with_context(|| format!("Invalid end date: {}", end_date))?;

    // Determine which year partitions to load
    let start_year = start.year();
    let end_year = end.year();

    let mut all_bars = Vec::new();

    for year in start_year..=end_year {
        let parquet_path = parquet_dir()
            .join("1d")
            .join(format!("symbol={}", symbol))
            .join(format!("year={}", year))
            .join("data.parquet");

        if parquet_path.exists() {
            let bars = read_parquet(&parquet_path)
                .with_context(|| format!("Failed to read parquet: {}", parquet_path.display()))?;
            all_bars.extend(bars);
        }
    }

    if all_bars.is_empty() {
        bail!(
            "No bars found for symbol {} in date range {} to {}",
            symbol,
            start_date,
            end_date
        );
    }

    // Filter to date range
    let start_dt = start.and_hms_opt(0, 0, 0).unwrap();
    let end_dt = end.and_hms_opt(23, 59, 59).unwrap();

    let filtered: Vec<_> = all_bars
        .into_iter()
        .filter(|bar| {
            let bar_date = bar.ts.naive_utc();
            bar_date >= start_dt && bar_date <= end_dt
        })
        .collect();

    Ok(filtered)
}

/// Print a summary of the artifact.
fn print_artifact_summary(artifact: &StrategyArtifact) {
    println!();
    println!("Artifact Summary:");
    println!(
        "  Strategy:       {} v{}",
        artifact.strategy_id,
        artifact.strategy_version.as_deref().unwrap_or("?")
    );
    println!("  Symbol:         {}", artifact.symbol);
    println!("  Timeframe:      {}", artifact.timeframe);
    println!("  Fill model:     {}", artifact.fill_model);
    println!(
        "  Fees:           {} bps/side",
        artifact.cost_model.fees_bps_per_side
    );
    println!("  Slippage:       {} bps", artifact.cost_model.slippage_bps);
    println!();

    println!("  Indicators:");
    for ind in &artifact.indicators {
        println!("    - {} ({})", ind.id, ind.indicator_type);
    }

    println!();
    println!("  Rules:");
    println!("    Entry: {}", artifact.rules.entry.condition);
    println!("    Exit:  {}", artifact.rules.exit.condition);

    println!();
    println!(
        "  Parity vectors: {} bars",
        artifact.parity_vectors.vectors.len()
    );

    // Count signals
    let entries = artifact
        .parity_vectors
        .vectors
        .iter()
        .filter(|v| v.signal.as_deref() == Some("enter_long"))
        .count();
    let exits = artifact
        .parity_vectors
        .vectors
        .iter()
        .filter(|v| v.signal.as_deref() == Some("exit_long"))
        .count();

    println!("    - {} entry signals", entries);
    println!("    - {} exit signals", exits);
}

use chrono::Datelike;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config_id_standard() {
        let (entry, exit) = parse_config_id("entry_10_exit_5").unwrap();
        assert_eq!(entry, 10);
        assert_eq!(exit, 5);
    }

    #[test]
    fn test_parse_config_id_short() {
        let (entry, exit) = parse_config_id("20_10").unwrap();
        assert_eq!(entry, 20);
        assert_eq!(exit, 10);
    }

    #[test]
    fn test_parse_config_id_invalid() {
        assert!(parse_config_id("invalid").is_err());
        assert!(parse_config_id("entry_abc_exit_5").is_err());
    }
}
