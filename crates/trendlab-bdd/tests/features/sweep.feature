@sweep
Feature: Parameter sweep infrastructure
  As a researcher, I want to systematically explore parameter space
  and identify robust configurations with proper reproducibility.

  Background:
    Given a synthetic price series with 100 bars

  @sweep @grid
  Scenario: Sweep executes all parameter combinations
    Given a sweep grid with entry_lookback [10, 20] and exit_lookback [5, 10]
    When I run the parameter sweep
    Then the sweep should execute 4 configurations
    And each configuration should produce a BacktestResult

  @sweep @parallel
  Scenario: Sweep results are deterministic regardless of execution order
    Given a sweep grid with entry_lookback [10, 20, 30] and exit_lookback [5, 10, 15]
    When I run the parameter sweep twice
    Then the results for each configuration should be identical

  @sweep @manifest
  Scenario: Every sweep produces a reproducible run manifest
    Given a completed sweep run
    When I examine the run manifest
    Then it should include the sweep_id
    And it should include the sweep_config with parameter grid
    And it should include the data_version hash
    And it should include timestamps for start and end
    And it should include result file paths

  @sweep @ranking
  Scenario: Ranking engine returns top-N configurations by metric
    Given a completed sweep with 9 configurations
    When I rank by sharpe descending and request top 3
    Then I should receive exactly 3 configurations
    And they should be ordered by sharpe descending

  @sweep @ranking
  Scenario: Ranking supports multiple metrics
    Given a completed sweep with various metrics
    When I rank by cagr descending
    Then the top config should have the highest cagr
    When I rank by max_drawdown ascending
    Then the top config should have the lowest max_drawdown

  @sweep @stability
  Scenario: Stability score penalizes performance outliers
    Given a parameter grid where one config outperforms due to luck
    And neighboring configurations have much worse performance
    When I compute stability scores
    Then the outlier should have a low stability score
    And a config with consistent neighbor performance should have a high stability score

  @sweep @stability @neighbors
  Scenario: Neighbor sensitivity measures robustness to parameter changes
    Given a completed sweep with entry_lookback [18, 19, 20, 21, 22]
    When I compute neighbor sensitivity for entry_lookback=20
    Then I should see the performance variance across +/- 1 and +/- 2 neighbors
    And smooth performance curves indicate robust parameters

  @sweep @costs
  Scenario: Cost sensitivity curve shows performance degradation
    Given a winning configuration
    When I compute cost sensitivity from 0 to 50 bps in 10 bps steps
    Then I should get performance at each cost level
    And I should see the breakeven cost level where returns go negative

  @sweep @output
  Scenario: Sweep outputs are saved to standard locations
    Given a completed sweep run with sweep_id "test_sweep_001"
    Then results should be saved to "reports/runs/test_sweep_001/"
    And the directory should contain "manifest.json"
    And the directory should contain "results.parquet"
    And the directory should contain "summary.md"
