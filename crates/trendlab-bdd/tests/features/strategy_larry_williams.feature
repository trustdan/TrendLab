@strategy @larry_williams @price_structure
Feature: Larry Williams Volatility Breakout strategy
  Range expansion breakout strategy inspired by Larry Williams.
  Enters when price exceeds the prior day's range by a multiple.
  Uses ATR-based trailing stop for exits.

  Background:
    Given a synthetic bar series from fixture synth/williams_range_20.csv

  @entry @range_breakout
  Scenario: Entry triggers on range expansion above prior day's high
    Given a Larry Williams strategy with range multiplier 0.5 and ATR stop 2.0 over 10 bars
    When I run the strategy
    Then a long entry signal must occur at index 11
    And the entry was triggered because high exceeded prior high + 0.5 * prior range

  @exit @atr_stop
  Scenario: Exit triggers when price drops below ATR trailing stop
    Given a Larry Williams strategy with range multiplier 0.5 and ATR stop 2.0 over 10 bars
    When I run the strategy
    Then an exit signal must occur when close drops below entry price - 2.0 * ATR
    And the position must be closed on stop hit

  @warmup
  Scenario: No signals during warmup period
    Given a Larry Williams strategy with range multiplier 0.5 and ATR stop 2.0 over 10 bars
    When I run the strategy
    Then no entry signal occurs before index 10
    And the warmup period must be 10 bars for ATR calculation

  @range_calculation
  Scenario: Prior day range calculated correctly
    Given a Larry Williams strategy with range multiplier 0.5 and ATR stop 2.0 over 10 bars
    When I compute range levels at index 12
    Then the prior high must be 108.0
    And the prior low must be 104.0
    And the prior range must be 4.0
    And the breakout level must be prior high + 0.5 * 4.0 = 110.0

  @determinism
  Scenario: Larry Williams strategy results are deterministic
    Given a Larry Williams strategy with range multiplier 0.5 and ATR stop 2.0 over 10 bars
    When I run the strategy twice
    Then the two results must be identical

  @complete_trade
  Scenario: Complete round-trip trade from breakout to stop
    Given a Larry Williams strategy with range multiplier 0.5 and ATR stop 2.0 over 10 bars
    And fees are set to 0 bps per side
    When I run the strategy with backtest
    Then a complete trade must occur from entry to ATR stop exit
    And the entry fill must be at the bar after breakout open price
    And the exit fill must be at the bar after stop hit open price

  @multiplier_sensitivity
  Scenario: Higher range multiplier requires larger breakout
    Given a Larry Williams strategy with range multiplier 1.0 and ATR stop 2.0 over 10 bars
    When I run the strategy
    Then fewer entry signals are generated
    And only moves exceeding 100% of prior range trigger entry

  @atr_stop_sensitivity
  Scenario: Tighter ATR stop exits earlier
    Given a Larry Williams strategy with range multiplier 0.5 and ATR stop 1.0 over 10 bars
    When I run the strategy
    Then exit signals occur earlier in drawdowns
    And smaller adverse moves trigger the stop

  @no_entry_on_gap_down
  Scenario: No entry when market gaps down
    Given a Larry Williams strategy with range multiplier 0.5 and ATR stop 2.0 over 10 bars
    When today's high is below prior day's breakout level
    Then no entry signal is generated
    And the strategy waits for next breakout opportunity
