@invariants
Feature: Backtest invariants
  These scenarios lock down correctness properties that must never regress.

  @no_lookahead
  Scenario: Indicator values do not use future bars
    Given a synthetic bar series from fixture synth/lookahead_30.csv
    When I compute SMA with window 3
    And I modify bars after index 15
    And I compute SMA with window 3 again
    Then SMA values through index 15 must be identical

  @determinism
  Scenario: Backtest results are deterministic
    Given a synthetic bar series from fixture synth/determinism_30.csv
    When I run a backtest with fixed entry at index 3 and exit at index 10
    And I run the same backtest again
    Then the two backtest results must be identical

  @accounting
  Scenario: Equity equals cash plus marked-to-market positions each bar
    Given a synthetic bar series from fixture synth/determinism_30.csv
    When I run a backtest with fixed entry at index 3 and exit at index 10
    Then for every bar equity must equal cash plus position_qty times close

  @fill_next_open
  Scenario: Default fill convention is next bar open
    Given a synthetic bar series from fixture synth/fill_next_open.csv
    When I run a backtest with fixed entry at index 2 and exit at index 4
    Then the entry fill price must equal the open price at index 3


