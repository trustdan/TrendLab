@strategy @52wk_high @price_structure
Feature: 52-Week High Proximity strategy
  Momentum strategy that enters when price is within a threshold of the 52-week high
  and exits when price drops below a lower threshold. Based on research showing
  stocks near highs tend to continue outperforming.

  Background:
    Given a synthetic bar series from fixture synth/52wk_high_20.csv

  @entry
  Scenario: Entry triggers when price is within threshold of 52-week high
    Given a 52-week high strategy with period 10, entry threshold 0.95, exit threshold 0.90
    When I run the strategy
    Then a long entry signal must occur at index 10
    And the entry was triggered because close 110.0 is within 95% of period high 111.0

  @exit
  Scenario: Exit triggers when price drops below exit threshold
    Given a 52-week high strategy with period 10, entry threshold 0.95, exit threshold 0.90
    When I run the strategy
    Then an exit signal must occur at index 17
    And the exit was triggered because price dropped below 90% of period high

  @warmup
  Scenario: No signals during warmup period
    Given a 52-week high strategy with period 10, entry threshold 0.95, exit threshold 0.90
    When I run the strategy
    Then no entry signal occurs before index 10
    And the warmup period must be 10 bars

  @proximity_calculation
  Scenario: Proximity percentage calculated correctly
    Given a 52-week high strategy with period 10, entry threshold 0.95, exit threshold 0.90
    When I compute high proximity at index 15
    Then the period high must be 114.5
    And the period low must be 107.0
    And the proximity percentage must be approximately 1.0

  @determinism
  Scenario: 52-week high strategy results are deterministic
    Given a 52-week high strategy with period 10, entry threshold 0.95, exit threshold 0.90
    When I run the strategy twice
    Then the two results must be identical

  @complete_trade
  Scenario: Complete round-trip trade from entry to exit
    Given a 52-week high strategy with period 10, entry threshold 0.95, exit threshold 0.90
    And fees are set to 0 bps per side
    When I run the strategy with backtest
    Then a complete trade must occur from index 10 to index 17
    And the entry fill must be at index 11 open price
    And the exit fill must be at index 18 open price

  @threshold_sensitivity
  Scenario: Tighter entry threshold delays entry
    Given a 52-week high strategy with period 10, entry threshold 0.99, exit threshold 0.90
    When I run the strategy
    Then entry signals occur later than with 0.95 threshold
    And only prices within 1% of period high trigger entry
