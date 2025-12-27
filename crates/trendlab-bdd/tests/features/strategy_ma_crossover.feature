@strategy @ma_crossover
Feature: Moving Average Crossover strategy
  Classic trend-following strategy using the crossover of two moving averages.
  Entry: Fast MA crosses above slow MA (golden cross)
  Exit: Fast MA crosses below slow MA (death cross)

  Supports both SMA and EMA variants:
  - SMA 50/200: Classic "golden cross / death cross"
  - EMA 12/26: Popular short-term crossover (MACD-style)
  - SMA 10/50: Medium-term trend following

  Background:
    Given a synthetic bar series from fixture synth/ma_crossover_25.csv

  @entry @golden_cross
  Scenario: Entry triggers on golden cross (fast MA crosses above slow MA)
    Given an MA crossover strategy with fast period 5 and slow period 10 using SMA
    When I run the strategy
    Then a long entry signal must occur at index 15
    And the entry was triggered because fast MA 107.0 crossed above slow MA 105.5

  @exit @death_cross
  Scenario: Exit triggers on death cross (fast MA crosses below slow MA)
    Given an MA crossover strategy with fast period 5 and slow period 10 using SMA
    When I run the strategy
    Then an exit signal must occur at index 22
    And the exit was triggered because fast MA 107.2 crossed below slow MA 109.2

  @warmup
  Scenario: No signals during warmup period
    Given an MA crossover strategy with fast period 5 and slow period 10 using SMA
    When I run the strategy
    Then no entry signal occurs before index 10
    And the warmup period must be 10 bars

  @complete_trade
  Scenario: Complete round-trip trade from entry to exit
    Given an MA crossover strategy with fast period 5 and slow period 10 using SMA
    And fees are set to 0 bps per side
    When I run the strategy with backtest
    Then a complete trade must occur from index 15 to index 22
    And the entry fill must be at index 16 open price
    And the exit fill must be at index 23 open price

  @determinism
  Scenario: MA crossover results are deterministic
    Given an MA crossover strategy with fast period 5 and slow period 10 using SMA
    When I run the strategy twice
    Then the two results must be identical

  @ema
  Scenario: EMA crossover uses exponential weighting
    Given an MA crossover strategy with fast period 5 and slow period 10 using EMA
    When I compute the moving averages at index 15
    Then the EMA values differ from SMA values
    And the EMA responds faster to recent price changes

  @preset_sma_50_200
  Scenario: Golden cross preset uses SMA 50/200
    Given the golden cross 50/200 preset strategy
    Then the fast period must be 50
    And the slow period must be 200
    And the MA type must be SMA

  @preset_ema_12_26
  Scenario: MACD-style preset uses EMA 12/26
    Given the MACD-style 12/26 preset strategy
    Then the fast period must be 12
    And the slow period must be 26
    And the MA type must be EMA

  @no_signal_when_flat
  Scenario: No entry signal when MAs are parallel (not crossing)
    Given an MA crossover strategy with fast period 5 and slow period 10 using SMA
    When I check signals during steady uptrend at indices 11-14
    Then all signals should be Hold because no crossover occurred

  @crossover_detection
  Scenario: Crossover detection requires actual crossing
    Given an MA crossover strategy with fast period 5 and slow period 10 using SMA
    When fast MA equals slow MA at index 14
    Then no signal is generated until fast MA exceeds slow MA
