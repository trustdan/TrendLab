@strategy @hybrid @macd_adx
Feature: MACD + ADX filter strategy
  Momentum strategy combining MACD crossover with ADX trend strength filter.
  Entry: MACD crosses above signal AND ADX > threshold (strong trend)
  Exit: MACD crosses below signal

  Uses ADX to filter out MACD signals during choppy, non-trending markets.

  Background:
    Given a synthetic bar series from fixture synth/trending_50.csv

  @entry @filtered_momentum
  Scenario: Entry triggers on MACD cross with ADX confirmation
    Given a MACD+ADX strategy with fast 12, slow 26, signal 9, ADX period 14, ADX threshold 25
    When I run the strategy
    Then a long entry signal must occur
    And MACD must cross above signal at entry
    And ADX must be above 25 at entry

  @exit @macd_cross
  Scenario: Exit triggers when MACD crosses below signal
    Given a MACD+ADX strategy with fast 12, slow 26, signal 9, ADX period 14, ADX threshold 25
    When I run the strategy
    Then an exit signal must occur when MACD crosses below signal

  @filter @adx_weak
  Scenario: No entry when ADX is below threshold
    Given a MACD+ADX strategy with fast 12, slow 26, signal 9, ADX period 14, ADX threshold 25
    When MACD crosses above signal but ADX is below 25
    Then no entry signal is generated

  @warmup
  Scenario: Warmup period accounts for both indicators
    Given a MACD+ADX strategy with fast 12, slow 26, signal 9, ADX period 14, ADX threshold 25
    Then the warmup period must be 48 bars

  @determinism
  Scenario: MACD+ADX results are deterministic
    Given a MACD+ADX strategy with fast 12, slow 26, signal 9, ADX period 14, ADX threshold 25
    When I run the strategy twice
    Then the two results must be identical

  @adx_range
  Scenario: ADX is bounded between 0 and 100
    Given a MACD+ADX strategy with fast 12, slow 26, signal 9, ADX period 14, ADX threshold 25
    When I compute ADX indicators
    Then ADX must be between 0 and 100
