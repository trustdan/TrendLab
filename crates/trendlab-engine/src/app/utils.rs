//! Utility functions for the TUI application.

/// Scan the parquet directory for available symbols
pub fn scan_parquet_directory() -> Vec<String> {
    let parquet_dir = std::path::Path::new("data/parquet/1d");

    if !parquet_dir.exists() {
        return vec![];
    }

    let mut symbols = Vec::new();

    if let Ok(entries) = std::fs::read_dir(parquet_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("symbol=") {
                if let Some(symbol) = name.strip_prefix("symbol=") {
                    symbols.push(symbol.to_string());
                }
            }
        }
    }

    symbols.sort();
    symbols
}

/// Calculate drawdown curve from equity curve
pub fn calculate_drawdown(equity: &[f64]) -> Vec<f64> {
    let mut max_equity = 0.0_f64;
    equity
        .iter()
        .map(|&eq| {
            max_equity = max_equity.max(eq);
            if max_equity > 0.0 {
                (eq / max_equity - 1.0) * 100.0
            } else {
                0.0
            }
        })
        .collect()
}
