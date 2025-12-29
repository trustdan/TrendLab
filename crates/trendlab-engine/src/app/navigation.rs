//! Panel navigation, startup mode, and UI state enums.

use trendlab_core::SweepDepth;

use super::strategies::StrategyType;

/// Startup flow mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupMode {
    Manual,
    FullAuto,
}

impl StartupMode {
    #[allow(dead_code)]
    pub fn name(&self) -> &'static str {
        match self {
            StartupMode::Manual => "Manual",
            StartupMode::FullAuto => "Full-Auto",
        }
    }
}

/// Strategy selection for Full-Auto mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrategySelection {
    /// Run all strategies and compare
    #[default]
    AllStrategies,
    /// Run a single strategy type
    Single(StrategyType),
}

impl StrategySelection {
    /// Get all options for the startup modal
    pub fn all_options() -> Vec<StrategySelection> {
        let mut options = vec![StrategySelection::AllStrategies];
        for st in StrategyType::all() {
            options.push(StrategySelection::Single(*st));
        }
        options
    }

    pub fn name(&self) -> &'static str {
        match self {
            StrategySelection::AllStrategies => "All Strategies",
            StrategySelection::Single(st) => st.name(),
        }
    }
}

/// Startup modal state (shown on app launch).
#[derive(Debug, Clone)]
pub struct StartupState {
    pub active: bool,
    pub mode: StartupMode,
    pub selected_strategy_index: usize,
    /// Strategy selection for Full-Auto mode (All Strategies = index 0)
    pub strategy_selection: StrategySelection,
    /// Sweep depth for parameter range coverage
    pub sweep_depth: SweepDepth,
}

impl Default for StartupState {
    fn default() -> Self {
        Self {
            active: true,
            mode: StartupMode::Manual,
            selected_strategy_index: 0,
            strategy_selection: StrategySelection::AllStrategies,
            sweep_depth: SweepDepth::Standard,
        }
    }
}

/// Full-auto pipeline stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutoStage {
    #[default]
    Idle,
    LoadingCache,
    FetchingMissing,
    Sweeping,
}

/// Full-auto run state.
#[derive(Debug, Clone, Default)]
pub struct AutoRunState {
    pub enabled: bool,
    pub stage: AutoStage,
    pub desired_symbols: Vec<String>,
    pub pending_missing: Vec<String>,
    pub jump_to_chart_on_complete: bool,
}

/// Panel identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Data,
    Strategy,
    Sweep,
    Results,
    Chart,
    Help,
}

impl Panel {
    pub fn all() -> &'static [Panel] {
        &[
            Panel::Data,
            Panel::Strategy,
            Panel::Sweep,
            Panel::Results,
            Panel::Chart,
            Panel::Help,
        ]
    }

    pub fn title(&self) -> &'static str {
        match self {
            Panel::Data => "Data",
            Panel::Strategy => "Strategy",
            Panel::Sweep => "Sweep",
            Panel::Results => "Results",
            Panel::Chart => "Chart",
            Panel::Help => "Help",
        }
    }

    pub fn hotkey(&self) -> char {
        match self {
            Panel::Data => '1',
            Panel::Strategy => '2',
            Panel::Sweep => '3',
            Panel::Results => '4',
            Panel::Chart => '5',
            Panel::Help => '6',
        }
    }
}

/// Help panel section identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HelpSection {
    #[default]
    Global,
    Data,
    Strategy,
    Sweep,
    Results,
    Chart,
    Features,
}

impl HelpSection {
    pub fn all() -> &'static [HelpSection] {
        &[
            HelpSection::Global,
            HelpSection::Data,
            HelpSection::Strategy,
            HelpSection::Sweep,
            HelpSection::Results,
            HelpSection::Chart,
            HelpSection::Features,
        ]
    }

    pub fn title(&self) -> &'static str {
        match self {
            HelpSection::Global => "Global",
            HelpSection::Data => "Data",
            HelpSection::Strategy => "Strategy",
            HelpSection::Sweep => "Sweep",
            HelpSection::Results => "Results",
            HelpSection::Chart => "Chart",
            HelpSection::Features => "Features",
        }
    }

    /// Map a Panel to its corresponding HelpSection
    pub fn from_panel(panel: Panel) -> Self {
        match panel {
            Panel::Data => HelpSection::Data,
            Panel::Strategy => HelpSection::Strategy,
            Panel::Sweep => HelpSection::Sweep,
            Panel::Results => HelpSection::Results,
            Panel::Chart => HelpSection::Chart,
            Panel::Help => HelpSection::Global,
        }
    }

    pub fn next(&self) -> Self {
        let all = Self::all();
        let idx = all.iter().position(|s| s == self).unwrap_or(0);
        all[(idx + 1) % all.len()]
    }

    pub fn prev(&self) -> Self {
        let all = Self::all();
        let idx = all.iter().position(|s| s == self).unwrap_or(0);
        if idx == 0 {
            all[all.len() - 1]
        } else {
            all[idx - 1]
        }
    }
}

/// Help panel state
#[derive(Debug, Clone)]
pub struct HelpState {
    pub active_section: HelpSection,
    pub scroll_offset: usize,
    pub max_scroll: usize,
    pub previous_panel: Panel,
    pub search_mode: bool,
    pub search_query: String,
    pub search_matches: Vec<usize>,
    pub search_index: usize,
}

impl Default for HelpState {
    fn default() -> Self {
        Self {
            active_section: HelpSection::Global,
            scroll_offset: 0,
            max_scroll: 0,
            previous_panel: Panel::Data,
            search_mode: false,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_index: 0,
        }
    }
}

/// Current operation state
#[derive(Debug, Clone, Default)]
pub enum OperationState {
    #[default]
    Idle,
    FetchingData {
        current_symbol: String,
        completed: usize,
        total: usize,
    },
    RunningSweep {
        completed: usize,
        total: usize,
    },
}

/// Type of status message for appropriate color styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MessageType {
    /// Informational message (cyan/default)
    #[default]
    Info,
    /// Success message (green)
    Success,
    /// Warning message (yellow)
    Warning,
    /// Error message (red)
    Error,
}
