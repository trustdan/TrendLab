@strategy @aroon
Feature: Aroon Cross strategy
  Tushar Chande's Aroon oscillator-based trend following system.
  Entry: Aroon-Up crosses above Aroon-Down (bullish trend emerging)
  Exit: Aroon-Up crosses below Aroon-Down (bearish trend emerging)

  Aroon indicator formula:
  - Aroon-Up = 100 * (period - bars_since_period_high) / period
  - Aroon-Down = 100 * (period - bars_since_period_low) / period
  - Aroon Oscillator = Aroon-Up - Aroon-Down

  When Aroon-Up is 100: just made a new period high
  When Aroon-Down is 100: just made a new period low

  Background:
    Given a synthetic bar series from fixture synth/aroon_30.csv

  @entry @aroon_cross
  Scenario: Entry triggers when Aroon-Up crosses above Aroon-Down
    Given an Aroon strategy with period 10
    When I run the strategy
    Then a long entry signal must occur
    And Aroon-Up must be above Aroon-Down at entry

  @exit @aroon_cross_down
  Scenario: Exit triggers when Aroon-Up crosses below Aroon-Down
    Given an Aroon strategy with period 10
    When I run the strategy
    Then an exit signal must occur when Aroon-Up crosses below Aroon-Down

  @warmup
  Scenario: Warmup period equals the lookback period
    Given an Aroon strategy with period 25
    Then the warmup period must be 25 bars

  @extreme_readings
  Scenario: Aroon-Up is 100 when at period high
    Given an Aroon strategy with period 10
    When I compute Aroon indicators at the period high
    Then Aroon-Up should be 100
    And this indicates the strongest possible uptrend signal

  @extreme_readings_down
  Scenario: Aroon-Down is 100 when at period low
    Given an Aroon strategy with period 10
    When I compute Aroon indicators at the period low
    Then Aroon-Down should be 100
    And this indicates the strongest possible downtrend signal

  @oscillator
  Scenario: Aroon oscillator ranges from -100 to +100
    Given an Aroon strategy with period 10
    When I compute Aroon indicators
    Then the Aroon oscillator must be between -100 and 100
    And positive oscillator indicates bullish momentum

  @determinism
  Scenario: Aroon results are deterministic
    Given an Aroon strategy with period 10
    When I run the strategy twice
    Then the two results must be identical

  @crossover_detection
  Scenario: Crossover detection requires actual crossing
    Given an Aroon strategy with period 10
    When Aroon-Up equals Aroon-Down
    Then no signal is generated until Aroon-Up exceeds Aroon-Down
