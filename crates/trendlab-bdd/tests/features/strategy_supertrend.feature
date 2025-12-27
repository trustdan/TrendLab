@strategy @supertrend
Feature: Supertrend strategy
  ATR-based trend following indicator developed by Olivier Seban.

  Supertrend Calculation:
  - Basic Upper Band = (High + Low) / 2 + Multiplier * ATR
  - Basic Lower Band = (High + Low) / 2 - Multiplier * ATR
  - Final bands are adjusted to only move in trend direction
  - Supertrend line = Lower band in uptrend, Upper band in downtrend

  Entry: Trend flips from down to up (close crosses above supertrend line)
  Exit: Trend flips from up to down (close crosses below supertrend line)

  Background:
    Given a synthetic bar series from fixture synth/supertrend_30.csv

  @entry @trend_flip_up
  Scenario: Entry triggers when trend flips to uptrend
    Given a Supertrend strategy with ATR period 10 and multiplier 3.0
    When I run the strategy
    Then a long entry signal must occur
    And the supertrend line must flip from upper band to lower band

  @exit @trend_flip_down
  Scenario: Exit triggers when trend flips to downtrend
    Given a Supertrend strategy with ATR period 10 and multiplier 3.0
    When I run the strategy
    Then an exit signal must occur when close crosses below supertrend

  @warmup
  Scenario: Warmup period equals ATR period
    Given a Supertrend strategy with ATR period 14 and multiplier 3.0
    Then the warmup period must be 14 bars

  @multiplier_sensitivity
  Scenario: Lower multiplier creates tighter bands and more signals
    Given a Supertrend strategy with ATR period 10 and multiplier 2.0
    When I run the strategy
    Then more trend flips should occur than with multiplier 3.0

  @band_adjustment
  Scenario: Bands only move in trend-favorable direction
    Given a Supertrend strategy with ATR period 10 and multiplier 3.0
    When I compute supertrend values
    Then lower band must only increase during uptrend
    And upper band must only decrease during downtrend

  @determinism
  Scenario: Supertrend results are deterministic
    Given a Supertrend strategy with ATR period 10 and multiplier 3.0
    When I run the strategy twice
    Then the two results must be identical

  @indicator_values
  Scenario: Supertrend values are computed correctly
    Given a Supertrend strategy with ATR period 10 and multiplier 3.0
    When I compute supertrend at a specific bar
    Then supertrend line equals lower band when is_uptrend is true
    And supertrend line equals upper band when is_uptrend is false
