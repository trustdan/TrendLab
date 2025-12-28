@strategy @williams_r @oscillator
Feature: Williams %R strategy
  Larry Williams' momentum oscillator strategy.
  Entry: %R crosses above oversold threshold (-80)
  Exit: %R crosses below overbought threshold (-20)

  Williams %R formula:
  - %R = (Highest High - Close) / (Highest High - Lowest Low) * -100
  - Range: -100 to 0
  - Inverse of Stochastic %K

  Note: %R uses negative values - -100 is oversold, 0 is overbought

  Background:
    Given a synthetic bar series from fixture synth/oscillator_30.csv

  @entry @williams_crossover
  Scenario: Entry triggers when %R crosses above oversold
    Given a Williams %R strategy with period 14, oversold -80, overbought -20
    When I run the strategy
    Then a long entry signal must occur
    And %R must cross above -80 at entry

  @exit @williams_overbought
  Scenario: Exit triggers when %R crosses below overbought
    Given a Williams %R strategy with period 14, oversold -80, overbought -20
    When I run the strategy
    Then an exit signal must occur when %R crosses below -20

  @warmup
  Scenario: Warmup period equals the lookback period
    Given a Williams %R strategy with period 14, oversold -80, overbought -20
    Then the warmup period must be 14 bars

  @range
  Scenario: Williams %R is bounded between -100 and 0
    Given a Williams %R strategy with period 14, oversold -80, overbought -20
    When I compute Williams %R indicators
    Then %R must be between -100 and 0

  @determinism
  Scenario: Williams %R results are deterministic
    Given a Williams %R strategy with period 14, oversold -80, overbought -20
    When I run the strategy twice
    Then the two results must be identical

  @inverse_stochastic
  Scenario: %R is the inverse of Stochastic %K
    Given a Williams %R strategy with period 14, oversold -80, overbought -20
    When I compute both %R and Stochastic %K
    Then %R should equal (%K - 100) at each bar
