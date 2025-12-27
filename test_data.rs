use std::path::Path;
use trendlab_core::read_parquet;

fn main() {
    let path = Path::new("data/parquet/1d/symbol=SPY/year=2024/data.parquet");
    match read_parquet(path) {
        Ok(bars) => {
            println!("Loaded {} bars", bars.len());
            if !bars.is_empty() {
                println!("\nFirst 3 bars:");
                for bar in bars.iter().take(3) {
                    println!("  {} O:{:.2} H:{:.2} L:{:.2} C:{:.2}", 
                             bar.ts, bar.open, bar.high, bar.low, bar.close);
                }
                println!("\nLast 3 bars:");
                for bar in bars.iter().rev().take(3).rev() {
                    println!("  {} O:{:.2} H:{:.2} L:{:.2} C:{:.2}", 
                             bar.ts, bar.open, bar.high, bar.low, bar.close);
                }
                
                // Check for any bars with 0 prices
                let zero_bars = bars.iter().filter(|b| b.open == 0.0 || b.close == 0.0).count();
                println!("\nBars with zero prices: {}", zero_bars);
                
                // Price range
                let max_high: f64 = bars.iter().map(|b| b.high).fold(f64::NEG_INFINITY, f64::max);
                let min_low: f64 = bars.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);
                println!("Price range: {:.2} - {:.2}", min_low, max_high);
            }
        }
        Err(e) => println!("Error: {:?}", e),
    }
}
