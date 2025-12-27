@strategy @heikin_ashi @price_structure
Feature: Heikin-Ashi Regime strategy
  Trend-following strategy using Heikin-Ashi candle patterns.
  Heikin-Ashi smooths price action to identify regime changes.
  Entry on bullish regime confirmation, exit on bearish regime.

  Background:
    Given a synthetic bar series from fixture synth/heikin_ashi_20.csv

  @ha_calculation
  Scenario: Heikin-Ashi candles calculated correctly
    Given a Heikin-Ashi strategy with confirmation bars 2
    When I compute Heikin-Ashi candles at index 5
    Then the HA close must be the average of open, high, low, close
    And the HA open must be the average of prior HA open and prior HA close
    And the HA high must be the max of high, HA open, HA close
    And the HA low must be the min of low, HA open, HA close

  @entry @bullish_regime
  Scenario: Entry triggers on confirmed bullish regime
    Given a Heikin-Ashi strategy with confirmation bars 2
    When I run the strategy
    Then a long entry signal must occur at index 17
    And the entry was triggered by 2 consecutive bullish HA candles

  @strong_bullish
  Scenario: Strong bullish candle has no lower wick
    Given a Heikin-Ashi strategy with confirmation bars 2
    When I identify a strong bullish HA candle
    Then the HA low must equal the HA open
    And this indicates strong buying pressure

  @exit @bearish_regime
  Scenario: Exit triggers on confirmed bearish regime
    Given a Heikin-Ashi strategy with confirmation bars 2
    When I run the strategy
    Then no exit signal is generated

  @strong_bearish
  Scenario: Strong bearish candle has no upper wick
    Given a Heikin-Ashi strategy with confirmation bars 2
    When I identify a strong bearish HA candle
    Then the HA high must equal the HA open
    And this indicates strong selling pressure

  @warmup
  Scenario: No signals during warmup period
    Given a Heikin-Ashi strategy with confirmation bars 2
    When I run the strategy
    Then no entry signal occurs before index 2
    And the warmup period must be 2 bars for confirmation

  @determinism
  Scenario: Heikin-Ashi strategy results are deterministic
    Given a Heikin-Ashi strategy with confirmation bars 2
    When I run the strategy twice
    Then the two results must be identical

  @complete_trade
  Scenario: No complete trade occurs in this short fixture
    Given a Heikin-Ashi strategy with confirmation bars 2
    When I run the strategy
    Then a long entry signal must occur at index 17
    And no exit signal is generated

  @confirmation_sensitivity
  Scenario: More confirmation bars filters noise
    Given a Heikin-Ashi strategy with confirmation bars 3
    When I run the strategy
    Then entry requires 3 consecutive bullish HA candles
    And exit requires 3 consecutive bearish HA candles
    And fewer whipsaw signals are generated

  @regime_persistence
  Scenario: Strategy stays in position during mixed signals
    Given a Heikin-Ashi strategy with confirmation bars 2
    When a single bearish candle appears during bullish regime
    Then no exit signal is generated
    And the position is maintained until confirmation count is reached

  @smoothing_effect
  Scenario: Heikin-Ashi smooths choppy price action
    Given regular OHLC bars with alternating up and down days
    When I compute Heikin-Ashi candles
    Then the HA candles show clearer directional bias
    And short-term noise is filtered out
