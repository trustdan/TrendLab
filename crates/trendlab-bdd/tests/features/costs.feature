@costs
Feature: Fees and slippage are explicit and applied consistently

  Scenario: Fees reduce PnL by the expected amount
    Given a synthetic bar series from fixture synth/costs_roundtrip.csv
    And fees are set to 10 bps per side
    And slippage is set to 0 bps
    When I run a backtest with fixed entry at index 1 and exit at index 3
    Then net PnL must equal gross PnL minus expected fees

  Scenario: Slippage adjusts fill prices in the correct direction
    Given a synthetic bar series from fixture synth/costs_roundtrip.csv
    And fees are set to 0 bps per side
    And slippage is set to 5 bps
    When I run a backtest with fixed entry at index 1 and exit at index 3
    Then entry fill must be worse than the raw price
    And exit fill must be worse than the raw price


