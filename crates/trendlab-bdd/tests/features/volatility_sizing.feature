@sizing @volatility
Feature: Volatility-based position sizing

  Volatility sizing adjusts position size inversely proportional to recent volatility.
  This is a core Turtle trading concept: take smaller positions in volatile markets,
  larger positions in calm markets, targeting a consistent dollar volatility per trade.

  Formula: Position Size = Target Volatility (in $) / (ATR × Point Value)
  Or simplified: Units = (Account × Risk%) / (ATR × Price)

  Background:
    Given a target volatility of 1000 dollars per day
    And an account size of 100000 dollars

  @atr
  Scenario: ATR calculation uses true range correctly
    Given a fixture with bars demonstrating high/low/close relationships
    When I compute ATR with period 3
    Then ATR at each bar equals the average of the last 3 true ranges
    And true range considers gaps from previous close

  @sizing_basic
  Scenario: Position size is inversely proportional to ATR
    Given a bar with ATR of 2.0 and price of 100.0
    When I compute volatility-sized position
    Then position size equals target_volatility / (ATR × price)
    And the position size is 5.0 units

  @sizing_high_vol
  Scenario: Higher volatility results in smaller position
    Given bar A with ATR of 1.0 and price of 100.0
    And bar B with ATR of 2.0 and price of 100.0
    When I compute positions for both using volatility sizing
    Then position for bar B is half the size of bar A

  @sizing_low_vol
  Scenario: Lower volatility results in larger position
    Given bar A with ATR of 4.0 and price of 100.0
    And bar B with ATR of 2.0 and price of 100.0
    When I compute positions for both using volatility sizing
    Then position for bar B is double the size of bar A

  @sizing_price_adjusted
  Scenario: Position size accounts for price differences
    Given bar A with ATR of 2.0 and price of 50.0
    And bar B with ATR of 2.0 and price of 100.0
    When I compute positions for both using volatility sizing
    Then position for bar A is double the units of bar B
    And both positions have equal dollar volatility

  @sizing_warmup
  Scenario: No position sizing during ATR warmup period
    Given an ATR period of 14
    And fewer than 14 bars of history
    When I request position size
    Then sizing returns None until warmup is complete

  @sizing_minimum
  Scenario: Position size respects minimum size constraint
    Given a minimum position size of 1 unit
    And a bar with very high ATR of 50.0 and price of 100.0
    When volatility sizing computes less than 1 unit
    Then position size is clamped to 1 unit

  @sizing_maximum
  Scenario: Position size respects maximum size constraint
    Given a maximum position size of 100 units
    And a bar with very low ATR of 0.1 and price of 100.0
    When volatility sizing computes more than 100 units
    Then position size is clamped to 100 units

  @sizing_backtest
  Scenario: Volatility sizing integrates with backtest engine
    Given a Donchian breakout strategy with volatility sizing
    And a fixture with varying volatility
    When I run a backtest
    Then each trade has a different position size based on ATR at entry
    And high volatility periods have smaller positions

  @turtle_n
  Scenario: Turtle N calculation matches specification
    Given the Turtle trading system formula
    When I compute position size with:
      | account_size | 100000 |
      | risk_percent | 1 |
      | atr | 2.5 |
      | price | 50.0 |
    Then position size equals (account × risk%) / (ATR × price)
    And the result is 8 units

  @determinism
  Scenario: Volatility sizing is deterministic
    Given the same bars and configuration
    When I compute position size twice
    Then both results are identical
