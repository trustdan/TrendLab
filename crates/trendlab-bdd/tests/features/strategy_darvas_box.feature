@strategy @darvas_box @price_structure
Feature: Darvas Box strategy
  Nicolas Darvas box breakout strategy. Forms boxes around price consolidation
  and enters on breakout above the box top. Classic price structure pattern.

  Background:
    Given a synthetic bar series from fixture synth/darvas_box_25.csv

  @box_formation
  Scenario: Box forms after price consolidates
    Given a Darvas box strategy with confirmation bars 3
    When I run the strategy through index 10
    Then a box must be formed with top 110.0 and bottom 103.0
    And the box must be confirmed after 3 bars of consolidation

  @entry @breakout
  Scenario: Entry triggers on breakout above box top
    Given a Darvas box strategy with confirmation bars 3
    When I run the strategy
    Then a long entry signal must occur at index 10
    And the entry was triggered because close 111.5 broke above box top 110.0

  @exit @breakdown
  Scenario: Exit triggers when price breaks below box bottom
    Given a Darvas box strategy with confirmation bars 3
    When I run the strategy
    Then an exit signal must occur when price breaks below box bottom
    And the position must be closed on breakdown

  @warmup
  Scenario: No signals during warmup period
    Given a Darvas box strategy with confirmation bars 3
    When I run the strategy
    Then no entry signal occurs before box formation completes
    And the warmup period must be at least 4 bars

  @box_update
  Scenario: New box forms after successful breakout
    Given a Darvas box strategy with confirmation bars 3
    When price breaks out and consolidates again
    Then a new box must form with updated top and bottom levels
    And the previous box is discarded

  @determinism
  Scenario: Darvas box strategy results are deterministic
    Given a Darvas box strategy with confirmation bars 3
    When I run the strategy twice
    Then the two results must be identical

  @complete_trade
  Scenario: Complete round-trip trade from breakout to breakdown
    Given a Darvas box strategy with confirmation bars 3
    And fees are set to 0 bps per side
    When I run the strategy with backtest
    Then a complete trade must occur from breakout to breakdown
    And the entry fill must be at the bar after breakout open price
    And the exit fill must be at the bar after breakdown open price

  @confirmation_sensitivity
  Scenario: More confirmation bars requires longer consolidation
    Given a Darvas box strategy with confirmation bars 5
    When I run the strategy
    Then box formation requires 5 bars of price staying within the range
    And fewer false breakouts are triggered

  @no_box_in_trend
  Scenario: No box forms during strong directional move
    Given a Darvas box strategy with confirmation bars 3
    When price makes new highs on consecutive bars
    Then no box is formed until consolidation begins
    And box top keeps updating to track new highs
