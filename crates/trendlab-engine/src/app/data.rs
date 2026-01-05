//! Data panel state and related types.

use std::collections::{HashMap, HashSet};

use trendlab_core::{Bar, Sector, Universe};

/// Search suggestion from Yahoo.
#[derive(Debug, Clone)]
pub struct SearchSuggestion {
    pub symbol: String,
    pub name: String,
    pub exchange: String,
    pub type_disp: String,
}

/// View mode for the Data panel (sector list vs ticker list).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DataViewMode {
    /// Viewing the list of sectors
    #[default]
    Sectors,
    /// Viewing tickers within the selected sector
    Tickers,
}

/// Data panel state
#[derive(Debug)]
pub struct DataState {
    pub symbols: Vec<String>,
    pub selected_index: usize,
    pub bar_count: usize,
    pub date_range: Option<(String, String)>,
    pub bars_cache: HashMap<String, Vec<Bar>>,
    // Search mode state
    pub search_mode: bool,
    pub search_input: String,
    pub search_suggestions: Vec<SearchSuggestion>,
    pub search_selected: usize,
    pub search_loading: bool,
    // Universe/sector state
    pub universe: Universe,
    pub view_mode: DataViewMode,
    pub selected_sector_index: usize,
    pub selected_ticker_index: usize,
    pub selected_tickers: HashSet<String>,
    // Scroll offsets for viewport management
    pub sector_scroll_offset: usize,
    pub ticker_scroll_offset: usize,
}

impl Default for DataState {
    fn default() -> Self {
        Self {
            symbols: Vec::new(),
            selected_index: 0,
            bar_count: 0,
            date_range: None,
            bars_cache: HashMap::new(),
            search_mode: false,
            search_input: String::new(),
            search_suggestions: Vec::new(),
            search_selected: 0,
            search_loading: false,
            universe: Universe::default_universe(),
            view_mode: DataViewMode::Sectors,
            selected_sector_index: 0,
            selected_ticker_index: 0,
            selected_tickers: HashSet::new(),
            sector_scroll_offset: 0,
            ticker_scroll_offset: 0,
        }
    }
}

impl DataState {
    /// Get the currently selected symbol
    pub fn selected_symbol(&self) -> Option<&String> {
        self.symbols.get(self.selected_index)
    }

    /// Get bars for the selected symbol
    pub fn selected_bars(&self) -> Option<&Vec<Bar>> {
        self.selected_symbol().and_then(|s| self.bars_cache.get(s))
    }

    /// Get the currently selected sector.
    pub fn selected_sector(&self) -> Option<&Sector> {
        self.universe
            .get_sector_by_index(self.selected_sector_index)
    }

    /// Get tickers in the currently selected sector.
    pub fn current_sector_tickers(&self) -> &[String] {
        self.selected_sector()
            .map(|s| s.tickers.as_slice())
            .unwrap_or(&[])
    }

    /// Get the currently focused ticker (in ticker view mode).
    pub fn focused_ticker(&self) -> Option<&String> {
        self.current_sector_tickers()
            .get(self.selected_ticker_index)
    }

    /// Check if a ticker is selected for multi-ticker sweep.
    pub fn is_ticker_selected(&self, ticker: &str) -> bool {
        self.selected_tickers.contains(ticker)
    }

    /// Toggle ticker selection for multi-ticker sweep.
    pub fn toggle_ticker_selection(&mut self, ticker: &str) {
        if self.selected_tickers.contains(ticker) {
            self.selected_tickers.remove(ticker);
        } else {
            self.selected_tickers.insert(ticker.to_string());
        }
    }

    /// Select all tickers in the current sector.
    pub fn select_all_in_sector(&mut self) {
        // Collect tickers first to avoid borrow conflict
        let tickers: Vec<String> = self
            .selected_sector()
            .map(|s| s.tickers.clone())
            .unwrap_or_default();

        for ticker in tickers {
            self.selected_tickers.insert(ticker);
        }
    }

    /// Deselect all tickers in the current sector.
    pub fn deselect_all_in_sector(&mut self) {
        // Collect tickers first to avoid borrow conflict
        let tickers: Vec<String> = self
            .selected_sector()
            .map(|s| s.tickers.clone())
            .unwrap_or_default();

        for ticker in &tickers {
            self.selected_tickers.remove(ticker);
        }
    }

    /// Select all tickers across all sectors (for YOLO mode).
    pub fn select_all(&mut self) {
        for sector in &self.universe.sectors {
            for ticker in &sector.tickers {
                self.selected_tickers.insert(ticker.clone());
            }
        }
    }

    /// Deselect all tickers globally.
    pub fn deselect_all(&mut self) {
        self.selected_tickers.clear();
    }

    /// Get count of selected tickers in a sector by sector ID.
    pub fn selected_count_in_sector(&self, sector_id: &str) -> usize {
        self.universe
            .get_sector(sector_id)
            .map(|sector| {
                sector
                    .tickers
                    .iter()
                    .filter(|t| self.selected_tickers.contains(*t))
                    .count()
            })
            .unwrap_or(0)
    }

    /// Get all selected tickers as a sorted vector.
    pub fn selected_tickers_sorted(&self) -> Vec<String> {
        let mut tickers: Vec<String> = self.selected_tickers.iter().cloned().collect();
        tickers.sort();
        tickers
    }

    /// Load universe from config file, falling back to default.
    pub fn load_universe_from_config(&mut self) {
        let config_path = std::path::Path::new("configs/universe.toml");
        if config_path.exists() {
            match Universe::load(config_path) {
                Ok(universe) => {
                    self.universe = universe;
                }
                Err(e) => {
                    eprintln!("Failed to load universe config: {}", e);
                    // Keep default universe
                }
            }
        }
        // Otherwise keep the default universe
    }

    /// Ensure the sector selection is visible within the viewport.
    /// Returns the adjusted scroll offset.
    pub fn ensure_sector_visible(&mut self, visible_height: usize) {
        if visible_height == 0 {
            return;
        }
        // If selection is above viewport, scroll up
        if self.selected_sector_index < self.sector_scroll_offset {
            self.sector_scroll_offset = self.selected_sector_index;
        }
        // If selection is below viewport, scroll down
        else if self.selected_sector_index >= self.sector_scroll_offset + visible_height {
            self.sector_scroll_offset = self
                .selected_sector_index
                .saturating_sub(visible_height - 1);
        }
    }

    /// Ensure the ticker selection is visible within the viewport.
    pub fn ensure_ticker_visible(&mut self, visible_height: usize) {
        if visible_height == 0 {
            return;
        }
        // If selection is above viewport, scroll up
        if self.selected_ticker_index < self.ticker_scroll_offset {
            self.ticker_scroll_offset = self.selected_ticker_index;
        }
        // If selection is below viewport, scroll down
        else if self.selected_ticker_index >= self.ticker_scroll_offset + visible_height {
            self.ticker_scroll_offset = self
                .selected_ticker_index
                .saturating_sub(visible_height - 1);
        }
    }

    /// Reset ticker scroll when switching sectors.
    pub fn reset_ticker_scroll(&mut self) {
        self.ticker_scroll_offset = 0;
    }
}
