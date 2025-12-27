@data
Feature: Data normalization and quality rules
  The data layer must detect quality issues and normalize raw provider data
  into a canonical Parquet format without inventing or interpolating data.

  Background:
    Given the data quality checker is initialized

  @duplicates
  Scenario: Duplicate timestamps are detected and reported
    Given a raw dataset with duplicate bars:
      | ts                   | symbol | open  | close |
      | 2024-01-02T00:00:00Z | TEST   | 100.0 | 101.0 |
      | 2024-01-02T00:00:00Z | TEST   | 100.5 | 101.5 |
      | 2024-01-03T00:00:00Z | TEST   | 101.0 | 102.0 |
    When I run the data quality check
    Then the report must show duplicate_count equal to 1
    And the report must list the duplicate timestamp "2024-01-02T00:00:00Z"

  @gaps
  Scenario: Missing trading days are detected as gaps, not interpolated
    Given a raw dataset with a gap:
      | ts                   | symbol | open  | close |
      | 2024-01-02T00:00:00Z | TEST   | 100.0 | 101.0 |
      | 2024-01-03T00:00:00Z | TEST   | 101.0 | 102.0 |
      | 2024-01-05T00:00:00Z | TEST   | 103.0 | 104.0 |
    When I run normalization
    Then the normalized output must have exactly 3 bars
    And the data quality report must show gap_count equal to 1

  @ordering
  Scenario: Out-of-order timestamps are detected
    Given a raw dataset with out-of-order bars:
      | ts                   | symbol | open  | close |
      | 2024-01-03T00:00:00Z | TEST   | 102.0 | 103.0 |
      | 2024-01-02T00:00:00Z | TEST   | 100.0 | 101.0 |
      | 2024-01-04T00:00:00Z | TEST   | 103.0 | 104.0 |
    When I run the data quality check
    Then the report must show out_of_order_count equal to 1

  @ohlc_validity
  Scenario: Invalid OHLC relationships are flagged
    Given a raw dataset with invalid OHLC:
      | ts                   | symbol | open  | high  | low   | close |
      | 2024-01-02T00:00:00Z | TEST   | 100.0 | 105.0 | 99.0  | 103.0 |
      | 2024-01-03T00:00:00Z | TEST   | 101.0 | 100.0 | 99.0  | 102.0 |
    When I run the data quality check
    Then the report must show invalid_ohlc_count equal to 1
    And the invalid bar must be at "2024-01-03T00:00:00Z" with reason "high < open"

  @idempotent
  Scenario: Re-running normalization produces identical output
    Given fixture synth/determinism_30.csv as raw input
    When I run normalization
    And I run normalization again
    Then the two normalized outputs must be byte-identical

  @schema
  Scenario: Normalized output matches canonical schema
    Given fixture synth/determinism_30.csv as raw input
    When I run normalization
    Then the output Parquet must have columns:
      | column    | dtype     |
      | ts        | datetime  |
      | open      | float64   |
      | high      | float64   |
      | low       | float64   |
      | close     | float64   |
      | volume    | float64   |
      | symbol    | utf8      |
      | timeframe | utf8      |
