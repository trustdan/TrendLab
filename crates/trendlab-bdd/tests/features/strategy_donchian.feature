@strategy @donchian
Feature: Donchian breakout strategy behavior
  The Donchian breakout strategy enters long when close breaks above the upper channel
  and exits when close breaks below the lower channel (or a shorter exit channel).

  Scenario: Long entry triggers on breakout above upper channel
    Given a synthetic bar series from fixture synth/donchian_breakout.csv
    And a Donchian breakout strategy with entry lookback 5 and exit lookback 3
    When I run the strategy
    Then a long entry signal must occur at index 7
    And the entry fill must be at index 8 open price

  Scenario: Exit triggers when close breaks below exit channel
    Given a synthetic bar series from fixture synth/donchian_breakout.csv
    And a Donchian breakout strategy with entry lookback 5 and exit lookback 3
    When I run the strategy
    Then an exit signal must occur when close breaks the exit channel
    And the trade must be closed

  Scenario: No signal during warmup period
    Given a synthetic bar series from fixture synth/donchian_breakout.csv
    And a Donchian breakout strategy with entry lookback 5 and exit lookback 3
    When I run the strategy
    Then no entry signal occurs before index 5

  @determinism
  Scenario: Donchian strategy results are deterministic
    Given a synthetic bar series from fixture synth/donchian_breakout.csv
    And a Donchian breakout strategy with entry lookback 5 and exit lookback 3
    When I run the strategy twice
    Then the two results must be identical
