@strategy @turtle @turtle_s2
Feature: Turtle System 2 strategy (55-day entry, 20-day exit)
  The Turtle Trading System 2 uses a 55-day breakout for entry
  and a 20-day breakdown for exit. This is a longer-term trend-following
  system designed to capture major trends with more patient exits.

  Background:
    Given a synthetic bar series from fixture synth/turtle_s2_70.csv

  @entry
  Scenario: Entry triggers when close breaks 55-day high
    Given a Turtle System 2 strategy (55-day entry, 20-day exit)
    When I run the strategy
    Then a long entry signal must occur at index 55
    And the entry was triggered because close 112.0 exceeded the 55-day high 111.6

  @exit
  Scenario: Exit triggers when close breaks 20-day low
    Given a Turtle System 2 strategy (55-day entry, 20-day exit)
    When I run the strategy
    Then an exit signal must occur at index 64
    And the exit was triggered because close 107.0 broke the 20-day low 108.0

  @warmup
  Scenario: No signals during warmup period
    Given a Turtle System 2 strategy (55-day entry, 20-day exit)
    When I run the strategy
    Then no entry signal occurs before index 55
    And the warmup period must be 55 bars

  @complete_trade
  Scenario: Complete round-trip trade from entry to exit
    Given a Turtle System 2 strategy (55-day entry, 20-day exit)
    And fees are set to 0 bps per side
    When I run the strategy with backtest
    Then a complete trade must occur from index 55 to index 64
    And the entry fill must be at index 56 open price
    And the exit fill must be at index 65 open price

  @determinism
  Scenario: Turtle System 2 results are deterministic
    Given a Turtle System 2 strategy (55-day entry, 20-day exit)
    When I run the strategy twice
    Then the two results must be identical

  @asymmetric
  Scenario: Entry and exit lookbacks are asymmetric
    Given a Turtle System 2 strategy (55-day entry, 20-day exit)
    Then the entry lookback must be 55
    And the exit lookback must be 20
    And entry lookback is longer than exit lookback for faster exits

  @preset
  Scenario: Turtle System 2 uses correct preset parameters
    Given the Turtle System 2 preset strategy
    Then the entry lookback must be 55
    And the exit lookback must be 20

  @longer_term
  Scenario: Turtle System 2 requires longer warmup than System 1
    Given a Turtle System 2 strategy (55-day entry, 20-day exit)
    And a Turtle System 1 strategy (20-day entry, 10-day exit) for comparison
    Then System 2 warmup period must be greater than System 1 warmup period
