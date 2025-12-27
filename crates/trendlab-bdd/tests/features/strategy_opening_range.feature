@strategy @opening_range @breakout
Feature: Opening Range Breakout strategy
  Intraday-style breakout strategy adapted for daily bars with period cycles.

  Opening Range:
  - First N bars of each period (week/month/rolling) define the range
  - Range high = max(high) of range bars
  - Range low = min(low) of range bars

  Periods:
  - Weekly: Range resets each Monday
  - Monthly: Range resets on first trading day of month
  - Rolling: Continuous N-bar lookback (no calendar reset)

  Entry: Close breaks above range high (after range complete)
  Exit: Close breaks below range low OR trailing stop

  Background:
    Given a synthetic bar series from fixture synth/orb_weekly_30.csv

  @range_detection @weekly
  Scenario: Weekly range detected correctly
    Given an ORB strategy with 5 bars, Weekly period
    When I compute the opening range
    Then range_high should equal max high of first 5 bars of each week
    And range_low should equal min low of first 5 bars of each week

  @range_detection @monthly
  Scenario: Monthly range detected correctly
    Given an ORB strategy with 3 bars, Monthly period
    When I compute the opening range
    Then range_high should equal max high of first 3 bars of each month
    And range_low should equal min low of first 3 bars of each month

  @range_detection @rolling
  Scenario: Rolling range uses continuous lookback
    Given an ORB strategy with 5 bars, Rolling period
    When I compute the opening range
    Then range should use trailing 5 bars continuously
    And no calendar reset should occur

  @entry @breakout
  Scenario: Entry triggers on breakout above range high
    Given an ORB strategy with 5 bars, Weekly period
    When I run the strategy
    Then a long entry signal must occur when:
      | condition | value |
      | is_range_complete | true |
      | close > range_high | true |

  @no_signal_in_range
  Scenario: No signals during range formation
    Given an ORB strategy with 5 bars, Weekly period
    When I run the strategy
    Then no entry signal should occur during first 5 bars of each period
    And range should be marked as incomplete

  @exit @range_low
  Scenario: Exit triggers on break below range low
    Given an ORB strategy with 5 bars, Weekly period
    When I run the strategy with a position
    Then an exit signal must occur when close < range_low

  @period_reset
  Scenario: Range resets at period boundary
    Given an ORB strategy with 5 bars, Weekly period
    When a new week begins
    Then range_high and range_low should reset
    And is_range_complete should be false
    And bars_in_range should restart at 0

  @warmup
  Scenario: Warmup period equals range bar count
    Given an ORB strategy with 5 bars, Weekly period
    Then the warmup period must be 5 bars

  @determinism
  Scenario: Opening Range Breakout results are deterministic
    Given an ORB strategy with 5 bars, Weekly period
    When I run the strategy twice
    Then the two results must be identical

  @range_width
  Scenario: Range width can be used for position sizing
    Given an ORB strategy with 5 bars, Weekly period
    When I compute the opening range
    Then range_width should equal range_high - range_low
    And range_width can be used for stop loss sizing
