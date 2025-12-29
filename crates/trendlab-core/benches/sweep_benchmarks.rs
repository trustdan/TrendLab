//! Benchmark suite for sweep function variants.
//!
//! Compares performance of:
//! - `run_strategy_sweep_polars` (baseline - each config separate)
//! - `run_strategy_sweep_polars_parallel` (rayon parallel)
//! - `run_strategy_sweep_polars_cached` (indicator caching)
//! - `run_strategy_sweep_polars_lazy` (optimal lazy + batched indicators)

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use polars::prelude::*;
use trendlab_core::{
    run_strategy_sweep_polars, run_strategy_sweep_polars_cached, run_strategy_sweep_polars_lazy,
    run_strategy_sweep_polars_parallel, PolarsBacktestConfig, StrategyGridConfig, StrategyParams,
    StrategyTypeId,
};

/// Generate synthetic OHLCV data for benchmarking.
fn generate_benchmark_data(num_bars: usize) -> DataFrame {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    let mut dates: Vec<i32> = Vec::with_capacity(num_bars);
    let mut opens: Vec<f64> = Vec::with_capacity(num_bars);
    let mut highs: Vec<f64> = Vec::with_capacity(num_bars);
    let mut lows: Vec<f64> = Vec::with_capacity(num_bars);
    let mut closes: Vec<f64> = Vec::with_capacity(num_bars);
    let mut volumes: Vec<f64> = Vec::with_capacity(num_bars);

    // Start from a base date and price
    let base_date = 19000; // ~2022-01-01 in days since epoch
    let mut price: f64 = 100.0;

    for i in 0..num_bars {
        // Random walk with slight upward drift (trend)
        let daily_return: f64 = rng.gen_range(-0.02..0.025);
        price *= 1.0 + daily_return;
        price = price.max(10.0); // Floor at $10

        let open: f64 = price * rng.gen_range(0.995..1.005);
        let close: f64 = price * rng.gen_range(0.995..1.005);
        let high: f64 = open.max(close) * rng.gen_range(1.001..1.015);
        let low: f64 = open.min(close) * rng.gen_range(0.985..0.999);
        let volume: f64 = rng.gen_range(100_000.0..10_000_000.0);

        dates.push(base_date + i as i32);
        opens.push(open);
        highs.push(high);
        lows.push(low);
        closes.push(close);
        volumes.push(volume);
    }

    df!(
        "date" => dates,
        "open" => opens,
        "high" => highs,
        "low" => lows,
        "close" => closes,
        "volume" => volumes
    )
    .expect("Failed to create benchmark DataFrame")
}

/// Create a Donchian grid with specified size.
fn donchian_grid(size: &str) -> StrategyGridConfig {
    let params = match size {
        "small" => StrategyParams::Donchian {
            entry_lookbacks: vec![10, 20, 30],
            exit_lookbacks: vec![5, 10],
        },
        "medium" => StrategyParams::Donchian {
            entry_lookbacks: vec![10, 20, 30, 40, 55],
            exit_lookbacks: vec![5, 10, 15, 20],
        },
        "large" => StrategyParams::Donchian {
            entry_lookbacks: vec![10, 15, 20, 25, 30, 35, 40, 45, 50, 55],
            exit_lookbacks: vec![5, 10, 15, 20, 25, 30, 35, 40],
        },
        _ => panic!("Unknown grid size: {}", size),
    };

    StrategyGridConfig {
        strategy_type: StrategyTypeId::Donchian,
        enabled: true,
        params,
    }
}

/// Create an MA Crossover grid with specified size.
fn ma_crossover_grid(size: &str) -> StrategyGridConfig {
    let params = match size {
        "small" => StrategyParams::MACrossover {
            fast_periods: vec![10, 20],
            slow_periods: vec![50, 100],
            ma_types: vec![trendlab_core::MAType::SMA],
        },
        "medium" => StrategyParams::MACrossover {
            fast_periods: vec![10, 20, 50],
            slow_periods: vec![50, 100, 200],
            ma_types: vec![trendlab_core::MAType::SMA, trendlab_core::MAType::EMA],
        },
        "large" => StrategyParams::MACrossover {
            fast_periods: vec![5, 10, 15, 20, 30, 50],
            slow_periods: vec![50, 75, 100, 150, 200],
            ma_types: vec![trendlab_core::MAType::SMA, trendlab_core::MAType::EMA],
        },
        _ => panic!("Unknown grid size: {}", size),
    };

    StrategyGridConfig {
        strategy_type: StrategyTypeId::MACrossover,
        enabled: true,
        params,
    }
}

/// Benchmark sweep variants on Donchian strategy with different data sizes.
fn bench_donchian_data_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("donchian_data_sizes");
    let config = PolarsBacktestConfig::default();
    let grid = donchian_grid("medium");

    // Test with different data sizes
    for num_bars in [500, 1000, 2500] {
        let df = generate_benchmark_data(num_bars);
        let num_configs = grid.params.config_count();

        group.throughput(Throughput::Elements(num_configs as u64));

        // Baseline: takes &DataFrame
        group.bench_with_input(BenchmarkId::new("baseline", num_bars), &df, |b, df| {
            b.iter(|| {
                run_strategy_sweep_polars(black_box(df), black_box(&grid), black_box(&config))
            })
        });

        // Parallel: takes &DataFrame
        group.bench_with_input(BenchmarkId::new("parallel", num_bars), &df, |b, df| {
            b.iter(|| {
                run_strategy_sweep_polars_parallel(
                    black_box(df),
                    black_box(&grid),
                    black_box(&config),
                )
            })
        });

        // Cached: takes &DataFrame
        group.bench_with_input(BenchmarkId::new("cached", num_bars), &df, |b, df| {
            b.iter(|| {
                run_strategy_sweep_polars_cached(
                    black_box(df),
                    black_box(&grid),
                    black_box(&config),
                )
            })
        });

        // Lazy: takes LazyFrame
        group.bench_with_input(BenchmarkId::new("lazy", num_bars), &df, |b, df| {
            b.iter(|| {
                let lf = df.clone().lazy();
                run_strategy_sweep_polars_lazy(black_box(lf), black_box(&grid), black_box(&config))
            })
        });
    }

    group.finish();
}

/// Benchmark sweep variants on Donchian strategy with different grid sizes.
fn bench_donchian_grid_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("donchian_grid_sizes");
    let config = PolarsBacktestConfig::default();
    let df = generate_benchmark_data(1000);

    for grid_size in ["small", "medium", "large"] {
        let grid = donchian_grid(grid_size);
        let num_configs = grid.params.config_count();
        let config_label = format!("{} ({})", grid_size, num_configs);

        group.throughput(Throughput::Elements(num_configs as u64));

        // Baseline: takes &DataFrame
        group.bench_with_input(BenchmarkId::new("baseline", &config_label), &df, |b, df| {
            b.iter(|| {
                run_strategy_sweep_polars(black_box(df), black_box(&grid), black_box(&config))
            })
        });

        // Parallel: takes &DataFrame
        group.bench_with_input(BenchmarkId::new("parallel", &config_label), &df, |b, df| {
            b.iter(|| {
                run_strategy_sweep_polars_parallel(
                    black_box(df),
                    black_box(&grid),
                    black_box(&config),
                )
            })
        });

        // Cached: takes &DataFrame
        group.bench_with_input(BenchmarkId::new("cached", &config_label), &df, |b, df| {
            b.iter(|| {
                run_strategy_sweep_polars_cached(
                    black_box(df),
                    black_box(&grid),
                    black_box(&config),
                )
            })
        });

        group.bench_with_input(BenchmarkId::new("lazy", &config_label), &df, |b, df| {
            b.iter(|| {
                let lf = df.clone().lazy();
                run_strategy_sweep_polars_lazy(black_box(lf), black_box(&grid), black_box(&config))
            })
        });
    }

    group.finish();
}

/// Benchmark sweep variants on MA Crossover (dual indicator, more complex).
fn bench_ma_crossover(c: &mut Criterion) {
    let mut group = c.benchmark_group("ma_crossover");
    let config = PolarsBacktestConfig::default();
    let df = generate_benchmark_data(1000);
    let grid = ma_crossover_grid("medium");
    let num_configs = grid.params.config_count();

    group.throughput(Throughput::Elements(num_configs as u64));

    // Baseline: takes &DataFrame
    group.bench_function("baseline", |b| {
        b.iter(|| run_strategy_sweep_polars(black_box(&df), black_box(&grid), black_box(&config)))
    });

    // Parallel: takes &DataFrame
    group.bench_function("parallel", |b| {
        b.iter(|| {
            run_strategy_sweep_polars_parallel(black_box(&df), black_box(&grid), black_box(&config))
        })
    });

    // Cached: takes &DataFrame
    group.bench_function("cached", |b| {
        b.iter(|| {
            run_strategy_sweep_polars_cached(black_box(&df), black_box(&grid), black_box(&config))
        })
    });

    group.bench_function("lazy", |b| {
        b.iter(|| {
            let lf = df.clone().lazy();
            run_strategy_sweep_polars_lazy(black_box(lf), black_box(&grid), black_box(&config))
        })
    });

    group.finish();
}

/// Quick sanity check benchmark (small, fast).
fn bench_sanity(c: &mut Criterion) {
    let df = generate_benchmark_data(100);
    let config = PolarsBacktestConfig::default();
    let grid = donchian_grid("small");

    c.bench_function("sanity_check", |b| {
        b.iter(|| {
            let lf = df.clone().lazy();
            run_strategy_sweep_polars_lazy(black_box(lf), black_box(&grid), black_box(&config))
        })
    });
}

criterion_group!(
    benches,
    bench_sanity,
    bench_donchian_data_sizes,
    bench_donchian_grid_sizes,
    bench_ma_crossover,
);
criterion_main!(benches);
