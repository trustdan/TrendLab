import { useEffect, useRef, memo } from 'react';
import { ISeriesApi } from 'lightweight-charts';
import {
  useChart,
  addCandlestickSeries,
  addHistogramSeries,
  configureVolumePriceScale,
  toCandlestickData,
  toVolumeData,
} from './useChart';
import type { ChartCandleData } from '../../types';
import styles from './Chart.module.css';

interface CandlestickChartProps {
  /** Candlestick data */
  data: ChartCandleData[];
  /** Show volume subplot */
  showVolume?: boolean;
  /** Chart height (default: 100%) */
  height?: string | number;
  /** Loading state */
  loading?: boolean;
}

/**
 * Candlestick chart with optional volume subplot.
 */
export const CandlestickChart = memo(function CandlestickChart({
  data,
  showVolume = true,
  height = '100%',
  loading = false,
}: CandlestickChartProps) {
  const { containerRef, chart, fitContent } = useChart();
  const candleSeriesRef = useRef<ISeriesApi<'Candlestick'> | null>(null);
  const volumeSeriesRef = useRef<ISeriesApi<'Histogram'> | null>(null);

  // Create series on chart mount
  useEffect(() => {
    if (!chart) return;

    // Add candlestick series
    candleSeriesRef.current = addCandlestickSeries(chart);

    // Add volume series if enabled
    if (showVolume) {
      volumeSeriesRef.current = addHistogramSeries(chart, {
        priceScaleId: 'volume',
      });
      configureVolumePriceScale(chart);
    }

    return () => {
      if (candleSeriesRef.current) {
        chart.removeSeries(candleSeriesRef.current);
        candleSeriesRef.current = null;
      }
      if (volumeSeriesRef.current) {
        chart.removeSeries(volumeSeriesRef.current);
        volumeSeriesRef.current = null;
      }
    };
  }, [chart, showVolume]);

  // Update data when it changes
  useEffect(() => {
    if (!candleSeriesRef.current || data.length === 0) return;

    const candleData = toCandlestickData(data);
    candleSeriesRef.current.setData(candleData);

    if (volumeSeriesRef.current && showVolume) {
      const volumeData = toVolumeData(data);
      volumeSeriesRef.current.setData(volumeData);
    }

    // Fit content after data update
    fitContent();
  }, [data, showVolume, fitContent]);

  if (loading) {
    return (
      <div className={styles.container} style={{ height }}>
        <div className={styles.loading}>Loading chart data...</div>
      </div>
    );
  }

  if (data.length === 0) {
    return (
      <div className={styles.container} style={{ height }}>
        <div className={styles.empty}>No data available</div>
      </div>
    );
  }

  return (
    <div className={styles.container} style={{ height }}>
      <div ref={containerRef} className={styles.chart} />
    </div>
  );
});
