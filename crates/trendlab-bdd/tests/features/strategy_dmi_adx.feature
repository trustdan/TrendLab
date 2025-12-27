@strategy @dmi @adx
Feature: DMI/ADX Directional System
  Welles Wilder's Directional Movement Index with ADX filter.
  Entry: +DI crosses above -DI AND ADX > threshold (trend is strong)
  Exit: +DI crosses below -DI OR ADX falls below threshold

  Uses three key components:
  - +DI (Plus Directional Indicator): Measures upward price movement
  - -DI (Minus Directional Indicator): Measures downward price movement
  - ADX (Average Directional Index): Measures trend strength (0-100)

  Background:
    Given a synthetic bar series from fixture synth/dmi_trending_40.csv

  @entry @di_cross
  Scenario: Entry triggers when +DI crosses above -DI with strong ADX
    Given a DMI/ADX strategy with DI period 14, ADX period 14, threshold 25
    When I run the strategy
    Then a long entry signal must occur
    And ADX must be above 25 at entry

  @no_entry @weak_trend
  Scenario: Higher ADX threshold delays entry
    Given a DMI/ADX strategy with DI period 14, ADX period 14, threshold 50
    When I run the strategy
    Then a long entry signal must occur
    And ADX must be above 50 at entry

  @exit @di_cross_down
  Scenario: Exit triggers when +DI crosses below -DI
    Given a DMI/ADX strategy with DI period 14, ADX period 14, threshold 25
    When I run the strategy
    Then an exit signal must occur when +DI crosses below -DI

  @warmup
  Scenario: Warmup period is double the period for two smoothing passes
    Given a DMI/ADX strategy with DI period 14, ADX period 14, threshold 25
    Then the warmup period must be 28 bars

  @determinism
  Scenario: DMI/ADX results are deterministic
    Given a DMI/ADX strategy with DI period 14, ADX period 14, threshold 25
    When I run the strategy twice
    Then the two results must be identical

  @indicator_calculation
  Scenario: DI values are calculated correctly with Wilder smoothing
    Given a DMI/ADX strategy with DI period 14, ADX period 14, threshold 25
    When I compute DMI indicators
    Then +DI and -DI values are between 0 and 100
    And ADX is between 0 and 100

  @crossover_detection
  Scenario: DI crossover detection requires actual crossing
    Given a DMI/ADX strategy with DI period 14, ADX period 14, threshold 25
    When +DI equals -DI
    Then no signal is generated until +DI exceeds -DI
