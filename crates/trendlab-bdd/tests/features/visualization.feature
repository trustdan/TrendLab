@visualization
Feature: Visualization and reporting infrastructure
  As a researcher, I want rich visualizations of backtest results
  so I can analyze performance and share findings effectively.

  Background:
    Given a synthetic price series with 100 bars
    And a completed sweep with multiple configurations

  @report @html
  Scenario: HTML report contains essential sections
    Given a completed sweep run with run_id "viz_test_run"
    When I generate an HTML report
    Then the report must include a summary section
    And the report must include an equity chart section
    And the report must include a trades table section
    And the report must include a metrics summary section

  @report @html @standalone
  Scenario: HTML report is self-contained
    Given a completed sweep run
    When I generate an HTML report
    Then the HTML file must contain inline CSS
    And the HTML file must contain inline JavaScript
    And the report must render correctly without external dependencies

  @report @html @charts
  Scenario: HTML report equity chart shows correct data
    Given a backtest with known equity curve
    When I generate an HTML report
    Then the equity chart must plot each equity point
    And the chart must show drawdown periods highlighted
    And the chart x-axis must show dates

  @report @html @trades
  Scenario: HTML report trades table is complete
    Given a backtest with 5 trades
    When I generate an HTML report
    Then the trades table must list all 5 trades
    And each trade must show entry date and price
    And each trade must show exit date and price
    And each trade must show PnL and fees

  @terminal @tables
  Scenario: Terminal output uses formatted tables
    Given a completed sweep run
    When I display results in the terminal
    Then metrics should be displayed in aligned columns
    And numerical values should be right-aligned
    And headers should be clearly distinguished

  @terminal @summary
  Scenario: Terminal summary shows key metrics prominently
    Given a completed sweep with a winning configuration
    When I display the summary
    Then total return should be displayed with color coding
    And max drawdown should be displayed with color coding
    And Sharpe ratio should be displayed with color coding

  @terminal @charts
  Scenario: Inline terminal chart renders equity curve
    Given a backtest with known equity curve
    When I render an inline terminal chart
    Then the chart should show equity progression using block characters
    And the chart should fit within 80 columns
    And the chart should show min and max values on y-axis

  @terminal @charts @sparkline
  Scenario: Sparkline shows quick equity summary
    Given a backtest with an equity curve
    When I render a sparkline
    Then it should show the general trend in a compact format
    And it should fit on a single line
    And it should use unicode block characters for height encoding

  @cli @report
  Scenario: CLI report command generates HTML
    Given a completed sweep run with run_id "report_test"
    When I run "trendlab report html --run-id report_test"
    Then the command should succeed
    And the output file should exist at "reports/report_test/report.html"
    And the file should be valid HTML

  @cli @report @open
  Scenario: CLI report command can auto-open browser
    Given a completed sweep run
    When I run "trendlab report html --run-id test --open"
    Then the command should generate the report
    And it should attempt to open the default browser
