import { useEffect, useRef, useCallback, useState } from 'react';
import {
  createChart,
  IChartApi,
  ISeriesApi,
  ColorType,
  CrosshairMode,
  LineStyle,
  CandlestickSeries,
  LineSeries,
  AreaSeries,
  HistogramSeries,
  CandlestickData,
  LineData,
  HistogramData,
  Time,
  LineWidth,
} from 'lightweight-charts';
import { CHART_COLORS } from '../../types';

/** Chart configuration options */
export interface ChartOptions {
  /** Show time scale */
  timeScale?: boolean;
  /** Show price scale */
  priceScale?: boolean;
  /** Show crosshair */
  crosshair?: boolean;
  /** Show grid */
  grid?: boolean;
  /** Right price scale width */
  rightPriceScaleWidth?: number;
}

/** Default chart options */
const DEFAULT_OPTIONS: ChartOptions = {
  timeScale: true,
  priceScale: true,
  crosshair: true,
  grid: true,
  rightPriceScaleWidth: 70,
};

/** Hook return type */
export interface UseChartReturn {
  /** Reference to chart container div */
  containerRef: React.RefObject<HTMLDivElement>;
  /** Chart API instance (null until mounted) */
  chart: IChartApi | null;
  /** Fit chart content to visible range */
  fitContent: () => void;
  /** Scroll to latest data */
  scrollToRealTime: () => void;
}

/**
 * Hook for managing a TradingView Lightweight Charts instance.
 *
 * Handles:
 * - Chart creation and cleanup
 * - Container resize observation
 * - Tokyo Night theming
 * - Waiting for valid container dimensions before creation
 */
export function useChart(options: ChartOptions = {}): UseChartReturn {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const [chart, setChart] = useState<IChartApi | null>(null);
  const [dimensionsReady, setDimensionsReady] = useState(false);

  const opts = { ...DEFAULT_OPTIONS, ...options };

  // Wait for container to have valid dimensions
  useEffect(() => {
    if (!containerRef.current) {
      return;
    }

    const container = containerRef.current;
    const { clientWidth, clientHeight } = container;

    // If dimensions are already valid, mark as ready
    if (clientWidth > 0 && clientHeight > 0) {
      setDimensionsReady(true);
      return;
    }

    // Wait for dimensions via ResizeObserver
    const observer = new ResizeObserver((entries) => {
      if (entries.length === 0) return;
      const { width, height } = entries[0].contentRect;
      if (width > 0 && height > 0) {
        setDimensionsReady(true);
        observer.disconnect();
      }
    });

    observer.observe(container);

    return () => observer.disconnect();
  }, []);

  // Create chart once dimensions are ready
  useEffect(() => {
    if (!containerRef.current || !dimensionsReady) {
      return;
    }

    const container = containerRef.current;
    const { clientWidth, clientHeight } = container;

    // Double-check dimensions (defensive)
    if (clientWidth === 0 || clientHeight === 0) {
      return;
    }

    // Create chart with Tokyo Night theme
    const chartInstance = createChart(container, {
      width: clientWidth,
      height: clientHeight,
      layout: {
        background: { type: ColorType.Solid, color: CHART_COLORS.background },
        textColor: CHART_COLORS.textColor,
        fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
        fontSize: 12,
      },
      grid: {
        vertLines: {
          color: opts.grid ? CHART_COLORS.gridLines : 'transparent',
          style: LineStyle.Dotted,
        },
        horzLines: {
          color: opts.grid ? CHART_COLORS.gridLines : 'transparent',
          style: LineStyle.Dotted,
        },
      },
      crosshair: {
        mode: opts.crosshair ? CrosshairMode.Normal : CrosshairMode.Hidden,
        vertLine: {
          color: CHART_COLORS.crosshairLine,
          width: 1,
          style: LineStyle.Dashed,
          labelBackgroundColor: CHART_COLORS.crosshairLabelBg,
        },
        horzLine: {
          color: CHART_COLORS.crosshairLine,
          width: 1,
          style: LineStyle.Dashed,
          labelBackgroundColor: CHART_COLORS.crosshairLabelBg,
        },
      },
      rightPriceScale: {
        visible: opts.priceScale,
        borderColor: CHART_COLORS.borderColor,
        scaleMargins: {
          top: 0.1,
          bottom: 0.2,
        },
      },
      timeScale: {
        visible: opts.timeScale,
        borderColor: CHART_COLORS.borderColor,
        timeVisible: true,
        secondsVisible: false,
      },
      handleScale: {
        axisPressedMouseMove: true,
      },
      handleScroll: {
        mouseWheel: true,
        pressedMouseMove: true,
        horzTouchDrag: true,
        vertTouchDrag: true,
      },
    });

    chartRef.current = chartInstance;
    setChart(chartInstance);

    // Resize observer for responsive charts
    const resizeObserver = new ResizeObserver((entries) => {
      if (entries.length === 0) return;
      const { width, height } = entries[0].contentRect;
      if (width > 0 && height > 0) {
        chartInstance.applyOptions({ width, height });
      }
    });

    resizeObserver.observe(container);

    // Cleanup
    return () => {
      resizeObserver.disconnect();
      chartInstance.remove();
      chartRef.current = null;
      setChart(null);
    };
  }, [dimensionsReady, opts.crosshair, opts.grid, opts.priceScale, opts.timeScale]);

  const fitContent = useCallback(() => {
    chartRef.current?.timeScale().fitContent();
  }, []);

  const scrollToRealTime = useCallback(() => {
    chartRef.current?.timeScale().scrollToRealTime();
  }, []);

  return {
    containerRef: containerRef as React.RefObject<HTMLDivElement>,
    chart,
    fitContent,
    scrollToRealTime,
  };
}

/** Series configuration for candlestick chart */
export interface CandlestickSeriesConfig {
  upColor?: string;
  downColor?: string;
  wickUpColor?: string;
  wickDownColor?: string;
  borderVisible?: boolean;
}

/**
 * Add a candlestick series to a chart (v5 API).
 */
export function addCandlestickSeries(
  chart: IChartApi,
  config: CandlestickSeriesConfig = {}
): ISeriesApi<'Candlestick'> {
  return chart.addSeries(CandlestickSeries, {
    upColor: config.upColor ?? CHART_COLORS.upColor,
    downColor: config.downColor ?? CHART_COLORS.downColor,
    wickUpColor: config.wickUpColor ?? CHART_COLORS.wickUpColor,
    wickDownColor: config.wickDownColor ?? CHART_COLORS.wickDownColor,
    borderVisible: config.borderVisible ?? false,
  });
}

/** Series configuration for line chart */
export interface LineSeriesConfig {
  color?: string;
  lineWidth?: LineWidth;
  lineStyle?: LineStyle;
  priceLineVisible?: boolean;
}

/**
 * Add a line series to a chart (v5 API).
 */
export function addLineSeries(
  chart: IChartApi,
  config: LineSeriesConfig = {}
): ISeriesApi<'Line'> {
  return chart.addSeries(LineSeries, {
    color: config.color ?? CHART_COLORS.blue,
    lineWidth: config.lineWidth ?? (2 as LineWidth),
    lineStyle: config.lineStyle ?? LineStyle.Solid,
    priceLineVisible: config.priceLineVisible ?? false,
    lastValueVisible: true,
  });
}

/** Series configuration for area chart */
export interface AreaSeriesConfig {
  lineColor?: string;
  topColor?: string;
  bottomColor?: string;
  lineWidth?: LineWidth;
}

/**
 * Add an area series to a chart (v5 API).
 */
export function addAreaSeries(
  chart: IChartApi,
  config: AreaSeriesConfig = {}
): ISeriesApi<'Area'> {
  return chart.addSeries(AreaSeries, {
    lineColor: config.lineColor ?? CHART_COLORS.blue,
    topColor: config.topColor ?? 'rgba(122, 162, 247, 0.4)',
    bottomColor: config.bottomColor ?? 'rgba(122, 162, 247, 0.0)',
    lineWidth: config.lineWidth ?? (2 as LineWidth),
  });
}

/** Series configuration for histogram (volume) */
export interface HistogramSeriesConfig {
  color?: string;
  priceFormat?: { type: 'volume' };
  priceScaleId?: string;
}

/**
 * Add a histogram series to a chart (v5 API, typically for volume).
 */
export function addHistogramSeries(
  chart: IChartApi,
  config: HistogramSeriesConfig = {}
): ISeriesApi<'Histogram'> {
  return chart.addSeries(HistogramSeries, {
    color: config.color ?? CHART_COLORS.volumeUp,
    priceFormat: { type: 'volume' },
    priceScaleId: config.priceScaleId ?? 'volume',
  });
}

/**
 * Configure a separate volume pane at the bottom of the chart.
 */
export function configureVolumePriceScale(chart: IChartApi): void {
  chart.priceScale('volume').applyOptions({
    scaleMargins: {
      top: 0.8,
      bottom: 0,
    },
    borderVisible: false,
  });
}

/** Convert Unix timestamp to Lightweight Charts Time format */
export function toChartTime(timestamp: number): Time {
  return timestamp as Time;
}

/** Convert CandleData array to Lightweight Charts format */
export function toCandlestickData(
  data: Array<{ time: number; open: number; high: number; low: number; close: number }>
): CandlestickData[] {
  return data.map((d) => ({
    time: toChartTime(d.time),
    open: d.open,
    high: d.high,
    low: d.low,
    close: d.close,
  }));
}

/** Convert equity point array to Lightweight Charts LineData format */
export function toLineData(
  data: Array<{ time: number; value: number }>
): LineData[] {
  return data.map((d) => ({
    time: toChartTime(d.time),
    value: d.value,
  }));
}

/** Convert volume data to histogram format with up/down colors */
export function toVolumeData(
  candles: Array<{ time: number; open: number; close: number; volume: number }>
): HistogramData[] {
  return candles.map((c) => ({
    time: toChartTime(c.time),
    value: c.volume,
    color: c.close >= c.open ? CHART_COLORS.volumeUp : CHART_COLORS.volumeDown,
  }));
}
