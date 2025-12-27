@strategy @parabolic_sar @trend_following
Feature: Parabolic SAR strategy
  Wilder's Parabolic Stop and Reverse (SAR) trend-following system.

  SAR Calculation:
  - SAR trails behind price, accelerating toward extreme point (EP)
  - AF (Acceleration Factor) starts at af_start, increments by af_step on new extremes
  - AF is capped at af_max
  - SAR flips when price crosses it, resetting AF to af_start

  Entry: SAR flips from above to below price (uptrend begins)
  Exit: SAR flips from below to above price (downtrend begins)

  Background:
    Given a synthetic bar series from fixture synth/parabolic_sar_50.csv

  @entry @flip
  Scenario: Entry triggers on SAR flip to uptrend
    Given a Parabolic SAR strategy with AF 0.02/0.02/0.20
    When I run the strategy
    Then a long entry signal must occur when SAR flips below price
    And the signal should indicate uptrend initiation

  @exit @flip
  Scenario: Exit triggers on SAR flip to downtrend
    Given a Parabolic SAR strategy with AF 0.02/0.02/0.20
    When I run the strategy
    Then an exit signal must occur when SAR flips above price
    And the signal should indicate trend reversal

  @sar_trailing
  Scenario: SAR trails below price in uptrend
    Given a Parabolic SAR strategy with AF 0.02/0.02/0.20
    When I run the strategy in uptrend conditions
    Then SAR should always be below price during uptrend
    And SAR should move higher each bar (never backwards)

  @sar_above
  Scenario: SAR stays above price in downtrend
    Given a Parabolic SAR strategy with AF 0.02/0.02/0.20
    When I run the strategy in downtrend conditions
    Then SAR should always be above price during downtrend
    And SAR should move lower each bar (never backwards)

  @acceleration @increment
  Scenario: AF increments on new extreme points
    Given a Parabolic SAR strategy with AF 0.02/0.02/0.20
    When price makes new highs in uptrend
    Then AF should increase from 0.02 toward 0.20
    And AF should increment by 0.02 for each new extreme

  @acceleration @cap
  Scenario: AF is capped at maximum value
    Given a Parabolic SAR strategy with AF 0.02/0.02/0.20
    When AF has been incremented many times
    Then AF should never exceed 0.20

  @af_reset
  Scenario: AF resets on trend reversal
    Given a Parabolic SAR strategy with AF 0.02/0.02/0.20
    When SAR flips from uptrend to downtrend
    Then AF should reset to 0.02

  @warmup
  Scenario: Warmup period for initial SAR calculation
    Given a Parabolic SAR strategy with AF 0.02/0.02/0.20
    Then the warmup period must be 5 bars
    And no signals should be generated during warmup

  @determinism
  Scenario: Parabolic SAR results are deterministic
    Given a Parabolic SAR strategy with AF 0.02/0.02/0.20
    When I run the strategy twice
    Then the two results must be identical

  @sar_never_penetrates
  Scenario: SAR never penetrates prior bars in uptrend
    Given a Parabolic SAR strategy with AF 0.02/0.02/0.20
    When I run the strategy in uptrend
    Then SAR should never be above the low of prior two bars

  @default_params
  Scenario: Default parameters follow Wilder's original
    Given a Parabolic SAR strategy with default parameters
    Then af_start should be 0.02
    And af_step should be 0.02
    And af_max should be 0.20
