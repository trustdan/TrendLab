@strategy @hybrid @confluence
Feature: Oscillator Confluence strategy
  Multi-oscillator strategy requiring RSI and Stochastic agreement.
  Entry: RSI crosses above oversold AND Stochastic %K crosses above %D
  Exit: RSI crosses below overbought OR Stochastic %K crosses below %D

  Uses two independent oscillators for confirmation, reducing false signals.

  Background:
    Given a synthetic bar series from fixture synth/oscillator_30.csv

  @entry @dual_confirmation
  Scenario: Entry triggers when both oscillators confirm
    Given a Confluence strategy with RSI period 14, RSI oversold 30, RSI overbought 70, Stoch K 14, Stoch K-smooth 3, Stoch D 3, Stoch oversold 20, Stoch overbought 80
    When I run the strategy
    Then a long entry signal must occur
    And RSI must cross above 30 at entry
    And Stochastic %K must cross above %D at entry

  @exit @either_oscillator
  Scenario: Exit triggers when either oscillator signals exit
    Given a Confluence strategy with RSI period 14, RSI oversold 30, RSI overbought 70, Stoch K 14, Stoch K-smooth 3, Stoch D 3, Stoch oversold 20, Stoch overbought 80
    When I run the strategy
    Then an exit signal occurs when RSI crosses below 70 or Stochastic signals bearish crossover

  @partial_signal
  Scenario: No entry when only RSI confirms
    Given a Confluence strategy with RSI period 14, RSI oversold 30, RSI overbought 70, Stoch K 14, Stoch K-smooth 3, Stoch D 3, Stoch oversold 20, Stoch overbought 80
    When RSI crosses above oversold but Stochastic shows no bullish cross
    Then no entry signal is generated

  @warmup
  Scenario: Warmup period is maximum of both indicator warmups
    Given a Confluence strategy with RSI period 14, RSI oversold 30, RSI overbought 70, Stoch K 14, Stoch K-smooth 3, Stoch D 3, Stoch oversold 20, Stoch overbought 80
    Then the warmup period must be 20 bars

  @determinism
  Scenario: Confluence results are deterministic
    Given a Confluence strategy with RSI period 14, RSI oversold 30, RSI overbought 70, Stoch K 14, Stoch K-smooth 3, Stoch D 3, Stoch oversold 20, Stoch overbought 80
    When I run the strategy twice
    Then the two results must be identical

  @independent_oscillators
  Scenario: RSI and Stochastic are computed independently
    Given a Confluence strategy with RSI period 14, RSI oversold 30, RSI overbought 70, Stoch K 14, Stoch K-smooth 3, Stoch D 3, Stoch oversold 20, Stoch overbought 80
    When I compute both indicators
    Then RSI uses Wilder smoothing on price change
    And Stochastic uses highest-high/lowest-low of OHLC
