@strategy @turtle @turtle_s1
Feature: Turtle System 1 strategy (20-day entry, 10-day exit)
  The classic Turtle Trading System 1 uses a 20-day breakout for entry
  and a 10-day breakdown for exit. This is a trend-following system
  designed to capture medium-term trends with faster exits.

  Background:
    Given a synthetic bar series from fixture synth/turtle_s1_30.csv

  @entry
  Scenario: Entry triggers when close breaks 20-day high
    Given a Turtle System 1 strategy (20-day entry, 10-day exit)
    When I run the strategy
    Then a long entry signal must occur at index 20
    And the entry was triggered because close 111.5 exceeded the 20-day high 110.5

  @exit
  Scenario: Exit triggers when close breaks 10-day low
    Given a Turtle System 1 strategy (20-day entry, 10-day exit)
    When I run the strategy
    Then an exit signal must occur at index 27
    And the exit was triggered because close 104.5 broke the 10-day low 106.0

  @warmup
  Scenario: No signals during warmup period
    Given a Turtle System 1 strategy (20-day entry, 10-day exit)
    When I run the strategy
    Then no entry signal occurs before index 20
    And the warmup period must be 20 bars

  @complete_trade
  Scenario: Complete round-trip trade from entry to exit
    Given a Turtle System 1 strategy (20-day entry, 10-day exit)
    And fees are set to 0 bps per side
    When I run the strategy with backtest
    Then a complete trade must occur from index 20 to index 27
    And the entry fill must be at index 21 open price
    And the exit fill must be at index 28 open price

  @determinism
  Scenario: Turtle System 1 results are deterministic
    Given a Turtle System 1 strategy (20-day entry, 10-day exit)
    When I run the strategy twice
    Then the two results must be identical

  @asymmetric
  Scenario: Entry and exit lookbacks are asymmetric
    Given a Turtle System 1 strategy (20-day entry, 10-day exit)
    Then the entry lookback must be 20
    And the exit lookback must be 10
    And entry lookback is longer than exit lookback for faster exits

  @preset
  Scenario: Turtle System 1 uses correct preset parameters
    Given the Turtle System 1 preset strategy
    Then the entry lookback must be 20
    And the exit lookback must be 10
