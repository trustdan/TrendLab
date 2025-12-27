@artifact @pine
Feature: StrategyArtifact export for Pine Script parity
  As a researcher, I want to export backtest configurations as structured artifacts
  so I can generate Pine Scripts that replicate the exact same signals.

  Background:
    Given a synthetic price series with 50 bars
    And a Donchian breakout strategy with entry lookback 10 and exit lookback 5

  @artifact @schema
  Scenario: Artifact includes strategy identification
    Given a completed backtest run
    When I export a StrategyArtifact
    Then the artifact must include strategy_id "donchian_breakout"
    And the artifact must include a schema_version
    And the artifact must include the symbol and timeframe

  @artifact @indicators
  Scenario: Artifact includes indicator definitions
    Given a completed backtest run
    When I export a StrategyArtifact
    Then the artifact must include indicator definitions
    And the indicators must include "donchian_entry" with lookback 10
    And the indicators must include "donchian_exit" with lookback 5

  @artifact @rules
  Scenario: Artifact includes entry and exit rules in Pine-friendly DSL
    Given a completed backtest run
    When I export a StrategyArtifact
    Then the artifact must include entry_rule
    And the entry_rule must be expressible as Pine condition "close > donchian_entry.upper"
    And the artifact must include exit_rule
    And the exit_rule must be expressible as Pine condition "close < donchian_exit.lower"

  @artifact @costs
  Scenario: Artifact includes fill model and cost configuration
    Given a completed backtest with fees 10 bps and slippage 5 bps
    When I export a StrategyArtifact
    Then the artifact must include fill_model "NextOpen"
    And the artifact must include cost_model with fees_bps 10
    And the artifact must include cost_model with slippage_bps 5

  @artifact @vectors
  Scenario: Artifact includes parity test vectors
    Given a completed backtest with at least one trade
    When I export a StrategyArtifact
    Then the artifact must include parity_vectors
    And the vectors must include timestamps
    And the vectors must include indicator values at each timestamp
    And the vectors must include expected signals (entry/exit)

  @artifact @vectors @roundtrip
  Scenario: Parity vectors match actual backtest signals
    Given a completed backtest with known signals
    When I export a StrategyArtifact
    And I compare the parity vectors to the actual signals
    Then all signal timestamps must match exactly

  @artifact @json
  Scenario: Artifact serializes to valid JSON
    Given a completed backtest run
    When I export a StrategyArtifact
    And I serialize it to JSON
    Then the JSON must be valid
    And it must roundtrip without data loss

  @artifact @cli
  Scenario: CLI artifact export command produces valid artifact
    Given a completed sweep run with run_id "test_artifact_run"
    And a configuration with config_id "entry_10_exit_5"
    When I run "trendlab artifact export --run-id test_artifact_run --config-id entry_10_exit_5"
    Then the command should succeed
    And the artifact output file should exist at "artifacts/test_artifact_run/entry_10_exit_5.json"
    And the output should be a valid StrategyArtifact
