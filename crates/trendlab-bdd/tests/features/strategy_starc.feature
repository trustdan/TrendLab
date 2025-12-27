@strategy @starc @bands
Feature: STARC Bands Breakout strategy
  Manning Stoller's Average Range Channel breakout system.

  STARC (Stoller Average Range Channel) Calculation:
  - Center Line = SMA(close, sma_period)
  - Upper Band = Center + Multiplier * ATR(atr_period)
  - Lower Band = Center - Multiplier * ATR(atr_period)

  Similar to Keltner Channel but uses SMA instead of EMA,
  making it less responsive but potentially more stable.

  Entry: Close breaks above upper band
  Exit: Close falls below lower band

  Background:
    Given a synthetic bar series from fixture synth/starc_30.csv

  @entry @breakout_upper
  Scenario: Entry triggers when close breaks above upper band
    Given a STARC strategy with SMA period 20, ATR period 15, multiplier 2.0
    When I run the strategy
    Then a long entry signal must occur
    And close must be above the upper band at entry

  @exit @below_lower
  Scenario: Exit triggers when close falls below lower band
    Given a STARC strategy with SMA period 20, ATR period 15, multiplier 2.0
    When I run the strategy
    Then an exit signal must occur when close < lower band

  @warmup
  Scenario: Warmup period is max of SMA and ATR periods
    Given a STARC strategy with SMA period 20, ATR period 15, multiplier 2.0
    Then the warmup period must be 20 bars

  @warmup_atr_longer
  Scenario: Warmup uses ATR period when longer
    Given a STARC strategy with SMA period 10, ATR period 20, multiplier 2.0
    Then the warmup period must be 20 bars

  @sma_vs_ema
  Scenario: SMA center is less responsive than EMA
    Given a STARC strategy with SMA period 20, ATR period 15, multiplier 2.0
    When price makes a sharp move
    Then the SMA center should lag more than an equivalent EMA

  @band_calculation
  Scenario: STARC bands are calculated correctly
    Given a STARC strategy with SMA period 20, ATR period 15, multiplier 2.0
    When I compute STARC bands
    Then upper band must equal SMA + 2.0 * ATR
    And lower band must equal SMA - 2.0 * ATR
    And center must equal SMA of close

  @multiplier_effect
  Scenario: Higher multiplier creates wider bands
    Given a STARC strategy with SMA period 20, ATR period 15, multiplier 3.0
    When I compute STARC bands
    Then bands should be 50% wider than with multiplier 2.0

  @determinism
  Scenario: STARC results are deterministic
    Given a STARC strategy with SMA period 20, ATR period 15, multiplier 2.0
    When I run the strategy twice
    Then the two results must be identical

  @comparison_keltner
  Scenario: STARC differs from Keltner only in center line
    Given equivalent STARC and Keltner configurations
    When I compute both indicators
    Then STARC center (SMA) differs from Keltner center (EMA)
    And both use ATR for band width calculation
