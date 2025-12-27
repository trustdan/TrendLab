@pyramiding @position_management
Feature: Pyramiding (adding to winning positions)
  Pyramiding allows adding to winning positions at fixed intervals.
  Based on Turtle trading system conventions:
  - Add 1/2 N (half ATR) after each entry
  - Maximum 4 units per position
  - All units exit together when exit signal triggers

  This feature tests position scaling behavior, not entry/exit signal logic.
  Entry/exit signals come from the underlying strategy (e.g., Donchian breakout).

  Background:
    Given a synthetic bar series from fixture synth/pyramid_40.csv

  @initial_entry
  Scenario: Initial entry creates first unit
    Given a Donchian strategy with 10/5 lookback and pyramiding enabled
    And max units is 4
    And pyramid threshold is 0.5 ATR
    When I run the backtest
    Then the first fill must be a buy for 1 unit
    And position after first entry must be 1 unit

  @add_unit
  Scenario: Add unit when price moves by pyramid threshold
    Given a Donchian strategy with 10/5 lookback and pyramiding enabled
    And max units is 4
    And pyramid threshold is 0.5 ATR
    And ATR at entry is 2.0
    When I run the backtest
    And price moves up by 1.0 after entry
    Then a pyramid add must occur
    And position must reach at least 2 units

  @max_units_limit
  Scenario: Cannot exceed maximum units
    Given a Donchian strategy with 10/5 lookback and pyramiding enabled
    And max units is 4
    And pyramid threshold is 0.5 ATR
    When I run the backtest
    And price continues rising allowing 5 pyramid adds
    Then position must never exceed 4 units
    And total buy fills must be 4

  @all_units_exit
  Scenario: All units exit together on exit signal
    Given a Donchian strategy with 10/5 lookback and pyramiding enabled
    And max units is 4
    When I run the backtest
    Then position must reach at least 3 units
    And position must be flat after exit

  @pyramid_spacing
  Scenario: Pyramid adds must be spaced by threshold
    Given a Donchian strategy with 10/5 lookback and pyramiding enabled
    And max units is 4
    And pyramid threshold is 0.5 ATR
    And ATR at entry is 4.0
    When I run the backtest
    Then pyramid adds must be at least 2.0 price units apart

  @average_entry_price
  Scenario: Average entry price tracks all pyramid fills
    Given a Donchian strategy with 10/5 lookback and pyramiding enabled
    And max units is 4
    When I run the backtest
    Then pyramid trades must have an average entry price
    And exit PnL must use average entry price

  @no_pyramid_without_position
  Scenario: No pyramid adds without an open position
    Given a Donchian strategy with 10/5 lookback and pyramiding enabled
    And max units is 4
    When I run the backtest
    Then pyramid adds only occur after initial entry

  @pyramid_warmup
  Scenario: Pyramiding respects ATR warmup period
    Given a Donchian strategy with 10/5 lookback and pyramiding enabled
    And ATR period is 14
    And max units is 4
    When entry occurs before ATR warmup completes
    Then pyramid adds use default threshold until ATR is available

  @determinism
  Scenario: Pyramiding results are deterministic
    Given a Donchian strategy with 10/5 lookback and pyramiding enabled
    And max units is 4
    When I run the backtest twice with identical configuration
    Then the results must be identical

  @pnl_accounting
  Scenario: PnL accounting handles multiple entry prices correctly
    Given a Donchian strategy with 10/5 lookback and pyramiding enabled
    And max units is 4
    When I run the backtest
    Then gross PnL must account for each pyramid entry
    And fees must be calculated on each fill individually

  @turtle_system_1_pyramid
  Scenario: Turtle System 1 with pyramiding preset
    Given Turtle System 1 strategy with pyramiding enabled
    Then max units must be 4
    And pyramid threshold must be 0.5 ATR
    And ATR period must be 20

  @disabled_pyramiding
  Scenario: Pyramiding can be disabled
    Given a Donchian strategy with 10/5 lookback
    And pyramiding is disabled
    When I run the backtest
    And price moves favorably by multiple thresholds
    Then only the initial entry occurs
    And position must be 1 unit throughout

