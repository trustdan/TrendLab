@strategy @ichimoku @japanese
Feature: Ichimoku Cloud strategy
  Goichi Hosoda's Ichimoku Kinko Hyo trend following system.
  Entry: Price above cloud AND Tenkan-sen crosses above Kijun-sen
  Exit: Price below cloud OR Tenkan-sen crosses below Kijun-sen

  Ichimoku components (standard periods 9/26/52):
  - Tenkan-sen (Conversion Line): (9-period high + 9-period low) / 2
  - Kijun-sen (Base Line): (26-period high + 26-period low) / 2
  - Senkou Span A (Leading Span A): (Tenkan + Kijun) / 2, plotted 26 periods ahead
  - Senkou Span B (Leading Span B): (52-period high + 52-period low) / 2, plotted 26 periods ahead
  - Chikou Span (Lagging Span): Close plotted 26 periods back

  The cloud (Kumo) is the area between Senkou Span A and B.

  Background:
    Given a synthetic bar series from fixture synth/trending_100.csv

  @entry @cloud_breakout
  Scenario: Entry triggers on TK cross above cloud
    Given an Ichimoku strategy with tenkan 9, kijun 26, senkou_b 52
    When I run the strategy
    Then a long entry signal must occur
    And price must be above the cloud at entry
    And Tenkan-sen must cross above Kijun-sen at entry

  @exit @cloud_breakdown
  Scenario: Exit triggers when price falls below cloud
    Given an Ichimoku strategy with tenkan 9, kijun 26, senkou_b 52
    When I run the strategy
    Then an exit signal must occur when price falls below the cloud

  @exit @tk_cross_down
  Scenario: Exit also triggers when Tenkan crosses below Kijun
    Given an Ichimoku strategy with tenkan 9, kijun 26, senkou_b 52
    When I run the strategy
    Then an exit signal can occur when Tenkan crosses below Kijun

  @warmup
  Scenario: Warmup period equals senkou_b period
    Given an Ichimoku strategy with tenkan 9, kijun 26, senkou_b 52
    Then the warmup period must be 52 bars

  @cloud_definition
  Scenario: Cloud is between Senkou Span A and B
    Given an Ichimoku strategy with tenkan 9, kijun 26, senkou_b 52
    When I compute Ichimoku indicators
    Then cloud top is max of Span A and Span B
    And cloud bottom is min of Span A and Span B

  @determinism
  Scenario: Ichimoku results are deterministic
    Given an Ichimoku strategy with tenkan 9, kijun 26, senkou_b 52
    When I run the strategy twice
    Then the two results must be identical

  @bullish_cloud
  Scenario: Bullish cloud when Span A above Span B
    Given an Ichimoku strategy with tenkan 9, kijun 26, senkou_b 52
    When Senkou Span A is above Senkou Span B
    Then the cloud is bullish (green)
    And trend is considered up

  @bearish_cloud
  Scenario: Bearish cloud when Span B above Span A
    Given an Ichimoku strategy with tenkan 9, kijun 26, senkou_b 52
    When Senkou Span B is above Senkou Span A
    Then the cloud is bearish (red)
    And trend is considered down
