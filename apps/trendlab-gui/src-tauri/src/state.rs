use crate::jobs::Jobs;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::RwLock;
use trendlab_core::{Sector, Universe};

/// Configuration for the data layer paths.
#[derive(Debug, Clone)]
pub struct DataConfig {
    /// Base directory for all data (typically "data")
    pub data_dir: PathBuf,
}

impl Default for DataConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("data"),
        }
    }
}

impl DataConfig {
    /// Path to raw cache directory.
    pub fn raw_dir(&self) -> PathBuf {
        self.data_dir.join("raw")
    }

    /// Path to normalized Parquet directory.
    pub fn parquet_dir(&self) -> PathBuf {
        self.data_dir.join("parquet")
    }

    /// Path to quality reports directory.
    pub fn reports_dir(&self) -> PathBuf {
        self.data_dir.join("reports")
    }
}

/// Serializable sector for frontend.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SectorInfo {
    pub id: String,
    pub name: String,
    pub tickers: Vec<String>,
}

impl From<&Sector> for SectorInfo {
    fn from(sector: &Sector) -> Self {
        Self {
            id: sector.id.clone(),
            name: sector.name.clone(),
            tickers: sector.tickers.clone(),
        }
    }
}

/// Serializable universe for frontend.
#[derive(Debug, Clone, serde::Serialize)]
pub struct UniverseInfo {
    pub name: String,
    pub description: String,
    pub sectors: Vec<SectorInfo>,
}

impl From<&Universe> for UniverseInfo {
    fn from(universe: &Universe) -> Self {
        Self {
            name: universe.name.clone(),
            description: universe.description.clone(),
            sectors: universe.sectors.iter().map(SectorInfo::from).collect(),
        }
    }
}

#[derive(Debug)]
pub struct AppState {
    /// Job lifecycle tracking
    pub jobs: Jobs,
    /// Universe of sectors and tickers
    pub universe: RwLock<Universe>,
    /// Currently selected tickers for operations
    pub selected_tickers: RwLock<HashSet<String>>,
    /// Symbols that have cached parquet data
    pub cached_symbols: RwLock<HashSet<String>>,
    /// Data directory configuration
    pub data_config: DataConfig,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            jobs: Jobs::new(),
            universe: RwLock::new(Universe::default_universe()),
            selected_tickers: RwLock::new(HashSet::new()),
            cached_symbols: RwLock::new(HashSet::new()),
            data_config: DataConfig::default(),
        }
    }

    /// Initialize cached symbols by scanning the parquet directory.
    pub fn init_cached_symbols(&self) {
        let parquet_dir = self.data_config.parquet_dir().join("1d");
        if let Ok(entries) = std::fs::read_dir(&parquet_dir) {
            let mut cached = self.cached_symbols.write().unwrap();
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Directory name format: "symbol=AAPL"
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if let Some(symbol) = name.strip_prefix("symbol=") {
                            cached.insert(symbol.to_string());
                        }
                    }
                }
            }
        }
    }

    /// Get universe as serializable struct.
    pub fn get_universe_info(&self) -> UniverseInfo {
        let universe = self.universe.read().unwrap();
        UniverseInfo::from(&*universe)
    }

    /// Get list of cached symbols.
    pub fn get_cached_symbols(&self) -> Vec<String> {
        let cached = self.cached_symbols.read().unwrap();
        let mut symbols: Vec<_> = cached.iter().cloned().collect();
        symbols.sort();
        symbols
    }

    /// Add a symbol to the cached set.
    pub fn add_cached_symbol(&self, symbol: &str) {
        let mut cached = self.cached_symbols.write().unwrap();
        cached.insert(symbol.to_string());
    }

    /// Get selected tickers.
    pub fn get_selected_tickers(&self) -> Vec<String> {
        let selected = self.selected_tickers.read().unwrap();
        let mut tickers: Vec<_> = selected.iter().cloned().collect();
        tickers.sort();
        tickers
    }

    /// Set selected tickers.
    pub fn set_selected_tickers(&self, tickers: Vec<String>) {
        let mut selected = self.selected_tickers.write().unwrap();
        *selected = tickers.into_iter().collect();
    }
}
