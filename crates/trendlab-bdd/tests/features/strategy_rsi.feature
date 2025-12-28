@strategy @rsi @oscillator
Feature: RSI strategy
  Welles Wilder's Relative Strength Index crossover strategy.
  Entry: RSI crosses above oversold threshold (e.g., 30)
  Exit: RSI crosses below overbought threshold (e.g., 70)

  RSI formula:
  - RS = Average Gain / Average Loss (Wilder smoothing)
  - RSI = 100 - (100 / (1 + RS))
  - Range: 0 to 100

  Background:
    Given a synthetic bar series from fixture synth/oscillator_30.csv

  @entry @rsi_crossover
  Scenario: Entry triggers when RSI crosses above oversold
    Given an RSI strategy with period 14, oversold 30, overbought 70
    When I run the strategy
    Then a long entry signal must occur
    And RSI must cross above 30 at entry

  @exit @rsi_overbought
  Scenario: Exit triggers when RSI crosses below overbought
    Given an RSI strategy with period 14, oversold 30, overbought 70
    When I run the strategy
    Then an exit signal must occur when RSI crosses below 70

  @warmup
  Scenario: Warmup period equals RSI period
    Given an RSI strategy with period 14, oversold 30, overbought 70
    Then the warmup period must be 14 bars

  @range
  Scenario: RSI is bounded between 0 and 100
    Given an RSI strategy with period 14, oversold 30, overbought 70
    When I compute RSI indicators
    Then RSI must be between 0 and 100

  @determinism
  Scenario: RSI results are deterministic
    Given an RSI strategy with period 14, oversold 30, overbought 70
    When I run the strategy twice
    Then the two results must be identical

  @crossover_detection
  Scenario: Crossover requires actual crossing
    Given an RSI strategy with period 14, oversold 30, overbought 70
    When RSI equals the oversold threshold
    Then no signal is generated until RSI exceeds the threshold
