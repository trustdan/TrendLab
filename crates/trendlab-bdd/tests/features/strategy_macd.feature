@strategy @macd @momentum
Feature: MACD strategy
  Gerald Appel's Moving Average Convergence Divergence strategy.
  Entry: MACD line crosses above signal line
  Exit: MACD line crosses below signal line

  MACD formula:
  - MACD Line = EMA(fast) - EMA(slow)
  - Signal Line = EMA(MACD Line, signal period)
  - Histogram = MACD Line - Signal Line

  Default periods: 12/26/9 (fast/slow/signal)

  Background:
    Given a synthetic bar series from fixture synth/momentum_50.csv

  @entry @signal_cross
  Scenario: Entry triggers when MACD crosses above signal line
    Given a MACD strategy with fast 12, slow 26, signal 9, mode CrossSignal
    When I run the strategy
    Then a long entry signal must occur
    And MACD line must cross above signal line at entry

  @exit @signal_cross_down
  Scenario: Exit triggers when MACD crosses below signal line
    Given a MACD strategy with fast 12, slow 26, signal 9, mode CrossSignal
    When I run the strategy
    Then an exit signal must occur when MACD crosses below signal line

  @entry_mode @zero_cross
  Scenario: Zero-cross mode enters when MACD crosses above zero
    Given a MACD strategy with fast 12, slow 26, signal 9, mode CrossZero
    When I run the strategy
    Then entry occurs when MACD line crosses above zero

  @entry_mode @histogram
  Scenario: Histogram mode enters when histogram turns positive
    Given a MACD strategy with fast 12, slow 26, signal 9, mode Histogram
    When I run the strategy
    Then entry occurs when histogram changes from negative to positive

  @warmup
  Scenario: Warmup period equals slow period plus signal period minus 1
    Given a MACD strategy with fast 12, slow 26, signal 9, mode CrossSignal
    Then the warmup period must be 34 bars

  @determinism
  Scenario: MACD results are deterministic
    Given a MACD strategy with fast 12, slow 26, signal 9, mode CrossSignal
    When I run the strategy twice
    Then the two results must be identical

  @parameter_validation
  Scenario: Slow period must be greater than fast period
    Given a MACD strategy with fast 26, slow 12, signal 9, mode CrossSignal
    Then the strategy should reject the configuration
