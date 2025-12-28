import { useEffect, useCallback } from 'react';
import { useShallow } from 'zustand/react/shallow';
import { useAppStore } from '../../store';
import {
  CandlestickChart,
  EquityChart,
  MultiSeriesChart,
  ChartControls,
  TradesTable,
} from '../charts';
import { useKeyboardNavigation, type KeyboardAction } from '../../hooks';
import type { ChartMode, ChartOverlays } from '../../types';
import styles from './ChartPanel.module.css';

export function ChartPanel() {

  const {
    chartMode,
    chartSymbol,
    chartOverlays,
    chartData,
    chartLoading,
    chartError,
    setChartMode,
    cycleChartMode,
    toggleOverlay,
    loadChartState,
    loadChartData,
    clearChartError,
    // From results slice - for showing what's selected
    selectedResult,
    results,
    activePanel,
  } = useAppStore(
    useShallow((state) => ({
      chartMode: state.chartMode,
      chartSymbol: state.chartSymbol,
      chartOverlays: state.chartOverlays,
      chartData: state.chartData,
      chartLoading: state.chartLoading,
      chartError: state.chartError,
      setChartMode: state.setChartMode,
      cycleChartMode: state.cycleChartMode,
      toggleOverlay: state.toggleOverlay,
      loadChartState: state.loadChartState,
      loadChartData: state.loadChartData,
      clearChartError: state.clearChartError,
      selectedResult: state.selectedResultId,
      results: state.results,
      activePanel: state.activePanel,
    }))
  );

  // Handle keyboard actions for Chart panel
  const handleAction = useCallback(
    (action: KeyboardAction) => {
      if (activePanel !== 'chart') return;

      switch (action.type) {
        case 'toggle_chart_mode':
          cycleChartMode();
          break;
        case 'toggle_drawdown':
          toggleOverlay('drawdown');
          break;
        case 'toggle_volume':
          toggleOverlay('volume');
          break;
        case 'toggle_crosshair':
          toggleOverlay('crosshair');
          break;
      }
    },
    [activePanel, cycleChartMode, toggleOverlay]
  );

  useKeyboardNavigation(handleAction);

  // Load chart state and data on mount (sequential to avoid race condition)
  useEffect(() => {
    let cancelled = false;
    const init = async () => {
      try {
        await loadChartState();
        if (!cancelled) {
          await loadChartData();
        }
      } catch (error) {
        console.error('[ChartPanel] Init error:', error);
      }
    };
    init();
    return () => {
      cancelled = true;
    };
  }, [loadChartState, loadChartData]);

  // Reload chart data when mode or selection changes (after initial load)
  useEffect(() => {
    // Skip on first render - handled by init above
    const isInitialRender = chartData === null && !chartLoading && !chartError;
    if (isInitialRender) return;
    loadChartData();
  }, [chartMode, chartSymbol]); // eslint-disable-line react-hooks/exhaustive-deps

  // Handle mode change
  const handleModeChange = useCallback(
    (mode: ChartMode) => {
      setChartMode(mode);
    },
    [setChartMode]
  );

  // Handle overlay toggle
  const handleOverlayToggle = useCallback(
    (overlay: keyof ChartOverlays) => {
      toggleOverlay(overlay);
    },
    [toggleOverlay]
  );

  // Get selected result info for display
  const selectedResultData = results.find((r) => r.id === selectedResult);

  // Determine what to show based on mode
  const getChartTitle = () => {
    switch (chartMode) {
      case 'candlestick':
        return chartSymbol ? `${chartSymbol} - Price` : 'Select a symbol';
      case 'equity':
        return selectedResultData
          ? `${selectedResultData.symbol} - ${selectedResultData.strategy}`
          : 'Select a result';
      case 'multi_ticker':
        return 'Multi-Ticker Comparison';
      case 'portfolio':
        return 'Portfolio Equity';
      case 'strategy_comparison':
        return 'Strategy Comparison';
      default:
        return 'Chart';
    }
  };

  // Render the appropriate chart based on mode
  const renderChart = () => {
    if (chartError) {
      return (
        <div className={styles.error}>
          <span>Error: {chartError}</span>
          <button onClick={clearChartError} className={styles.dismissBtn}>
            Dismiss
          </button>
        </div>
      );
    }

    if (!chartData && !chartLoading) {
      return (
        <div className={styles.empty}>
          <p>
            {chartMode === 'candlestick' && !chartSymbol
              ? 'Select a symbol from the Data panel to view price chart'
              : chartMode === 'equity' && !selectedResult
                ? 'Select a result from the Results panel to view equity curve'
                : 'No data available. Run a sweep first.'}
          </p>
        </div>
      );
    }

    switch (chartMode) {
      case 'candlestick':
        return (
          <CandlestickChart
            data={chartData?.candles ?? []}
            showVolume={chartOverlays.volume}
            loading={chartLoading}
            height="100%"
          />
        );

      case 'equity':
        return (
          <EquityChart
            data={chartData?.equity ?? []}
            drawdown={chartData?.drawdown}
            showDrawdown={chartOverlays.drawdown}
            loading={chartLoading}
            height="100%"
            title={getChartTitle()}
          />
        );

      case 'multi_ticker':
        return (
          <MultiSeriesChart
            curves={chartData?.curves ?? []}
            loading={chartLoading}
            height="100%"
            title="Multi-Ticker Comparison"
          />
        );

      case 'portfolio':
        return (
          <EquityChart
            data={chartData?.equity ?? []}
            loading={chartLoading}
            height="100%"
            title="Portfolio Equity"
          />
        );

      case 'strategy_comparison':
        return (
          <MultiSeriesChart
            curves={chartData?.curves ?? []}
            loading={chartLoading}
            height="100%"
            title="Strategy Comparison"
          />
        );

      default:
        return null;
    }
  };

  // Determine which overlay controls to show based on mode
  const showOverlayControls =
    chartMode === 'candlestick' || chartMode === 'equity';

  return (
    <div className={styles.panel}>
      <div className={styles.header}>
        <h1 className={styles.title}>{getChartTitle()}</h1>
        {selectedResultData && chartMode === 'equity' && (
          <div className={styles.metrics}>
            <span className={styles.metric}>
              Sharpe: <strong>{selectedResultData.metrics.sharpe.toFixed(2)}</strong>
            </span>
            <span className={styles.metric}>
              CAGR: <strong>{(selectedResultData.metrics.cagr * 100).toFixed(1)}%</strong>
            </span>
            <span className={styles.metric}>
              Max DD: <strong>{(selectedResultData.metrics.max_drawdown * 100).toFixed(1)}%</strong>
            </span>
          </div>
        )}
      </div>

      <ChartControls
        mode={chartMode}
        overlays={chartOverlays}
        onModeChange={handleModeChange}
        onOverlayToggle={handleOverlayToggle}
        showOverlayControls={showOverlayControls}
      />

      <div className={styles.chartContainer}>{renderChart()}</div>

      {/* Trades table for equity mode */}
      {chartMode === 'equity' &&
        chartOverlays.trades &&
        chartData?.trades &&
        chartData.trades.length > 0 && (
          <div className={styles.tradesPanel}>
            <h3 className={styles.tradesPanelTitle}>Trades</h3>
            <div className={styles.tradesTable}>
              <TradesTable trades={chartData.trades} />
            </div>
          </div>
        )}

      {/* Keyboard shortcuts hint */}
      <div className={styles.shortcuts}>
        <span>
          <kbd>m</kbd> Mode
        </span>
        <span>
          <kbd>d</kbd> Drawdown
        </span>
        <span>
          <kbd>v</kbd> Volume
        </span>
      </div>
    </div>
  );
}
