@strategy @cci @oscillator
Feature: CCI strategy
  Donald Lambert's Commodity Channel Index trend breakout strategy.
  Entry: CCI crosses above +100 (bullish breakout)
  Exit: CCI crosses below -100 (bearish breakdown)

  CCI formula:
  - Typical Price = (High + Low + Close) / 3
  - Mean Deviation = average of |TP - SMA(TP)|
  - CCI = (TP - SMA(TP)) / (0.015 * Mean Deviation)

  CCI has no fixed bounds but typically oscillates between -200 and +200

  Background:
    Given a synthetic bar series from fixture synth/oscillator_30.csv

  @entry @cci_breakout
  Scenario: Entry triggers when CCI crosses above entry threshold
    Given a CCI strategy with period 20, entry threshold 100, exit threshold -100
    When I run the strategy
    Then a long entry signal must occur
    And CCI must cross above 100 at entry

  @exit @cci_breakdown
  Scenario: Exit triggers when CCI crosses below exit threshold
    Given a CCI strategy with period 20, entry threshold 100, exit threshold -100
    When I run the strategy
    Then an exit signal must occur when CCI crosses below -100

  @warmup
  Scenario: Warmup period equals the lookback period
    Given a CCI strategy with period 20, entry threshold 100, exit threshold -100
    Then the warmup period must be 20 bars

  @determinism
  Scenario: CCI results are deterministic
    Given a CCI strategy with period 20, entry threshold 100, exit threshold -100
    When I run the strategy twice
    Then the two results must be identical

  @no_bounds
  Scenario: CCI can exceed typical +/- 200 range
    Given a CCI strategy with period 20, entry threshold 100, exit threshold -100
    When I compute CCI on volatile data
    Then CCI values outside -200 to +200 are valid
