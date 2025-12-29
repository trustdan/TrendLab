//! Strategy types, categories, and configuration structs.

use std::collections::HashSet;

/// Strategy category for grouped checkboxes in the TUI.
/// Each category groups related strategy types for easier navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StrategyCategory {
    /// Channel-based breakout strategies (Donchian, Keltner, STARC, Bollinger, Supertrend)
    ChannelBreakouts,
    /// Momentum and direction strategies (MA Cross, TSMOM, DMI/ADX, Aroon, Heikin-Ashi)
    MomentumDirection,
    /// Price structure breakouts (Darvas, 52-Week High, Larry Williams, ORB)
    PriceBreakouts,
    /// Classic preset strategies (Turtle S1, Turtle S2, Parabolic SAR)
    ClassicPresets,
}

impl StrategyCategory {
    /// All categories in display order.
    pub fn all() -> &'static [StrategyCategory] {
        &[
            StrategyCategory::ChannelBreakouts,
            StrategyCategory::MomentumDirection,
            StrategyCategory::PriceBreakouts,
            StrategyCategory::ClassicPresets,
        ]
    }

    /// Display name for the category.
    pub fn name(&self) -> &'static str {
        match self {
            StrategyCategory::ChannelBreakouts => "Channel Breakouts",
            StrategyCategory::MomentumDirection => "Momentum/Direction",
            StrategyCategory::PriceBreakouts => "Price Breakouts",
            StrategyCategory::ClassicPresets => "Classic Presets",
        }
    }

    /// Get all strategy types in this category.
    pub fn strategies(&self) -> &'static [StrategyType] {
        match self {
            StrategyCategory::ChannelBreakouts => &[
                StrategyType::Donchian,
                StrategyType::Keltner,
                StrategyType::STARC,
                StrategyType::Supertrend,
            ],
            StrategyCategory::MomentumDirection => {
                &[StrategyType::MACrossover, StrategyType::Tsmom]
            }
            StrategyCategory::PriceBreakouts => &[StrategyType::OpeningRange],
            StrategyCategory::ClassicPresets => &[
                StrategyType::TurtleS1,
                StrategyType::TurtleS2,
                StrategyType::ParabolicSar,
            ],
        }
    }

    /// Check if category has any strategies (for display purposes).
    pub fn is_empty(&self) -> bool {
        self.strategies().is_empty()
    }

    /// Count of strategies in this category.
    pub fn len(&self) -> usize {
        self.strategies().len()
    }
}

/// Voting method for ensemble strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VotingMethod {
    /// Simple majority: > 50% agreement takes that signal
    #[default]
    Majority,
    /// Longer horizons weighted more heavily
    WeightedByHorizon,
    /// Only enter if ALL horizons agree; exit on ANY exit signal
    UnanimousEntry,
}

impl VotingMethod {
    /// All voting methods.
    pub fn all() -> &'static [VotingMethod] {
        &[
            VotingMethod::Majority,
            VotingMethod::WeightedByHorizon,
            VotingMethod::UnanimousEntry,
        ]
    }

    /// Display name.
    pub fn name(&self) -> &'static str {
        match self {
            VotingMethod::Majority => "Majority",
            VotingMethod::WeightedByHorizon => "Weighted by Horizon",
            VotingMethod::UnanimousEntry => "Unanimous Entry",
        }
    }
}

/// Ensemble configuration for multi-horizon voting.
#[derive(Debug, Clone)]
pub struct EnsembleConfig {
    /// Whether ensemble mode is enabled
    pub enabled: bool,
    /// Base strategy type for the ensemble
    pub base_strategy: StrategyType,
    /// Lookback horizons to use (e.g., [10, 20, 55] for Donchian)
    pub horizons: Vec<usize>,
    /// Voting method for combining signals
    pub voting: VotingMethod,
}

impl Default for EnsembleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_strategy: StrategyType::Donchian,
            horizons: vec![10, 20, 55],
            voting: VotingMethod::Majority,
        }
    }
}

/// Strategy type selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum StrategyType {
    #[default]
    Donchian,
    TurtleS1,
    TurtleS2,
    MACrossover,
    Tsmom,
    // Phase 1: ATR-Based Channels
    Keltner,
    STARC,
    Supertrend,
    // Phase 4: Complex Stateful + Ensemble
    ParabolicSar,
    OpeningRange,
}

impl StrategyType {
    pub fn all() -> &'static [StrategyType] {
        &[
            StrategyType::Donchian,
            StrategyType::TurtleS1,
            StrategyType::TurtleS2,
            StrategyType::MACrossover,
            StrategyType::Tsmom,
            StrategyType::Keltner,
            StrategyType::STARC,
            StrategyType::Supertrend,
            StrategyType::ParabolicSar,
            StrategyType::OpeningRange,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            StrategyType::Donchian => "Donchian Breakout",
            StrategyType::TurtleS1 => "Turtle System 1",
            StrategyType::TurtleS2 => "Turtle System 2",
            StrategyType::MACrossover => "MA Crossover",
            StrategyType::Tsmom => "TSMOM",
            StrategyType::Keltner => "Keltner Channel",
            StrategyType::STARC => "STARC Bands",
            StrategyType::Supertrend => "Supertrend",
            StrategyType::ParabolicSar => "Parabolic SAR",
            StrategyType::OpeningRange => "Opening Range Breakout",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            StrategyType::Donchian => "Breakout above N-day high, exit below M-day low",
            StrategyType::TurtleS1 => "Classic Turtle: 20-day entry, 10-day exit",
            StrategyType::TurtleS2 => "Turtle variant: 55-day entry, 20-day exit",
            StrategyType::MACrossover => "Enter on golden cross, exit on death cross",
            StrategyType::Tsmom => "Time-series momentum: long when return > 0",
            StrategyType::Keltner => "Breakout above EMA + k*ATR channel",
            StrategyType::STARC => "Breakout above SMA + k*ATR bands",
            StrategyType::Supertrend => "Follow ATR-based trailing trend line",
            StrategyType::ParabolicSar => "Wilder's SAR: trailing stop with acceleration",
            StrategyType::OpeningRange => "Entry on breakout above opening range high",
        }
    }

    /// Number of configurable parameters for this strategy
    pub fn param_count(&self) -> usize {
        match self {
            StrategyType::Donchian => 2,
            StrategyType::TurtleS1 => 0, // Fixed params
            StrategyType::TurtleS2 => 0, // Fixed params
            StrategyType::MACrossover => 3,
            StrategyType::Tsmom => 1,
            StrategyType::Keltner => 3,    // ema_period, atr_period, multiplier
            StrategyType::STARC => 3,      // sma_period, atr_period, multiplier
            StrategyType::Supertrend => 2, // atr_period, multiplier
            StrategyType::ParabolicSar => 3, // af_start, af_step, af_max
            StrategyType::OpeningRange => 2, // range_bars, period
        }
    }

    /// Get the category this strategy belongs to.
    pub fn category(&self) -> StrategyCategory {
        match self {
            StrategyType::Donchian
            | StrategyType::Keltner
            | StrategyType::STARC
            | StrategyType::Supertrend => StrategyCategory::ChannelBreakouts,
            StrategyType::MACrossover | StrategyType::Tsmom => StrategyCategory::MomentumDirection,
            StrategyType::OpeningRange => StrategyCategory::PriceBreakouts,
            StrategyType::TurtleS1 | StrategyType::TurtleS2 | StrategyType::ParabolicSar => {
                StrategyCategory::ClassicPresets
            }
        }
    }
}

/// Donchian strategy configuration
#[derive(Debug, Clone)]
pub struct DonchianConfig {
    pub entry_lookback: usize,
    pub exit_lookback: usize,
}

impl Default for DonchianConfig {
    fn default() -> Self {
        Self {
            entry_lookback: 20,
            exit_lookback: 10,
        }
    }
}

/// MA Crossover strategy configuration
#[derive(Debug, Clone)]
pub struct MACrossoverConfig {
    pub fast_period: usize,
    pub slow_period: usize,
    pub ma_type: usize, // 0 = SMA, 1 = EMA
}

impl Default for MACrossoverConfig {
    fn default() -> Self {
        Self {
            fast_period: 10,
            slow_period: 50,
            ma_type: 0, // SMA
        }
    }
}

impl MACrossoverConfig {
    pub fn ma_type_name(&self) -> &'static str {
        if self.ma_type == 0 {
            "SMA"
        } else {
            "EMA"
        }
    }

    // Convenience alias used by some UI helpers.
    pub fn ma_type_label(&self) -> &'static str {
        self.ma_type_name()
    }
}

/// TSMOM strategy configuration
#[derive(Debug, Clone)]
pub struct TsmomConfig {
    pub lookback: usize,
}

impl Default for TsmomConfig {
    fn default() -> Self {
        Self { lookback: 252 }
    }
}

/// Keltner Channel strategy configuration
#[derive(Debug, Clone)]
pub struct KeltnerConfig {
    pub ema_period: usize,
    pub atr_period: usize,
    pub multiplier: f64,
}

impl Default for KeltnerConfig {
    fn default() -> Self {
        Self {
            ema_period: 20,
            atr_period: 10,
            multiplier: 2.0,
        }
    }
}

/// STARC Bands strategy configuration
#[derive(Debug, Clone)]
pub struct STARCConfig {
    pub sma_period: usize,
    pub atr_period: usize,
    pub multiplier: f64,
}

impl Default for STARCConfig {
    fn default() -> Self {
        Self {
            sma_period: 20,
            atr_period: 15,
            multiplier: 2.0,
        }
    }
}

/// Supertrend strategy configuration
#[derive(Debug, Clone)]
pub struct SupertrendConfig {
    pub atr_period: usize,
    pub multiplier: f64,
}

impl Default for SupertrendConfig {
    fn default() -> Self {
        Self {
            atr_period: 10,
            multiplier: 3.0,
        }
    }
}

/// Parabolic SAR strategy configuration
#[derive(Debug, Clone)]
pub struct ParabolicSarConfig {
    pub af_start: f64,
    pub af_step: f64,
    pub af_max: f64,
}

impl Default for ParabolicSarConfig {
    fn default() -> Self {
        Self {
            af_start: 0.02,
            af_step: 0.02,
            af_max: 0.20,
        }
    }
}

/// Opening Range Breakout strategy configuration
#[derive(Debug, Clone)]
pub struct OpeningRangeConfig {
    pub range_bars: usize,
    pub period: usize, // 0 = Weekly, 1 = Monthly, 2 = Rolling
}

impl Default for OpeningRangeConfig {
    fn default() -> Self {
        Self {
            range_bars: 5,
            period: 0, // Weekly
        }
    }
}

impl OpeningRangeConfig {
    pub fn period_name(&self) -> &'static str {
        match self.period {
            0 => "Weekly",
            1 => "Monthly",
            _ => "Rolling",
        }
    }

    // Convenience alias used by some UI helpers.
    pub fn timeframe_label(&self) -> &'static str {
        self.period_name()
    }
}

/// Focus mode within the strategy panel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrategyFocus {
    /// Navigating categories and strategy checkboxes (left panel)
    #[default]
    Selection,
    /// Editing parameters for a specific strategy (right panel)
    Parameters,
}

/// Strategy panel state with grouped checkbox support
#[derive(Debug)]
pub struct StrategyState {
    // === Grouped checkbox state ===
    /// Categories that are currently expanded (show their strategies)
    pub expanded_categories: HashSet<StrategyCategory>,
    /// Strategies that are currently selected (checked)
    pub selected_strategies: HashSet<StrategyType>,
    /// Currently focused category index
    pub focused_category_index: usize,
    /// Currently focused strategy index within the focused category
    pub focused_strategy_in_category: usize,
    /// Whether we're focused on a category header or a strategy within it
    pub focus_on_category: bool,

    // === Ensemble configuration ===
    pub ensemble: EnsembleConfig,

    // === Legacy fields for parameter editing ===
    pub selected_type: StrategyType,
    pub selected_type_index: usize,
    pub donchian_config: DonchianConfig,
    pub ma_config: MACrossoverConfig,
    pub tsmom_config: TsmomConfig,
    pub keltner_config: KeltnerConfig,
    pub starc_config: STARCConfig,
    pub supertrend_config: SupertrendConfig,
    pub parabolic_sar_config: ParabolicSarConfig,
    pub opening_range_config: OpeningRangeConfig,
    pub selected_field: usize,
    /// Current focus mode: Selection (left) or Parameters (right)
    pub focus: StrategyFocus,

    // Legacy alias for backwards compatibility
    pub editing_strategy: bool,
}

impl Default for StrategyState {
    fn default() -> Self {
        // Start with non-empty categories expanded
        let mut expanded = HashSet::new();
        for cat in StrategyCategory::all() {
            if !cat.is_empty() {
                expanded.insert(*cat);
            }
        }

        // Start with Donchian selected by default
        let mut selected = HashSet::new();
        selected.insert(StrategyType::Donchian);

        Self {
            expanded_categories: expanded,
            selected_strategies: selected,
            focused_category_index: 0,
            focused_strategy_in_category: 0,
            focus_on_category: true,
            ensemble: EnsembleConfig::default(),
            selected_type: StrategyType::default(),
            selected_type_index: 0,
            donchian_config: DonchianConfig::default(),
            ma_config: MACrossoverConfig::default(),
            tsmom_config: TsmomConfig::default(),
            keltner_config: KeltnerConfig::default(),
            starc_config: STARCConfig::default(),
            supertrend_config: SupertrendConfig::default(),
            parabolic_sar_config: ParabolicSarConfig::default(),
            opening_range_config: OpeningRangeConfig::default(),
            selected_field: 0,
            focus: StrategyFocus::Selection,
            editing_strategy: true, // Legacy: true = selection mode
        }
    }
}

impl StrategyState {
    /// Get the currently focused category.
    pub fn focused_category(&self) -> Option<StrategyCategory> {
        StrategyCategory::all()
            .get(self.focused_category_index)
            .copied()
    }

    /// Check if a category is expanded.
    pub fn is_expanded(&self, cat: StrategyCategory) -> bool {
        self.expanded_categories.contains(&cat)
    }

    /// Toggle category expansion.
    pub fn toggle_category(&mut self, cat: StrategyCategory) {
        if self.expanded_categories.contains(&cat) {
            self.expanded_categories.remove(&cat);
        } else {
            self.expanded_categories.insert(cat);
        }
    }

    /// Check if a strategy is selected.
    pub fn is_selected(&self, strat: StrategyType) -> bool {
        self.selected_strategies.contains(&strat)
    }

    /// Toggle strategy selection.
    pub fn toggle_strategy(&mut self, strat: StrategyType) {
        if self.selected_strategies.contains(&strat) {
            self.selected_strategies.remove(&strat);
        } else {
            self.selected_strategies.insert(strat);
        }
    }

    /// Select all strategies in a category.
    pub fn select_all_in_category(&mut self, cat: StrategyCategory) {
        for strat in cat.strategies() {
            self.selected_strategies.insert(*strat);
        }
    }

    /// Deselect all strategies in a category.
    pub fn deselect_all_in_category(&mut self, cat: StrategyCategory) {
        for strat in cat.strategies() {
            self.selected_strategies.remove(strat);
        }
    }

    /// Count selected strategies in a category.
    pub fn selected_count_in_category(&self, cat: StrategyCategory) -> usize {
        cat.strategies()
            .iter()
            .filter(|s| self.selected_strategies.contains(s))
            .count()
    }

    /// Get the currently focused strategy (if focus is on a strategy, not category header).
    pub fn focused_strategy(&self) -> Option<StrategyType> {
        if self.focus_on_category {
            return None;
        }
        let cat = self.focused_category()?;
        cat.strategies()
            .get(self.focused_strategy_in_category)
            .copied()
    }

    /// Get all selected strategies as a sorted vector.
    pub fn selected_strategies_sorted(&self) -> Vec<StrategyType> {
        let mut strats: Vec<StrategyType> = self.selected_strategies.iter().copied().collect();
        strats.sort_by_key(|s| s.name());
        strats
    }

    /// Format the current strategy config as a display string for Pine export
    pub fn config_display_string(&self) -> String {
        match self.selected_type {
            StrategyType::Donchian => {
                format!(
                    "Entry={}, Exit={}",
                    self.donchian_config.entry_lookback, self.donchian_config.exit_lookback
                )
            }
            StrategyType::MACrossover => {
                format!(
                    "Fast={}, Slow={}, Type={}",
                    self.ma_config.fast_period,
                    self.ma_config.slow_period,
                    self.ma_config.ma_type_name()
                )
            }
            StrategyType::Tsmom => {
                format!("Lookback={}", self.tsmom_config.lookback)
            }
            StrategyType::Keltner => {
                format!(
                    "EMA={}, ATR={}, Mult={:.1}",
                    self.keltner_config.ema_period,
                    self.keltner_config.atr_period,
                    self.keltner_config.multiplier
                )
            }
            StrategyType::STARC => {
                format!(
                    "SMA={}, ATR={}, Mult={:.1}",
                    self.starc_config.sma_period,
                    self.starc_config.atr_period,
                    self.starc_config.multiplier
                )
            }
            StrategyType::Supertrend => {
                format!(
                    "ATR={}, Mult={:.1}",
                    self.supertrend_config.atr_period, self.supertrend_config.multiplier
                )
            }
            StrategyType::ParabolicSar => {
                format!(
                    "Start={:.2}, Step={:.2}, Max={:.2}",
                    self.parabolic_sar_config.af_start,
                    self.parabolic_sar_config.af_step,
                    self.parabolic_sar_config.af_max
                )
            }
            StrategyType::OpeningRange => {
                format!(
                    "Bars={}, Period={}",
                    self.opening_range_config.range_bars,
                    self.opening_range_config.period_name()
                )
            }
            StrategyType::TurtleS1 => "System 1 (20/10)".to_string(),
            StrategyType::TurtleS2 => "System 2 (55/20)".to_string(),
        }
    }
}
