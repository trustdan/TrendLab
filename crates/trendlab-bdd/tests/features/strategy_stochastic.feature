@strategy @stochastic @oscillator
Feature: Stochastic Oscillator strategy
  George Lane's Stochastic Oscillator crossover strategy.
  Entry: %K crosses above %D when both below oversold (20)
  Exit: %K crosses below %D when both above overbought (80)

  Stochastic formula:
  - %K = 100 * (Close - Lowest Low) / (Highest High - Lowest Low)
  - %D = SMA of %K (smoothing period)

  Default periods: 14/3/3 (K period/K smooth/D period)

  Background:
    Given a synthetic bar series from fixture synth/oscillator_30.csv

  @entry @stochastic_cross
  Scenario: Entry triggers when %K crosses above %D in oversold zone
    Given a Stochastic strategy with K 14, K-smooth 3, D 3, oversold 20, overbought 80
    When I run the strategy
    Then a long entry signal must occur
    And %K must cross above %D when both are below 20

  @exit @stochastic_cross_down
  Scenario: Exit triggers when %K crosses below %D in overbought zone
    Given a Stochastic strategy with K 14, K-smooth 3, D 3, oversold 20, overbought 80
    When I run the strategy
    Then an exit signal must occur when %K crosses below %D when both above 80

  @warmup
  Scenario: Warmup period equals K period plus D period
    Given a Stochastic strategy with K 14, K-smooth 3, D 3, oversold 20, overbought 80
    Then the warmup period must be 20 bars

  @range
  Scenario: Stochastic values are bounded between 0 and 100
    Given a Stochastic strategy with K 14, K-smooth 3, D 3, oversold 20, overbought 80
    When I compute Stochastic indicators
    Then %K and %D must be between 0 and 100

  @determinism
  Scenario: Stochastic results are deterministic
    Given a Stochastic strategy with K 14, K-smooth 3, D 3, oversold 20, overbought 80
    When I run the strategy twice
    Then the two results must be identical

  @zone_filtering
  Scenario: Signals only trigger in appropriate zones
    Given a Stochastic strategy with K 14, K-smooth 3, D 3, oversold 20, overbought 80
    When %K and %D are both above 20 and below 80
    Then no entry or exit signal is generated
