@indicators
Feature: Indicator calculations are correctly aligned
  Indicators must use only historical data (no lookahead) and match defined formulas.

  Scenario: SMA uses only prior closes including current bar
    Given a synthetic bar series from fixture synth/lookahead_30.csv
    When I compute SMA with window 3
    Then SMA at index 2 must equal 101.0
    And SMA at index 3 must equal 102.0

  @donchian
  Scenario: Donchian channel uses highest high and lowest low of prior N bars
    Given a synthetic bar series from fixture synth/donchian_10.csv
    When I compute Donchian channel with lookback 5
    Then Donchian upper at index 5 must equal 106.0
    And Donchian lower at index 5 must equal 98.0

  @donchian
  Scenario: Donchian channel is undefined during warmup period
    Given a synthetic bar series from fixture synth/donchian_10.csv
    When I compute Donchian channel with lookback 5
    Then Donchian values at index 0 through 4 must be undefined

  @donchian @no_lookahead
  Scenario: Donchian values do not use future bars
    Given a synthetic bar series from fixture synth/donchian_10.csv
    When I compute Donchian channel with lookback 5
    And I record Donchian values through index 6
    And I modify bars after index 6
    And I compute Donchian channel with lookback 5 again
    Then Donchian values through index 6 must be identical
