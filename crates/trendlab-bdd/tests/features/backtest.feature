@deprecated
Feature: Backtest Engine (deprecated)
  This feature was an initial scaffold. The real Milestone-0 suite lives in:
  - invariants.feature
  - costs.feature

  Scenario: Deprecated placeholder
    Given a synthetic bar series from fixture synth/determinism_30.csv
    When I compute SMA with window 3
    And I modify bars after index 0
    And I compute SMA with window 3 again
    Then SMA values through index 0 must be identical
