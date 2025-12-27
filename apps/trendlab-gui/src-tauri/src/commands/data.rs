//! Data panel commands for fetching and managing market data.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{Datelike, NaiveDate, TimeZone, Utc};
use sha2::{Digest, Sha256};
use tauri::Emitter;

use crate::{
    error::GuiError,
    events::EventEnvelope,
    jobs::JobStatus,
    state::{AppState, UniverseInfo},
};

use super::system::StartJobResponse;
use trendlab_core::{
    parse_yahoo_csv, write_partitioned_parquet, CacheMetadata, DataQualityChecker,
};

/// Progress payload for data fetch operations.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FetchProgressPayload {
    pub symbol: String,
    pub current: u64,
    pub total: u64,
    pub message: String,
}

/// Completion payload for data fetch operations.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FetchCompletePayload {
    pub symbols_fetched: usize,
    pub symbols_failed: usize,
    pub message: String,
}

/// Search result from Yahoo Finance.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    pub symbol: String,
    pub name: String,
    pub exchange: String,
    pub type_disp: String,
}

/// Get the universe of sectors and tickers.
#[tauri::command]
pub fn get_universe(state: tauri::State<'_, AppState>) -> UniverseInfo {
    state.get_universe_info()
}

/// Get list of symbols that have cached parquet data.
#[tauri::command]
pub fn get_cached_symbols(state: tauri::State<'_, AppState>) -> Vec<String> {
    state.get_cached_symbols()
}

/// Update the selected tickers.
#[tauri::command]
pub fn update_selection(state: tauri::State<'_, AppState>, tickers: Vec<String>) {
    state.set_selected_tickers(tickers);
}

/// Get the currently selected tickers.
#[tauri::command]
pub fn get_selection(state: tauri::State<'_, AppState>) -> Vec<String> {
    state.get_selected_tickers()
}

/// Search for symbols via Yahoo Finance.
#[tauri::command]
pub async fn search_symbols(query: String) -> Result<Vec<SearchResult>, GuiError> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let url = format!(
        "https://query1.finance.yahoo.com/v1/finance/search?q={}&quotesCount=10&newsCount=0",
        urlencoding::encode(&query)
    );

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()
        .map_err(|e| GuiError::Internal(e.to_string()))?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| GuiError::Internal(format!("Network error: {}", e)))?;

    if !response.status().is_success() {
        return Ok(Vec::new());
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| GuiError::Internal(format!("JSON parse error: {}", e)))?;

    let quotes = json
        .get("quotes")
        .and_then(|q| q.as_array())
        .map(|arr| arr.to_vec())
        .unwrap_or_default();

    let results: Vec<SearchResult> = quotes
        .iter()
        .filter_map(|q| {
            let symbol = q.get("symbol")?.as_str()?.to_string();
            let name = q
                .get("longname")
                .or_else(|| q.get("shortname"))
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let exchange = q
                .get("exchange")
                .and_then(|e| e.as_str())
                .unwrap_or("")
                .to_string();
            let type_disp = q
                .get("typeDisp")
                .or_else(|| q.get("quoteType"))
                .and_then(|t| t.as_str())
                .unwrap_or("EQUITY")
                .to_string();

            Some(SearchResult {
                symbol,
                name,
                exchange,
                type_disp,
            })
        })
        .collect();

    Ok(results)
}

/// Start a data fetch job for the given symbols.
#[tauri::command]
pub async fn fetch_data(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    symbols: Vec<String>,
    start: String,
    end: String,
    force: bool,
) -> Result<StartJobResponse, GuiError> {
    if symbols.is_empty() {
        return Err(GuiError::InvalidInput {
            message: "No symbols provided".to_string(),
        });
    }

    let start_date = parse_date(&start)?;
    let end_date = parse_date(&end)?;

    let job_id = format!("fetch-{}", now_ms());
    let token = state.jobs.create(job_id.clone());

    let app_for_task = app.clone();
    let jobs = state.jobs.clone();
    let job_id_clone = job_id.clone();
    let data_config = state.data_config.clone();

    // Clone state for updating cached symbols
    let cached_symbols_update = {
        let cs = state.cached_symbols.read().unwrap();
        std::sync::Arc::new(std::sync::RwLock::new(cs.clone()))
    };

    tokio::spawn(async move {
        jobs.set_status(&job_id_clone, JobStatus::Running);

        let total = symbols.len() as u64;
        let mut fetched = 0usize;
        let mut failed = 0usize;

        for (i, symbol) in symbols.iter().enumerate() {
            // Check cancellation
            if token.is_cancelled() {
                jobs.set_status(&job_id_clone, JobStatus::Cancelled);
                let _ = app_for_task.emit(
                    "data:cancelled",
                    EventEnvelope {
                        event: "data:cancelled",
                        job_id: job_id_clone.clone(),
                        ts_ms: now_ms(),
                        payload: FetchCompletePayload {
                            symbols_fetched: fetched,
                            symbols_failed: failed,
                            message: format!("Cancelled after {} symbols", fetched),
                        },
                    },
                );
                return;
            }

            // Emit progress
            let _ = app_for_task.emit(
                "data:progress",
                EventEnvelope {
                    event: "data:progress",
                    job_id: job_id_clone.clone(),
                    ts_ms: now_ms(),
                    payload: FetchProgressPayload {
                        symbol: symbol.clone(),
                        current: (i + 1) as u64,
                        total,
                        message: format!("Fetching {} ({}/{})", symbol, i + 1, total),
                    },
                },
            );

            // Fetch the symbol
            match fetch_symbol(symbol, start_date, end_date, force, &data_config).await {
                Ok(bars_count) => {
                    fetched += 1;
                    // Mark as cached
                    let mut cs = cached_symbols_update.write().unwrap();
                    cs.insert(symbol.clone());
                    drop(cs);

                    // Log success
                    log::info!("Fetched {} bars for {}", bars_count, symbol);
                }
                Err(e) => {
                    failed += 1;
                    log::error!("Failed to fetch {}: {}", symbol, e);
                }
            }
        }

        // Complete
        jobs.set_status(&job_id_clone, JobStatus::Completed);
        let _ = app_for_task.emit(
            "data:complete",
            EventEnvelope {
                event: "data:complete",
                job_id: job_id_clone.clone(),
                ts_ms: now_ms(),
                payload: FetchCompletePayload {
                    symbols_fetched: fetched,
                    symbols_failed: failed,
                    message: format!("Fetched {} symbols ({} failed)", fetched, failed),
                },
            },
        );
    });

    Ok(StartJobResponse { job_id })
}

/// Fetch data for a single symbol.
async fn fetch_symbol(
    symbol: &str,
    start: NaiveDate,
    end: NaiveDate,
    force: bool,
    config: &crate::state::DataConfig,
) -> Result<usize, anyhow::Error> {
    let raw_dir = config.raw_dir();
    let parquet_dir = config.parquet_dir();

    // Check cache first
    let cache_path = raw_dir.join(format!("yahoo/{}/{}_{}.csv", symbol, start, end));
    let meta_path = raw_dir.join(format!("yahoo/{}/{}_{}.meta.json", symbol, start, end));

    let csv_text = if !force && cache_path.exists() && meta_path.exists() {
        // Load from cache
        std::fs::read_to_string(&cache_path)?
    } else {
        // Fetch fresh data
        let csv = fetch_yahoo_csv(symbol, start, end).await?;

        // Write to cache
        write_cache(&csv, symbol, start, end, &raw_dir)?;

        csv
    };

    // Parse CSV to bars
    let bars = parse_yahoo_csv(&csv_text, symbol, "1d")?;

    // Run quality checks
    let checker = DataQualityChecker::new().with_timeframe("1d");
    let quality_report = checker.check(&bars);

    if !quality_report.is_clean() {
        log::warn!(
            "{}: {} quality issues found",
            symbol,
            quality_report.issues.len()
        );
    }

    // Write normalized Parquet
    if !bars.is_empty() {
        write_partitioned_parquet(&bars, &parquet_dir)?;
    }

    Ok(bars.len())
}

/// Fetch OHLCV data from Yahoo Finance using the chart API.
async fn fetch_yahoo_csv(
    symbol: &str,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<String, anyhow::Error> {
    let start_ts = Utc
        .with_ymd_and_hms(start.year(), start.month(), start.day(), 0, 0, 0)
        .single()
        .unwrap()
        .timestamp();

    let end_ts = Utc
        .with_ymd_and_hms(end.year(), end.month(), end.day(), 23, 59, 59)
        .single()
        .unwrap()
        .timestamp();

    let url = format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{}?period1={}&period2={}&interval=1d&events=history",
        symbol, start_ts, end_ts
    );

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()?;

    let response = client.get(&url).send().await?;

    let status = response.status();
    if !status.is_success() {
        if status.as_u16() == 404 {
            anyhow::bail!("Symbol not found: {}", symbol);
        }
        anyhow::bail!("HTTP error {}: {}", status.as_u16(), status.as_str());
    }

    let json: serde_json::Value = response.json().await?;

    // Extract data from the chart API response
    let chart = json
        .get("chart")
        .and_then(|c| c.get("result"))
        .and_then(|r| r.get(0))
        .ok_or_else(|| anyhow::anyhow!("Invalid chart response structure"))?;

    // Check for errors in the response
    if let Some(error) = json.get("chart").and_then(|c| c.get("error")) {
        if !error.is_null() {
            let error_msg = error
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("Unknown error");
            anyhow::bail!("Yahoo Finance error: {}", error_msg);
        }
    }

    let timestamps = chart
        .get("timestamp")
        .and_then(|t| t.as_array())
        .ok_or_else(|| anyhow::anyhow!("Missing timestamp array"))?;

    let indicators = chart
        .get("indicators")
        .and_then(|i| i.get("quote"))
        .and_then(|q| q.get(0))
        .ok_or_else(|| anyhow::anyhow!("Missing quote indicators"))?;

    let adj_close = chart
        .get("indicators")
        .and_then(|i| i.get("adjclose"))
        .and_then(|a| a.get(0))
        .and_then(|a| a.get("adjclose"))
        .and_then(|a| a.as_array());

    let opens = indicators.get("open").and_then(|o| o.as_array());
    let highs = indicators.get("high").and_then(|h| h.as_array());
    let lows = indicators.get("low").and_then(|l| l.as_array());
    let closes = indicators.get("close").and_then(|c| c.as_array());
    let volumes = indicators.get("volume").and_then(|v| v.as_array());

    // Convert to CSV format
    let mut csv = String::from("Date,Open,High,Low,Close,Adj Close,Volume\n");

    for (i, ts) in timestamps.iter().enumerate() {
        let ts_val = ts.as_i64().unwrap_or(0);
        let date = Utc.timestamp_opt(ts_val, 0).single();
        let date_str = match date {
            Some(d) => d.format("%Y-%m-%d").to_string(),
            None => continue,
        };

        let open = opens
            .and_then(|o| o.get(i))
            .and_then(|v| v.as_f64())
            .map(|v| format!("{:.6}", v))
            .unwrap_or_else(|| "null".to_string());

        let high = highs
            .and_then(|h| h.get(i))
            .and_then(|v| v.as_f64())
            .map(|v| format!("{:.6}", v))
            .unwrap_or_else(|| "null".to_string());

        let low = lows
            .and_then(|l| l.get(i))
            .and_then(|v| v.as_f64())
            .map(|v| format!("{:.6}", v))
            .unwrap_or_else(|| "null".to_string());

        let close = closes
            .and_then(|c| c.get(i))
            .and_then(|v| v.as_f64())
            .map(|v| format!("{:.6}", v))
            .unwrap_or_else(|| "null".to_string());

        let adj = adj_close
            .and_then(|a| a.get(i))
            .and_then(|v| v.as_f64())
            .map(|v| format!("{:.6}", v))
            .unwrap_or_else(|| close.clone());

        let volume = volumes
            .and_then(|v| v.get(i))
            .and_then(|v| v.as_f64())
            .map(|v| format!("{:.0}", v))
            .unwrap_or_else(|| "0".to_string());

        csv.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            date_str, open, high, low, close, adj, volume
        ));
    }

    Ok(csv)
}

/// Write raw CSV and metadata to cache.
fn write_cache(
    csv_text: &str,
    symbol: &str,
    start: NaiveDate,
    end: NaiveDate,
    raw_dir: &Path,
) -> Result<(), anyhow::Error> {
    let cache_dir = raw_dir.join(format!("yahoo/{}", symbol));
    std::fs::create_dir_all(&cache_dir)?;

    let cache_path = cache_dir.join(format!("{}_{}.csv", start, end));
    let meta_path = cache_dir.join(format!("{}_{}.meta.json", start, end));

    // Write CSV
    std::fs::write(&cache_path, csv_text)?;

    // Calculate checksum
    let mut hasher = Sha256::new();
    hasher.update(csv_text.as_bytes());
    let checksum = format!("{:x}", hasher.finalize());

    // Count rows (excluding header)
    let row_count = csv_text.lines().skip(1).filter(|l| !l.is_empty()).count();

    // Write metadata
    let metadata = CacheMetadata::new("yahoo", symbol, start, end, "1d", row_count, checksum);
    let meta_json = serde_json::to_string_pretty(&metadata)?;
    std::fs::write(&meta_path, meta_json)?;

    Ok(())
}

fn parse_date(s: &str) -> Result<NaiveDate, GuiError> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| GuiError::InvalidInput {
        message: format!("Invalid date format: '{}'. Expected YYYY-MM-DD", s),
    })
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_millis(0))
        .as_millis() as u64
}

/// URL-encode a string for use in query parameters.
mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::new();
        for c in s.chars() {
            match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
                ' ' => result.push_str("%20"),
                _ => {
                    for byte in c.to_string().as_bytes() {
                        result.push_str(&format!("%{:02X}", byte));
                    }
                }
            }
        }
        result
    }
}
