//! Sweep panel state.

use trendlab_core::SweepGrid;

/// Sweep panel state
#[derive(Debug, Default)]
pub struct SweepState {
    pub is_running: bool,
    pub progress: f64,
    pub total_configs: usize,
    pub completed_configs: usize,
    pub selected_param: usize,
    pub param_ranges: Vec<(String, Vec<String>)>,
}

impl SweepState {
    /// Generate a SweepGrid from current param ranges
    /// Returns Ok(grid) or Err(message) if parameter parsing fails
    pub fn to_sweep_grid(&self) -> Result<SweepGrid, String> {
        let entry_result = self
            .param_ranges
            .iter()
            .find(|(name, _)| name == "entry_lookback");

        let exit_result = self
            .param_ranges
            .iter()
            .find(|(name, _)| name == "exit_lookback");

        // Parse entry lookbacks with error tracking
        let entry_lookbacks: Vec<usize> = if let Some((_, values)) = entry_result {
            let parsed: Vec<usize> = values.iter().filter_map(|v| v.parse().ok()).collect();
            if parsed.is_empty() && !values.is_empty() {
                return Err(format!(
                    "Invalid entry_lookback values: {:?}. Use comma-separated integers like '10,20,30'",
                    values
                ));
            }
            if parsed.is_empty() {
                vec![10, 20, 30, 40, 50] // Default if no values provided
            } else {
                parsed
            }
        } else {
            vec![10, 20, 30, 40, 50]
        };

        // Parse exit lookbacks with error tracking
        let exit_lookbacks: Vec<usize> = if let Some((_, values)) = exit_result {
            let parsed: Vec<usize> = values.iter().filter_map(|v| v.parse().ok()).collect();
            if parsed.is_empty() && !values.is_empty() {
                return Err(format!(
                    "Invalid exit_lookback values: {:?}. Use comma-separated integers like '5,10,15'",
                    values
                ));
            }
            if parsed.is_empty() {
                vec![5, 10, 15, 20, 25] // Default if no values provided
            } else {
                parsed
            }
        } else {
            vec![5, 10, 15, 20, 25]
        };

        Ok(SweepGrid::new(entry_lookbacks, exit_lookbacks))
    }
}
