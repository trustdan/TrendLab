@strategy @keltner @channel
Feature: Keltner Channel Breakout strategy
  Chester Keltner's volatility channel breakout system.

  Keltner Channel Calculation:
  - Center Line = EMA(close, ema_period)
  - Upper Band = Center + Multiplier * ATR(atr_period)
  - Lower Band = Center - Multiplier * ATR(atr_period)

  Entry: Close breaks above upper band (volatility expansion)
  Exit: Close falls below center (EMA) OR below lower band

  Background:
    Given a synthetic bar series from fixture synth/keltner_30.csv

  @entry @breakout_upper
  Scenario: Entry triggers when close breaks above upper band
    Given a Keltner strategy with EMA period 20, ATR period 10, multiplier 2.0
    When I run the strategy
    Then a long entry signal must occur
    And close must be above the upper band at entry

  @exit @below_center
  Scenario: Exit triggers when close falls below center (EMA)
    Given a Keltner strategy with EMA period 20, ATR period 10, multiplier 2.0
    When I run the strategy
    Then an exit signal must occur when close < center EMA

  @exit @below_lower
  Scenario: Exit also triggers when close falls below lower band
    Given a Keltner strategy with EMA period 20, ATR period 10, multiplier 2.0
    When I run the strategy
    Then an exit signal must occur when close < lower band

  @warmup
  Scenario: Warmup period is max of EMA and ATR periods
    Given a Keltner strategy with EMA period 20, ATR period 10, multiplier 2.0
    Then the warmup period must be 20 bars

  @warmup_atr_longer
  Scenario: Warmup uses ATR period when longer
    Given a Keltner strategy with EMA period 10, ATR period 20, multiplier 2.0
    Then the warmup period must be 20 bars

  @multiplier_effect
  Scenario: Higher multiplier creates wider bands
    Given a Keltner strategy with EMA period 20, ATR period 10, multiplier 3.0
    When I compute Keltner bands
    Then bands should be 50% wider than with multiplier 2.0

  @band_calculation
  Scenario: Keltner bands are calculated correctly
    Given a Keltner strategy with EMA period 20, ATR period 10, multiplier 2.0
    When I compute Keltner bands
    Then upper band must equal EMA + 2.0 * ATR
    And lower band must equal EMA - 2.0 * ATR
    And center must equal EMA of close

  @determinism
  Scenario: Keltner results are deterministic
    Given a Keltner strategy with EMA period 20, ATR period 10, multiplier 2.0
    When I run the strategy twice
    Then the two results must be identical

  @ema_responsiveness
  Scenario: EMA center line responds to price changes
    Given a Keltner strategy with EMA period 20, ATR period 10, multiplier 2.0
    When price trends strongly upward
    Then the center line must follow price with a lag
