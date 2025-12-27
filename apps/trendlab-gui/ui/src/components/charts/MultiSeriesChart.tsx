import { useEffect, useRef, memo, useState } from 'react';
import { ISeriesApi } from 'lightweight-charts';
import { useChart, addLineSeries, toLineData } from './useChart';
import type { NamedEquityCurve } from '../../types';
import styles from './Chart.module.css';

interface MultiSeriesChartProps {
  /** Named equity curves to display */
  curves: NamedEquityCurve[];
  /** Chart height (default: 100%) */
  height?: string | number;
  /** Loading state */
  loading?: boolean;
  /** Chart title */
  title?: string;
}

/**
 * Multi-series line chart for comparing multiple tickers or strategies.
 */
export const MultiSeriesChart = memo(function MultiSeriesChart({
  curves,
  height = '100%',
  loading = false,
  title,
}: MultiSeriesChartProps) {
  const { containerRef, chart, fitContent } = useChart();
  const seriesRefs = useRef<Map<string, ISeriesApi<'Line'>>>(new Map());
  const [hoveredSeries, setHoveredSeries] = useState<string | null>(null);

  // Manage series lifecycle
  useEffect(() => {
    if (!chart) return;

    const currentNames = new Set(curves.map((c) => c.name));
    const existingNames = new Set(seriesRefs.current.keys());

    // Remove series that are no longer in curves
    for (const name of existingNames) {
      if (!currentNames.has(name)) {
        const series = seriesRefs.current.get(name);
        if (series) {
          chart.removeSeries(series);
          seriesRefs.current.delete(name);
        }
      }
    }

    // Add or update series
    for (const curve of curves) {
      let series = seriesRefs.current.get(curve.name);

      if (!series) {
        // Create new series
        series = addLineSeries(chart, {
          color: curve.color,
          lineWidth: 2,
          priceLineVisible: false,
        });
        seriesRefs.current.set(curve.name, series);
      }

      // Update data
      if (curve.data.length > 0) {
        const lineData = toLineData(curve.data);
        series.setData(lineData);
      }
    }

    fitContent();

    return () => {
      // Cleanup all series on unmount
      for (const series of seriesRefs.current.values()) {
        try {
          chart.removeSeries(series);
        } catch {
          // Series may already be removed
        }
      }
      seriesRefs.current.clear();
    };
  }, [chart, curves, fitContent]);

  // Handle series highlighting on hover
  useEffect(() => {
    if (!chart) return;

    for (const [name, series] of seriesRefs.current.entries()) {
      const curve = curves.find((c) => c.name === name);
      if (!curve) continue;

      const isHighlighted = hoveredSeries === null || hoveredSeries === name;
      series.applyOptions({
        lineWidth: isHighlighted ? 2 : 1,
        color: isHighlighted ? curve.color : `${curve.color}66`, // Add opacity if not highlighted
      });
    }
  }, [chart, hoveredSeries, curves]);

  if (loading) {
    return (
      <div className={styles.container} style={{ height }}>
        <div className={styles.loading}>Loading chart data...</div>
      </div>
    );
  }

  if (curves.length === 0) {
    return (
      <div className={styles.container} style={{ height }}>
        <div className={styles.empty}>No data available</div>
      </div>
    );
  }

  return (
    <div className={styles.multiSeriesContainer} style={{ height }}>
      {title && (
        <div className={styles.legend}>
          <div className={styles.legendItem}>
            <span>{title}</span>
          </div>
        </div>
      )}
      <div className={styles.chartArea}>
        <div ref={containerRef} className={styles.chart} />
      </div>
      <div className={styles.legendBar}>
        {curves.map((curve) => {
          // Calculate return for this curve
          const firstValue = curve.data[0]?.value ?? 0;
          const lastValue = curve.data[curve.data.length - 1]?.value ?? 0;
          const returnPct =
            firstValue > 0 ? ((lastValue - firstValue) / firstValue) * 100 : 0;

          return (
            <div
              key={curve.name}
              className={styles.legendChip}
              onMouseEnter={() => setHoveredSeries(curve.name)}
              onMouseLeave={() => setHoveredSeries(null)}
              style={{
                opacity: hoveredSeries === null || hoveredSeries === curve.name ? 1 : 0.5,
                cursor: 'pointer',
              }}
            >
              <div
                className={styles.legendDot}
                style={{ backgroundColor: curve.color }}
              />
              <span>{curve.name}</span>
              <span
                style={{
                  color: returnPct >= 0 ? '#9ece6a' : '#f7768e',
                  marginLeft: 4,
                }}
              >
                {returnPct >= 0 ? '+' : ''}
                {returnPct.toFixed(1)}%
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
});
