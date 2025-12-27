@strategy @bollinger @squeeze
Feature: Bollinger Squeeze Breakout strategy
  Volatility contraction/expansion breakout strategy using Bollinger Bands.

  Bollinger Bands:
  - Middle = SMA(close, period)
  - Upper = Middle + multiplier * std_dev
  - Lower = Middle - multiplier * std_dev
  - Bandwidth = (Upper - Lower) / Middle

  Squeeze Detection:
  - When bandwidth drops below threshold, volatility is contracting (squeeze)
  - Breakout from squeeze often leads to significant moves

  Entry: In squeeze state AND close breaks above upper band
  Exit: Close falls below middle band (take profit / stop out)

  Background:
    Given a synthetic bar series from fixture synth/bollinger_squeeze_50.csv

  @squeeze_detection
  Scenario: Squeeze detected when bandwidth narrows
    Given a Bollinger Squeeze strategy with period 20, std_mult 2.0, threshold 0.04
    When I compute Bollinger bandwidth
    Then squeeze should be detected when bandwidth < 0.04
    And this indicates low volatility consolidation

  @entry @breakout
  Scenario: Entry triggers on breakout from squeeze
    Given a Bollinger Squeeze strategy with period 20, std_mult 2.0, threshold 0.04
    When I run the strategy
    Then a long entry signal must occur when:
      | condition | value |
      | in_squeeze | true |
      | close > upper_band | true |

  @no_entry_without_squeeze
  Scenario: No entry when breaking upper band without prior squeeze
    Given a Bollinger Squeeze strategy with period 20, std_mult 2.0, threshold 0.01
    When I run the strategy on volatile data
    Then no entry signal should occur without squeeze condition

  @exit @below_middle
  Scenario: Exit triggers when price falls below middle band
    Given a Bollinger Squeeze strategy with period 20, std_mult 2.0, threshold 0.04
    When I run the strategy
    Then an exit signal must occur when close < middle band

  @warmup
  Scenario: Warmup period equals the lookback period
    Given a Bollinger Squeeze strategy with period 20, std_mult 2.0, threshold 0.04
    Then the warmup period must be 20 bars

  @determinism
  Scenario: Bollinger Squeeze results are deterministic
    Given a Bollinger Squeeze strategy with period 20, std_mult 2.0, threshold 0.04
    When I run the strategy twice
    Then the two results must be identical

  @band_calculation
  Scenario: Bollinger Bands are calculated correctly
    Given a Bollinger Squeeze strategy with period 20, std_mult 2.0, threshold 0.04
    When I compute Bollinger Bands
    Then upper band must equal middle + 2.0 * std_dev
    And lower band must equal middle - 2.0 * std_dev
    And bandwidth must equal (upper - lower) / middle

  @multiplier_effect
  Scenario: Higher multiplier creates wider bands
    Given a Bollinger Squeeze strategy with period 20, std_mult 3.0, threshold 0.04
    When I compute Bollinger Bands
    Then the bands should be wider than with std_mult 2.0
    And fewer squeeze conditions should be detected
