@strategy @tsmom
Feature: Time-Series Momentum (TSMOM) strategy
  Academic momentum strategy based on absolute return over a lookback period.
  Entry: When N-period return is positive (price > price N bars ago)
  Exit: When N-period return turns negative (price < price N bars ago)

  This is a pure time-series momentum strategy that does not require cross-sectional
  ranking. It captures the well-documented "momentum effect" where assets that have
  been rising tend to continue rising.

  Common lookback periods:
  - 12-month (252 days): Academic standard from Moskowitz et al.
  - 6-month (126 days): Short-term momentum
  - 1-month (21 days): Very short-term

  Background:
    Given a synthetic bar series from fixture synth/tsmom_30.csv

  @entry @positive_momentum
  Scenario: Entry triggers when N-period return becomes positive
    Given a TSMOM strategy with lookback period 10
    When I run the strategy
    Then a long entry signal must occur at index 15
    And the entry was triggered because close 115.0 > close 10 bars ago 99.0

  @exit @negative_momentum
  Scenario: Exit triggers when N-period return becomes negative
    Given a TSMOM strategy with lookback period 10
    When I run the strategy
    Then an exit signal must occur at index 25
    And the exit was triggered because close 95.0 < close 10 bars ago 115.0

  @warmup
  Scenario: No signals during warmup period
    Given a TSMOM strategy with lookback period 10
    When I run the strategy
    Then no entry signal occurs before index 10
    And the warmup period must be 10 bars

  @complete_trade
  Scenario: Complete round-trip trade from entry to exit
    Given a TSMOM strategy with lookback period 10
    And fees are set to 0 bps per side
    When I run the strategy with backtest
    Then a complete trade must occur from index 15 to index 25
    And the entry fill must be at index 16 open price
    And the exit fill must be at index 26 open price

  @determinism
  Scenario: TSMOM results are deterministic
    Given a TSMOM strategy with lookback period 10
    When I run the strategy twice
    Then the two results must be identical

  @threshold @no_entry_at_zero
  Scenario: No entry when return is exactly zero
    Given a TSMOM strategy with lookback period 10
    When close equals close 10 bars ago at index 12
    Then no entry signal is generated at index 12

  @preset_12m
  Scenario: Standard 12-month preset uses 252-day lookback
    Given the TSMOM 12-month preset strategy
    Then the lookback period must be 252

  @preset_6m
  Scenario: 6-month preset uses 126-day lookback
    Given the TSMOM 6-month preset strategy
    Then the lookback period must be 126

  @preset_1m
  Scenario: 1-month preset uses 21-day lookback
    Given the TSMOM 1-month preset strategy
    Then the lookback period must be 21

  @momentum_calculation
  Scenario: Momentum is calculated as simple return
    Given a TSMOM strategy with lookback period 10
    When I compute momentum at index 20
    Then momentum equals (close[20] - close[10]) / close[10]
    And the sign of momentum determines the signal

  @re_entry
  Scenario: Strategy can re-enter after exiting
    Given a TSMOM strategy with lookback period 10
    And the price pattern shows: up trend, down trend, up trend
    When I run the strategy
    Then the strategy should enter, exit, and enter again
