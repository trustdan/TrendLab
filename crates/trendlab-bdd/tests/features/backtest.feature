Feature: Backtest Engine
  As a quant researcher
  I want to backtest trend-following strategies
  So that I can evaluate their performance

  @invariant
  Scenario: No lookahead in signal generation
    Given no data loaded
    When the backtest runs
    Then no trades should be generated

  Scenario: Loading fixture data
    Given a bar series from fixture spy_100_bars.csv
    Then the result should contain fixture
