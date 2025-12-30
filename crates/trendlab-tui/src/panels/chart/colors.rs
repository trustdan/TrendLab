//! Color definitions for chart curves and strategies

use ratatui::style::Color;
use trendlab_core::StrategyTypeId;

/// Color palette for multi-ticker curves
pub const CURVE_COLORS: &[Color] = &[
    Color::Rgb(46, 204, 113),  // Emerald green
    Color::Rgb(52, 152, 219),  // Blue
    Color::Rgb(155, 89, 182),  // Purple
    Color::Rgb(241, 196, 15),  // Yellow
    Color::Rgb(231, 76, 60),   // Red
    Color::Rgb(26, 188, 156),  // Turquoise
    Color::Rgb(230, 126, 34),  // Orange
    Color::Rgb(236, 240, 241), // Light gray
    Color::Rgb(149, 165, 166), // Gray
    Color::Rgb(46, 134, 193),  // Steel blue
    Color::Rgb(175, 122, 197), // Amethyst
    Color::Rgb(244, 208, 63),  // Sunflower
];

/// Fixed color mapping for strategy types
pub fn strategy_color(strategy_type: StrategyTypeId) -> Color {
    match strategy_type {
        StrategyTypeId::Donchian => Color::Rgb(46, 204, 113), // Green
        StrategyTypeId::TurtleS1 => Color::Rgb(52, 152, 219), // Blue
        StrategyTypeId::TurtleS2 => Color::Rgb(155, 89, 182), // Purple
        StrategyTypeId::MACrossover => Color::Rgb(241, 196, 15), // Yellow
        StrategyTypeId::Tsmom => Color::Rgb(231, 76, 60),     // Red
        StrategyTypeId::Keltner => Color::Rgb(230, 126, 34),  // Orange
        StrategyTypeId::STARC => Color::Rgb(26, 188, 156),    // Teal
        StrategyTypeId::Supertrend => Color::Rgb(142, 68, 173), // Dark Purple
        StrategyTypeId::DmiAdx => Color::Rgb(22, 160, 133),   // Dark Teal
        StrategyTypeId::Aroon => Color::Rgb(39, 174, 96),     // Dark Green
        StrategyTypeId::BollingerSqueeze => Color::Rgb(41, 128, 185), // Dark Blue
        StrategyTypeId::FiftyTwoWeekHigh => Color::Rgb(192, 57, 43), // Dark Red
        StrategyTypeId::DarvasBox => Color::Rgb(243, 156, 18), // Orange-Yellow
        StrategyTypeId::LarryWilliams => Color::Rgb(211, 84, 0), // Burnt Orange
        StrategyTypeId::HeikinAshi => Color::Rgb(127, 140, 141), // Gray
        StrategyTypeId::ParabolicSar => Color::Rgb(44, 62, 80), // Dark Gray
        StrategyTypeId::OpeningRangeBreakout => Color::Rgb(189, 195, 199), // Light Gray
        StrategyTypeId::Ensemble => Color::Rgb(149, 165, 166), // Silver
        // Phase 5 oscillator strategies
        _ => Color::Rgb(100, 100, 100), // Default gray for new strategy types
    }
}
