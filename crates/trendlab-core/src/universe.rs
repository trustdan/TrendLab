//! Universe configuration for sector-based ticker organization.
//!
//! Provides:
//! - Sector and Universe types for organizing tickers
//! - TOML-based configuration loading
//! - Utility methods for ticker lookups

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use thiserror::Error;

/// Errors that can occur when loading universe configuration.
#[derive(Debug, Error)]
pub enum UniverseError {
    #[error("Failed to read universe file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Failed to parse universe TOML: {0}")]
    ParseError(#[from] toml::de::Error),

    #[error("Sector not found: {0}")]
    SectorNotFound(String),
}

/// A sector containing related tickers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Sector {
    /// Unique identifier (e.g., "technology", "healthcare")
    pub id: String,
    /// Display name (e.g., "Technology", "Healthcare")
    pub name: String,
    /// List of ticker symbols in this sector
    pub tickers: Vec<String>,
}

impl Sector {
    /// Create a new sector with the given id, name, and tickers.
    pub fn new(id: impl Into<String>, name: impl Into<String>, tickers: Vec<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            tickers,
        }
    }

    /// Returns the number of tickers in this sector.
    pub fn len(&self) -> usize {
        self.tickers.len()
    }

    /// Returns true if this sector has no tickers.
    pub fn is_empty(&self) -> bool {
        self.tickers.is_empty()
    }

    /// Check if a ticker belongs to this sector.
    pub fn contains(&self, ticker: &str) -> bool {
        self.tickers.iter().any(|t| t == ticker)
    }
}

/// Top-level universe configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniverseConfig {
    /// Universe metadata
    pub universe: UniverseMetadata,
    /// List of sectors
    pub sectors: Vec<Sector>,
}

/// Universe metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniverseMetadata {
    /// Name of the universe
    pub name: String,
    /// Description
    #[serde(default)]
    pub description: String,
}

/// A universe of tickers organized by sector.
#[derive(Debug, Clone)]
pub struct Universe {
    /// Name of the universe
    pub name: String,
    /// Description
    pub description: String,
    /// Sectors in this universe
    pub sectors: Vec<Sector>,
}

impl Universe {
    /// Load a universe from a TOML configuration file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, UniverseError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_toml(&content)
    }

    /// Parse a universe from TOML string content.
    pub fn from_toml(content: &str) -> Result<Self, UniverseError> {
        let config: UniverseConfig = toml::from_str(content)?;
        Ok(Self {
            name: config.universe.name,
            description: config.universe.description,
            sectors: config.sectors,
        })
    }

    /// Returns the number of sectors in this universe.
    pub fn sector_count(&self) -> usize {
        self.sectors.len()
    }

    /// Returns the total number of unique tickers across all sectors.
    pub fn ticker_count(&self) -> usize {
        self.all_tickers().len()
    }

    /// Get a sector by its ID.
    pub fn get_sector(&self, id: &str) -> Option<&Sector> {
        self.sectors.iter().find(|s| s.id == id)
    }

    /// Get a sector by index.
    pub fn get_sector_by_index(&self, index: usize) -> Option<&Sector> {
        self.sectors.get(index)
    }

    /// Returns all unique tickers across all sectors.
    pub fn all_tickers(&self) -> HashSet<String> {
        self.sectors
            .iter()
            .flat_map(|s| s.tickers.iter().cloned())
            .collect()
    }

    /// Returns all tickers as a sorted vector.
    pub fn all_tickers_sorted(&self) -> Vec<String> {
        let mut tickers: Vec<String> = self.all_tickers().into_iter().collect();
        tickers.sort();
        tickers
    }

    /// Find which sector a ticker belongs to.
    pub fn find_sector_for_ticker(&self, ticker: &str) -> Option<&Sector> {
        self.sectors.iter().find(|s| s.contains(ticker))
    }

    /// Build a fast lookup table mapping ticker symbols to sector names.
    ///
    /// Returns a HashMap where keys are ticker symbols (e.g., "AAPL") and
    /// values are sector display names (e.g., "Technology").
    ///
    /// This is useful for enriching DataFrames with sector information
    /// without repeated sector lookups.
    ///
    /// # Example
    /// ```
    /// use trendlab_core::universe::Universe;
    ///
    /// let universe = Universe::default_universe();
    /// let lookup = universe.build_sector_lookup();
    ///
    /// assert_eq!(lookup.get("AAPL"), Some(&"Technology".to_string()));
    /// assert_eq!(lookup.get("JPM"), Some(&"Financial".to_string()));
    /// ```
    pub fn build_sector_lookup(&self) -> HashMap<String, String> {
        let mut lookup = HashMap::new();
        for sector in &self.sectors {
            for ticker in &sector.tickers {
                lookup.insert(ticker.clone(), sector.name.clone());
            }
        }
        lookup
    }

    /// Build a fast lookup table mapping ticker symbols to sector IDs.
    ///
    /// Similar to `build_sector_lookup()` but returns sector IDs instead of names.
    /// Useful when you need machine-readable identifiers (e.g., "technology")
    /// rather than display names (e.g., "Technology").
    pub fn build_sector_id_lookup(&self) -> HashMap<String, String> {
        let mut lookup = HashMap::new();
        for sector in &self.sectors {
            for ticker in &sector.tickers {
                lookup.insert(ticker.clone(), sector.id.clone());
            }
        }
        lookup
    }

    /// Get tickers for a specific sector by ID.
    pub fn tickers_for_sector(&self, sector_id: &str) -> Result<&[String], UniverseError> {
        self.get_sector(sector_id)
            .map(|s| s.tickers.as_slice())
            .ok_or_else(|| UniverseError::SectorNotFound(sector_id.to_string()))
    }

    /// Create a default universe with the built-in sector/ticker mapping.
    pub fn default_universe() -> Self {
        Self {
            name: "US Equities".to_string(),
            description: "Default curated list of US equities by sector".to_string(),
            sectors: vec![
                // Individual Stocks by Sector
                Sector::new(
                    "basic_materials",
                    "Basic Materials",
                    vec![
                        "LIN", "SCCO", "NEM", "CF", "FCX", "APD", "SHW", "DOW", "NUE", "ECL", "DD",
                        "PPG", "ALB", "CTVA",
                    ]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                ),
                Sector::new(
                    "comms_services",
                    "Communication Services",
                    vec![
                        "GOOG", "META", "RDDT", "NFLX", "DIS", "T", "VZ", "CMCSA", "TMUS", "SPOT",
                        "ROKU", "CHTR", "EA", "TTWO",
                    ]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                ),
                Sector::new(
                    "consumer_cyclical",
                    "Consumer Cyclical",
                    vec![
                        "AMZN", "TJX", "DASH", "HD", "BABA", "TSLA", "MCD", "NKE", "SBUX", "LOW",
                        "GM", "F", "CMG", "BKNG", "ABNB",
                    ]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                ),
                Sector::new(
                    "consumer_defensive",
                    "Consumer Defensive",
                    vec![
                        "WMT", "PM", "UL", "COST", "PG", "MDLZ", "KO", "PEP", "MO", "CL", "GIS",
                        "KHC", "STZ", "SYY", "KR", "HSY",
                    ]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                ),
                Sector::new(
                    "energy",
                    "Energy",
                    vec![
                        "XOM", "CVX", "SHEL", "COP", "SLB", "EOG", "MPC", "VLO", "PSX", "OXY",
                        "BP", "HAL", "DVN", "KMI",
                    ]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                ),
                Sector::new(
                    "financial",
                    "Financial",
                    vec![
                        "BRK-B", "JPM", "MS", "COIN", "PYPL", "IBKR", "BAC", "WFC", "GS", "C", "V",
                        "MA", "BLK", "SCHW", "AXP", "CME", "ICE",
                    ]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                ),
                Sector::new(
                    "healthcare",
                    "Healthcare",
                    vec![
                        "LLY", "JNJ", "ABBV", "UNH", "PFE", "MRK", "TMO", "ABT", "BMY", "AMGN",
                        "GILD", "CVS", "ISRG", "DHR", "VRTX",
                    ]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                ),
                Sector::new(
                    "industrials",
                    "Industrials",
                    vec![
                        "GE", "RTX", "BA", "ETN", "MMM", "CAT", "HON", "UNP", "DE", "LMT", "UPS",
                        "NOC", "GD", "WM", "FDX", "EMR",
                    ]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                ),
                Sector::new(
                    "real_estate",
                    "Real Estate",
                    vec![
                        "PLD", "AMT", "O", "EQIX", "CCI", "SPG", "PSA", "WELL", "DLR", "AVB",
                        "EQR", "SBAC", "VTR",
                    ]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                ),
                Sector::new(
                    "technology",
                    "Technology",
                    vec![
                        "NVDA", "AAPL", "PLTR", "CSCO", "MSFT", "AMD", "AVGO", "CRM", "ORCL",
                        "ADBE", "INTC", "QCOM", "TXN", "NOW", "MU", "AMAT", "IBM",
                    ]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                ),
                Sector::new(
                    "utilities",
                    "Utilities",
                    vec![
                        "EIX", "NEE", "DUK", "AEP", "SO", "D", "SRE", "XEL", "ED", "EXC", "WEC",
                        "CEG", "PEG", "AWK", "ES",
                    ]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                ),
                // Broad Market ETFs
                Sector::new(
                    "etf_broad_market",
                    "ETF - Broad Market",
                    vec![
                        "SPY", "VOO", "IVV", "QQQ", "VTI", "IWM", "DIA", "RSP", "VXF", "SPLG",
                    ]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                ),
                // Sector ETFs
                Sector::new(
                    "etf_technology",
                    "ETF - Technology",
                    vec!["XLK", "VGT", "SMH", "SOXX", "IGV", "FTEC", "FDN"]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                ),
                Sector::new(
                    "etf_financials",
                    "ETF - Financials",
                    vec!["XLF", "VFH", "KRE", "KBE", "IAI"]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                ),
                Sector::new(
                    "etf_healthcare",
                    "ETF - Healthcare",
                    vec!["XLV", "VHT", "IBB", "XBI", "ARKG"]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                ),
                Sector::new(
                    "etf_energy",
                    "ETF - Energy",
                    vec!["XLE", "VDE", "XOP", "OIH", "AMLP"]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                ),
                Sector::new(
                    "etf_industrials",
                    "ETF - Industrials",
                    vec!["XLI", "VIS", "ITA", "PPA"]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                ),
                Sector::new(
                    "etf_consumer_disc",
                    "ETF - Consumer Discretionary",
                    vec!["XLY", "VCR", "BITO"]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                ),
                Sector::new(
                    "etf_consumer_staples",
                    "ETF - Consumer Staples",
                    vec!["XLP", "VDC", "KXI"]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                ),
                Sector::new(
                    "etf_utilities",
                    "ETF - Utilities",
                    vec!["XLU", "VPU"].into_iter().map(String::from).collect(),
                ),
                Sector::new(
                    "etf_materials",
                    "ETF - Materials",
                    vec!["XLB", "VAW", "GDX", "GDXJ", "SLV", "GLD"]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                ),
                Sector::new(
                    "etf_real_estate",
                    "ETF - Real Estate",
                    vec!["VNQ", "XLRE", "IYR", "MORT"]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                ),
                Sector::new(
                    "etf_communications",
                    "ETF - Communications",
                    vec!["XLC", "VOX"].into_iter().map(String::from).collect(),
                ),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sector_creation() {
        let sector = Sector::new(
            "tech",
            "Technology",
            vec!["AAPL".to_string(), "MSFT".to_string()],
        );
        assert_eq!(sector.id, "tech");
        assert_eq!(sector.name, "Technology");
        assert_eq!(sector.len(), 2);
        assert!(sector.contains("AAPL"));
        assert!(!sector.contains("GOOG"));
    }

    #[test]
    fn test_universe_from_toml() {
        let toml = r#"
[universe]
name = "Test Universe"
description = "A test"

[[sectors]]
id = "tech"
name = "Technology"
tickers = ["AAPL", "MSFT"]

[[sectors]]
id = "finance"
name = "Finance"
tickers = ["JPM", "GS"]
"#;

        let universe = Universe::from_toml(toml).unwrap();
        assert_eq!(universe.name, "Test Universe");
        assert_eq!(universe.sector_count(), 2);
        assert_eq!(universe.ticker_count(), 4);

        let tech = universe.get_sector("tech").unwrap();
        assert_eq!(tech.name, "Technology");
        assert_eq!(tech.tickers, vec!["AAPL", "MSFT"]);
    }

    #[test]
    fn test_all_tickers() {
        let universe = Universe::default_universe();
        let all = universe.all_tickers();
        assert!(all.contains("AAPL"));
        assert!(all.contains("JPM"));
        assert!(all.contains("XOM"));
    }

    #[test]
    fn test_find_sector_for_ticker() {
        let universe = Universe::default_universe();
        let sector = universe.find_sector_for_ticker("AAPL").unwrap();
        assert_eq!(sector.id, "technology");

        let sector = universe.find_sector_for_ticker("JPM").unwrap();
        assert_eq!(sector.id, "financial");

        assert!(universe.find_sector_for_ticker("UNKNOWN").is_none());
    }

    #[test]
    fn test_default_universe() {
        let universe = Universe::default_universe();
        // 11 individual stock sectors + 12 ETF sectors = 23 total
        assert_eq!(universe.sector_count(), 23);
        // Expanded to ~200 tickers
        assert!(universe.ticker_count() > 180);
    }

    #[test]
    fn test_build_sector_lookup() {
        let universe = Universe::default_universe();
        let lookup = universe.build_sector_lookup();

        // Check expected mappings
        assert_eq!(lookup.get("AAPL"), Some(&"Technology".to_string()));
        assert_eq!(lookup.get("MSFT"), Some(&"Technology".to_string()));
        assert_eq!(lookup.get("JPM"), Some(&"Financial".to_string()));
        assert_eq!(lookup.get("XOM"), Some(&"Energy".to_string()));
        assert_eq!(lookup.get("LLY"), Some(&"Healthcare".to_string()));

        // Unknown ticker should not be in lookup
        assert!(!lookup.contains_key("UNKNOWN"));

        // Should have all tickers from universe
        assert_eq!(lookup.len(), universe.ticker_count());
    }

    #[test]
    fn test_build_sector_id_lookup() {
        let universe = Universe::default_universe();
        let lookup = universe.build_sector_id_lookup();

        // Check expected mappings (IDs, not names)
        assert_eq!(lookup.get("AAPL"), Some(&"technology".to_string()));
        assert_eq!(lookup.get("JPM"), Some(&"financial".to_string()));
        assert_eq!(lookup.get("XOM"), Some(&"energy".to_string()));
        assert_eq!(lookup.get("LLY"), Some(&"healthcare".to_string()));
    }
}
