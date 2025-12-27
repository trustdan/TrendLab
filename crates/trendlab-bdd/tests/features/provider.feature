@provider
Feature: Data provider trait and Yahoo Finance integration
  The provider layer fetches raw market data from external sources,
  caches responses, and converts them to canonical Bar format.

  Background:
    Given the provider subsystem is initialized

  # ============================================================================
  # Yahoo CSV Parsing
  # ============================================================================

  @yahoo @parsing
  Scenario: Yahoo CSV response is parsed into Bar structs
    Given a Yahoo Finance CSV response:
      """
      Date,Open,High,Low,Close,Adj Close,Volume
      2024-01-02,100.00,102.50,99.50,101.00,101.00,1000000
      2024-01-03,101.50,103.00,100.00,102.50,102.50,1200000
      2024-01-04,102.00,104.00,101.00,103.00,103.00,800000
      """
    When I parse the response for symbol "TEST" with timeframe "1d"
    Then I should have 3 bars
    And bar 0 should have open 100.00 and close 101.00
    And bar 1 should have open 101.50 and close 102.50
    And bar 2 should have open 102.00 and close 103.00

  @yahoo @parsing
  Scenario: Yahoo CSV uses adjusted close for close field
    Given a Yahoo Finance CSV response:
      """
      Date,Open,High,Low,Close,Adj Close,Volume
      2024-01-02,100.00,102.50,99.50,101.00,50.50,1000000
      """
    When I parse the response for symbol "SPLIT" with timeframe "1d"
    Then bar 0 should have close 50.50

  @yahoo @parsing
  Scenario: Empty Yahoo response produces zero bars
    Given a Yahoo Finance CSV response:
      """
      Date,Open,High,Low,Close,Adj Close,Volume
      """
    When I parse the response for symbol "EMPTY" with timeframe "1d"
    Then I should have 0 bars

  @yahoo @parsing
  Scenario: Yahoo CSV with null values is handled gracefully
    Given a Yahoo Finance CSV response:
      """
      Date,Open,High,Low,Close,Adj Close,Volume
      2024-01-02,100.00,102.50,99.50,101.00,101.00,1000000
      2024-01-03,null,null,null,null,null,null
      2024-01-04,102.00,104.00,101.00,103.00,103.00,800000
      """
    When I parse the response for symbol "GAPS" with timeframe "1d"
    Then I should have 2 bars
    And bar 0 should have date "2024-01-02"
    And bar 1 should have date "2024-01-04"

  # ============================================================================
  # Raw Cache
  # ============================================================================

  @cache
  Scenario: Raw response is cached with metadata sidecar
    Given a Yahoo Finance CSV response:
      """
      Date,Open,High,Low,Close,Adj Close,Volume
      2024-01-02,100.00,102.50,99.50,101.00,101.00,1000000
      """
    When I cache the response for symbol "CACHE" from "2024-01-01" to "2024-01-31"
    Then the cache file should exist at "yahoo/CACHE/2024-01-01_2024-01-31.csv"
    And the metadata sidecar should exist at "yahoo/CACHE/2024-01-01_2024-01-31.meta.json"
    And the metadata should contain "fetched_at" timestamp
    And the metadata should contain "row_count" equal to 1

  @cache
  Scenario: Cached data is returned without re-fetch
    Given a cached response exists for symbol "CACHED" from "2024-01-01" to "2024-01-31"
    When I request data for symbol "CACHED" from "2024-01-01" to "2024-01-31"
    Then the data should be loaded from cache
    And no HTTP request should be made

  @cache @force
  Scenario: Force flag bypasses cache
    Given a cached response exists for symbol "FORCE" from "2024-01-01" to "2024-01-31"
    When I request data for symbol "FORCE" from "2024-01-01" to "2024-01-31" with force flag
    Then the cache should be invalidated
    And the request should fetch fresh data

  # ============================================================================
  # Normalization to Parquet
  # ============================================================================

  @parquet
  Scenario: Parsed bars are normalized to Parquet
    Given a Yahoo Finance CSV response:
      """
      Date,Open,High,Low,Close,Adj Close,Volume
      2024-01-02,100.00,102.50,99.50,101.00,101.00,1000000
      2024-01-03,101.50,103.00,100.00,102.50,102.50,1200000
      """
    When I parse the response for symbol "PARQ" with timeframe "1d"
    And I write the bars to Parquet
    Then the Parquet file should exist at "1d/symbol=PARQ/year=2024/data.parquet"
    And reading the Parquet should return 2 bars matching the original

  @parquet @partitions
  Scenario: Multi-year data is partitioned correctly
    Given bars spanning multiple years:
      | ts                   | symbol | open  | close |
      | 2023-12-29T00:00:00Z | MULTI  | 98.0  | 99.0  |
      | 2024-01-02T00:00:00Z | MULTI  | 100.0 | 101.0 |
      | 2024-01-03T00:00:00Z | MULTI  | 101.0 | 102.0 |
    When I write the bars to Parquet
    Then Parquet partition "1d/symbol=MULTI/year=2023/data.parquet" should have 1 bar
    And Parquet partition "1d/symbol=MULTI/year=2024/data.parquet" should have 2 bars

  # ============================================================================
  # Provider Error Handling
  # ============================================================================

  @errors
  Scenario: Invalid symbol returns appropriate error
    Given no data exists for symbol "INVALID_XYZ123"
    When I request data for symbol "INVALID_XYZ123"
    Then I should receive a "symbol not found" error

  @errors
  Scenario: Malformed CSV produces parse error
    Given a malformed Yahoo response:
      """
      Date,Open,High
      2024-01-02,not_a_number,102.50
      """
    When I attempt to parse the response
    Then I should receive a "parse error"
