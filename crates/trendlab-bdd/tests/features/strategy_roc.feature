@strategy @roc @momentum
Feature: ROC strategy
  Rate of Change momentum crossover strategy.
  Entry: ROC crosses above zero (positive momentum)
  Exit: ROC crosses below zero (negative momentum)

  ROC formula:
  - ROC = ((Close - Close[n]) / Close[n]) * 100
  - Positive ROC: price is higher than n bars ago
  - Negative ROC: price is lower than n bars ago

  Simple percentage-based momentum indicator

  Background:
    Given a synthetic bar series from fixture synth/momentum_50.csv

  @entry @roc_positive
  Scenario: Entry triggers when ROC crosses above zero
    Given a ROC strategy with period 12
    When I run the strategy
    Then a long entry signal must occur
    And ROC must cross above 0 at entry

  @exit @roc_negative
  Scenario: Exit triggers when ROC crosses below zero
    Given a ROC strategy with period 12
    When I run the strategy
    Then an exit signal must occur when ROC crosses below 0

  @warmup
  Scenario: Warmup period equals the lookback period
    Given a ROC strategy with period 12
    Then the warmup period must be 12 bars

  @determinism
  Scenario: ROC results are deterministic
    Given a ROC strategy with period 12
    When I run the strategy twice
    Then the two results must be identical

  @percentage_calculation
  Scenario: ROC correctly calculates percentage change
    Given a ROC strategy with period 10
    When close moves from 100 to 110 over 10 bars
    Then ROC should equal 10 percent
