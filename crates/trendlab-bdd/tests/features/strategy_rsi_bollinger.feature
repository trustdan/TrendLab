@strategy @hybrid @rsi_bollinger
Feature: RSI + Bollinger Bands hybrid strategy
  Mean reversion strategy combining RSI oversold with Bollinger Band touches.
  Entry: RSI < oversold AND close touches lower Bollinger Band
  Exit: Close crosses above middle band OR RSI > exit threshold

  This hybrid strategy looks for oversold conditions confirmed by
  price at the lower volatility band, suggesting a potential bounce.

  Background:
    Given a synthetic bar series from fixture synth/mean_reversion_50.csv

  @entry @confluence
  Scenario: Entry triggers on RSI oversold plus lower band touch
    Given an RSI+Bollinger strategy with RSI period 14, RSI oversold 30, RSI exit 50, BB period 20, BB std 2.0
    When I run the strategy
    Then a long entry signal must occur
    And RSI must be below 30 at entry
    And close must be at or below lower Bollinger Band at entry

  @exit @middle_band
  Scenario: Exit triggers when close crosses above middle band
    Given an RSI+Bollinger strategy with RSI period 14, RSI oversold 30, RSI exit 50, BB period 20, BB std 2.0
    When I run the strategy
    Then an exit signal must occur when close crosses above middle band

  @exit @rsi_recovery
  Scenario: Exit also triggers when RSI crosses above exit threshold
    Given an RSI+Bollinger strategy with RSI period 14, RSI oversold 30, RSI exit 50, BB period 20, BB std 2.0
    When I run the strategy
    Then an exit signal can occur when RSI crosses above 50

  @warmup
  Scenario: Warmup period is maximum of RSI and BB periods
    Given an RSI+Bollinger strategy with RSI period 14, RSI oversold 30, RSI exit 50, BB period 20, BB std 2.0
    Then the warmup period must be 20 bars

  @determinism
  Scenario: RSI+Bollinger results are deterministic
    Given an RSI+Bollinger strategy with RSI period 14, RSI oversold 30, RSI exit 50, BB period 20, BB std 2.0
    When I run the strategy twice
    Then the two results must be identical

  @no_partial_signal
  Scenario: Both conditions must be met for entry
    Given an RSI+Bollinger strategy with RSI period 14, RSI oversold 30, RSI exit 50, BB period 20, BB std 2.0
    When only RSI is oversold but price is above lower band
    Then no entry signal is generated
