import { useEffect, useRef, memo } from 'react';
import { ISeriesApi } from 'lightweight-charts';
import {
  useChart,
  addAreaSeries,
  addLineSeries,
  toLineData,
} from './useChart';
import type { ChartEquityPoint, DrawdownPoint } from '../../types';
import { CHART_COLORS } from '../../types';
import styles from './Chart.module.css';

interface EquityChartProps {
  /** Equity curve data */
  data: ChartEquityPoint[];
  /** Optional drawdown overlay data */
  drawdown?: DrawdownPoint[];
  /** Show drawdown overlay */
  showDrawdown?: boolean;
  /** Chart height (default: 100%) */
  height?: string | number;
  /** Loading state */
  loading?: boolean;
  /** Chart title */
  title?: string;
}

/**
 * Equity curve chart with optional drawdown overlay.
 */
export const EquityChart = memo(function EquityChart({
  data,
  drawdown,
  showDrawdown = false,
  height = '100%',
  loading = false,
  title,
}: EquityChartProps) {
  const { containerRef, chart, fitContent } = useChart();
  const equitySeriesRef = useRef<ISeriesApi<'Area'> | null>(null);
  const drawdownSeriesRef = useRef<ISeriesApi<'Line'> | null>(null);

  // Create series on chart mount
  useEffect(() => {
    if (!chart) return;

    // Add equity area series
    equitySeriesRef.current = addAreaSeries(chart, {
      lineColor: CHART_COLORS.blue,
      topColor: 'rgba(122, 162, 247, 0.4)',
      bottomColor: 'rgba(122, 162, 247, 0.0)',
      lineWidth: 2,
    });

    return () => {
      if (equitySeriesRef.current) {
        chart.removeSeries(equitySeriesRef.current);
        equitySeriesRef.current = null;
      }
    };
  }, [chart]);

  // Handle drawdown series separately
  useEffect(() => {
    if (!chart) return;

    if (showDrawdown && drawdown && drawdown.length > 0) {
      if (!drawdownSeriesRef.current) {
        drawdownSeriesRef.current = addLineSeries(chart, {
          color: CHART_COLORS.drawdownLine,
          lineWidth: 1,
          priceLineVisible: false,
        });

        // Configure drawdown on separate price scale
        drawdownSeriesRef.current.priceScale().applyOptions({
          scaleMargins: {
            top: 0.8,
            bottom: 0,
          },
        });
      }

      const ddData = drawdown.map((d) => ({
        time: d.time,
        value: d.drawdown * 100, // Convert to percentage
      }));
      drawdownSeriesRef.current.setData(toLineData(ddData));
    } else if (drawdownSeriesRef.current) {
      chart.removeSeries(drawdownSeriesRef.current);
      drawdownSeriesRef.current = null;
    }

    return () => {
      if (drawdownSeriesRef.current && chart) {
        try {
          chart.removeSeries(drawdownSeriesRef.current);
        } catch {
          // Series may already be removed
        }
        drawdownSeriesRef.current = null;
      }
    };
  }, [chart, showDrawdown, drawdown]);

  // Update equity data when it changes
  useEffect(() => {
    if (!equitySeriesRef.current || data.length === 0) return;

    const lineData = toLineData(data);
    equitySeriesRef.current.setData(lineData);

    // Fit content after data update
    fitContent();
  }, [data, fitContent]);

  if (loading) {
    return (
      <div className={styles.container} style={{ height }}>
        <div className={styles.loading}>Loading equity curve...</div>
      </div>
    );
  }

  if (data.length === 0) {
    return (
      <div className={styles.container} style={{ height }}>
        <div className={styles.empty}>No equity data available</div>
      </div>
    );
  }

  // Calculate stats for legend
  const firstValue = data[0]?.value ?? 0;
  const lastValue = data[data.length - 1]?.value ?? 0;
  const totalReturn = firstValue > 0 ? ((lastValue - firstValue) / firstValue) * 100 : 0;
  const maxValue = Math.max(...data.map((d) => d.value));
  const minValue = Math.min(...data.map((d) => d.value));
  const maxDrawdown = drawdown ? Math.min(...drawdown.map((d) => d.drawdown)) * 100 : 0;

  return (
    <div className={styles.container} style={{ height }}>
      <div className={styles.legend}>
        {title && (
          <div className={styles.legendItem}>
            <span>{title}</span>
          </div>
        )}
        <div className={styles.legendItem}>
          <span>Return:</span>
          <span
            className={styles.legendValue}
            style={{ color: totalReturn >= 0 ? CHART_COLORS.green : CHART_COLORS.red }}
          >
            {totalReturn >= 0 ? '+' : ''}
            {totalReturn.toFixed(2)}%
          </span>
        </div>
        <div className={styles.legendItem}>
          <span>High:</span>
          <span className={styles.legendValue}>${maxValue.toLocaleString()}</span>
        </div>
        <div className={styles.legendItem}>
          <span>Low:</span>
          <span className={styles.legendValue}>${minValue.toLocaleString()}</span>
        </div>
        {showDrawdown && drawdown && drawdown.length > 0 && (
          <div className={styles.legendItem}>
            <span>Max DD:</span>
            <span className={styles.legendValue} style={{ color: CHART_COLORS.red }}>
              {maxDrawdown.toFixed(2)}%
            </span>
          </div>
        )}
      </div>
      <div ref={containerRef} className={styles.chart} />
    </div>
  );
});
