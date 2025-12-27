@strategy @short @donchian
Feature: Short selling strategy behavior
  The system supports short selling when TradingMode is ShortOnly or LongShort.
  Short entries trigger when price breaks below the Donchian lower channel.
  Short exits trigger when price breaks above the exit upper channel.

  Background:
    Given a synthetic bar series from fixture synth/donchian_short_breakout.csv

  Scenario: Short entry triggers on breakdown below lower channel
    Given a Donchian breakout strategy in short mode with entry lookback 5 and exit lookback 3
    When I run the short strategy
    Then a short entry signal must occur at index 6
    And the short entry fill must be at index 6 open price

  Scenario: Short exit triggers when price breaks above exit channel
    Given a Donchian breakout strategy in short mode with entry lookback 5 and exit lookback 3
    When I run the short strategy
    Then a short exit signal must occur when close breaks the exit upper channel
    And the short position must be closed

  Scenario: No short signal during warmup period
    Given a Donchian breakout strategy in short mode with entry lookback 5 and exit lookback 3
    When I run the short strategy
    Then no short entry signal occurs before index 5

  Scenario: Short position has negative quantity
    Given a Donchian breakout strategy in short mode with entry lookback 5 and exit lookback 3
    When I run the short strategy
    Then when in short position the position quantity must be negative

  Scenario: Short trades profit in downtrend
    Given a Donchian breakout strategy in short mode with entry lookback 5 and exit lookback 3
    When I run the short strategy
    Then the total return must be positive
    And the short strategy should profit from falling prices

  @determinism
  Scenario: Short strategy results are deterministic
    Given a Donchian breakout strategy in short mode with entry lookback 5 and exit lookback 3
    When I run the short strategy twice
    Then the two short results must be identical

  @longshort
  Scenario: LongShort mode can take both long and short positions
    Given a Donchian breakout strategy in longshort mode with entry lookback 5 and exit lookback 3
    When I run the longshort strategy
    Then position state can be -1, 0, or 1
