@strategy @ensemble @voting
Feature: Multi-Horizon Ensemble strategy
  Combines signals from multiple strategy parameterizations via voting.

  Voting Methods:
  - Majority: Entry when >50% of child strategies signal entry
  - WeightedByHorizon: Longer horizons weighted more heavily
  - UnanimousEntry: All must agree for entry, any triggers exit

  Common Ensemble Presets:
  - Donchian Triple: 20/55/100 day breakouts
  - MA Triple: Short/Medium/Long moving average crosses
  - TSMOM Multi: Multiple momentum lookbacks

  Background:
    Given a synthetic bar series from fixture synth/ensemble_60.csv

  @majority_vote
  Scenario: Majority voting entry requires >50% agreement
    Given a Donchian Triple ensemble with Majority voting
    And horizons 20, 55, 100
    When 2 of 3 child strategies signal entry
    Then the ensemble should generate an entry signal
    And the third dissenting strategy is overruled

  @majority_no_entry
  Scenario: Majority voting blocks entry with <50% agreement
    Given a Donchian Triple ensemble with Majority voting
    And horizons 20, 55, 100
    When only 1 of 3 child strategies signals entry
    Then the ensemble should not generate an entry signal

  @weighted_voting
  Scenario: Weighted voting favors longer horizons
    Given a Donchian Triple ensemble with WeightedByHorizon voting
    And horizons 20, 55, 100
    When computing weighted votes
    Then the 100-day strategy should have highest weight
    And the 20-day strategy should have lowest weight

  @weighted_entry
  Scenario: Weighted voting entry with sufficient weight
    Given a Donchian Triple ensemble with WeightedByHorizon voting
    And horizons 20, 55, 100
    When long-horizon strategies agree
    Then entry should trigger even if short-horizon disagrees

  @unanimous_entry
  Scenario: Unanimous entry requires all strategies to agree
    Given a Donchian Triple ensemble with UnanimousEntry voting
    And horizons 20, 55, 100
    When all 3 child strategies signal entry
    Then the ensemble should generate an entry signal

  @unanimous_blocked
  Scenario: Unanimous entry blocked if any strategy disagrees
    Given a Donchian Triple ensemble with UnanimousEntry voting
    And horizons 20, 55, 100
    When 2 of 3 child strategies signal entry
    Then the ensemble should not generate an entry signal
    And entry waits for full consensus

  @unanimous_exit
  Scenario: Unanimous voting exits on any child exit signal
    Given a Donchian Triple ensemble with UnanimousEntry voting
    And horizons 20, 55, 100
    When any 1 child strategy signals exit
    Then the ensemble should generate an exit signal
    And this provides earlier risk-off response

  @preset_donchian_triple
  Scenario: Donchian Triple preset creates correct configuration
    Given a Donchian Triple preset ensemble
    Then it should contain 3 child strategies
    And horizons should be 20, 55, 100
    And each child should be a DonchianBreakoutStrategy

  @preset_ma_triple
  Scenario: MA Triple preset creates correct configuration
    Given an MA Triple preset ensemble
    Then it should contain 3 child strategies
    And horizons should be 20, 50, 200
    And each child should be an MACrossoverStrategy

  @warmup
  Scenario: Warmup period equals longest child horizon
    Given a Donchian Triple ensemble with Majority voting
    And horizons 20, 55, 100
    Then the warmup period must be 100 bars

  @determinism
  Scenario: Ensemble results are deterministic
    Given a Donchian Triple ensemble with Majority voting
    And horizons 20, 55, 100
    When I run the strategy twice
    Then the two results must be identical

  @signal_aggregation
  Scenario: Individual child signals are accessible
    Given a Donchian Triple ensemble with Majority voting
    And horizons 20, 55, 100
    When I run the strategy
    Then I should be able to inspect each child's individual signal
    And understand why the ensemble voted a particular way
