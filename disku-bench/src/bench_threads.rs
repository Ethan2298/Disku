//! Benchmark: how does Rayon thread count affect scan performance?
//!
//! Runs the full mac_scanner::scan_bulk with different thread pool sizes
//! to find the scaling curve and optimal parallelism.
//!
//! Usage:
//!   bench_threads [OPTIONS] [PATH]
//!
//! Options:
//!   -n, --iterations N   Runs per thread count (default: 3)
//!   --warmup             Run one warmup pass before measuring
//!   --max-threads N      Maximum thread count to test (default: 2x CPU cores)

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("error: this benchmark requires macOS (getattrlistbulk)");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
fn main() {
    use std::sync::atomic::Ordering;

    let args = parse_args();
    let num_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let max_threads = args.max_threads.unwrap_or(num_cpus * 2);

    println!("=== thread scaling benchmark ===");
    println!("target:     {}", args.path.display());
    println!("iterations: {}", args.iterations);
    println!("CPU cores:  {}", num_cpus);
    println!("max threads: {}", max_threads);
    println!();

    // Warmup
    if args.warmup {
        print!("warmup... ");
        let progress = disku_core::scanner::ScanProgress::new();
        let _ = disku_core::mac_scanner::scan_bulk(&args.path, &progress);
        println!("done");
        println!();
    }

    // Thread counts to test: 1, 2, 4, ..., up to max, plus num_cpus if not already included
    let mut thread_counts: Vec<usize> = Vec::new();
    let mut t = 1;
    while t <= max_threads {
        thread_counts.push(t);
        t *= 2;
    }
    // Ensure we test the actual CPU count
    if !thread_counts.contains(&num_cpus) {
        thread_counts.push(num_cpus);
        thread_counts.sort();
    }
    // Ensure max is included
    if !thread_counts.contains(&max_threads) && max_threads > *thread_counts.last().unwrap_or(&0) {
        thread_counts.push(max_threads);
    }

    let mut all_results: Vec<(usize, ThreadResult)> = Vec::new();

    for &threads in &thread_counts {
        println!("--- {} thread{} ({} runs) ---", threads, if threads == 1 { "" } else { "s" }, args.iterations);

        let mut times = Vec::new();
        let mut entry_count = 0u64;

        for i in 0..args.iterations {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();

            let progress = disku_core::scanner::ScanProgress::new();
            let start = std::time::Instant::now();

            let tree = pool.install(|| {
                disku_core::mac_scanner::scan_bulk(&args.path, &progress)
            });

            let elapsed = start.elapsed().as_secs_f64();
            times.push(elapsed);

            let files = progress.files_scanned.load(Ordering::Relaxed);
            let dirs = progress.dirs_scanned.load(Ordering::Relaxed);
            entry_count = files + dirs;

            let rate = entry_count as f64 / elapsed;
            println!(
                "  run {}/{}: {:.3}s | {} entries | {:.0} entries/sec",
                i + 1,
                args.iterations,
                elapsed,
                entry_count,
                rate,
            );

            // Prevent the tree from being optimized away
            std::hint::black_box(&tree);
        }

        let min = fmin(&times);
        let mean = times.iter().sum::<f64>() / times.len() as f64;
        let max = fmax(&times);

        all_results.push((threads, ThreadResult {
            min,
            mean,
            max,
            entries: entry_count,
        }));

        println!();
    }

    // Summary table
    println!("=== summary ===");
    println!(
        "{:>8} {:>8} {:>8} {:>8} {:>12} {:>8}",
        "threads", "min", "mean", "max", "entries/sec", "speedup"
    );
    println!("{}", "-".repeat(58));

    let baseline_min = all_results
        .first()
        .map(|(_, r)| r.min)
        .unwrap_or(1.0);
    let best_min = all_results
        .iter()
        .map(|(_, r)| r.min)
        .fold(f64::INFINITY, f64::min);

    for (threads, result) in &all_results {
        let rate = result.entries as f64 / result.min;
        let speedup = baseline_min / result.min;
        let marker = if (result.min - best_min).abs() < 0.001 {
            " <--"
        } else {
            ""
        };
        println!(
            "{:>8} {:>7.3}s {:>7.3}s {:>7.3}s {:>11.0} {:>7.2}x{}",
            threads, result.min, result.mean, result.max, rate, speedup, marker,
        );
    }

    // Scaling analysis
    println!();
    let (best_threads, best_result) = all_results
        .iter()
        .min_by(|(_, a), (_, b)| a.min.partial_cmp(&b.min).unwrap())
        .unwrap();

    let ideal_speedup = *best_threads as f64;
    let actual_speedup = baseline_min / best_result.min;
    let efficiency = actual_speedup / ideal_speedup * 100.0;

    println!(
        "best:       {} thread{} ({:.3}s)",
        best_threads,
        if *best_threads == 1 { "" } else { "s" },
        best_result.min,
    );
    println!(
        "speedup:    {:.2}x over single-threaded ({:.3}s)",
        actual_speedup, baseline_min,
    );
    println!(
        "efficiency: {:.0}% (ideal would be {:.1}x at {} threads)",
        efficiency, ideal_speedup, best_threads,
    );

    // Check for degradation at high thread counts
    let last = all_results.last().unwrap();
    if last.1.min > best_result.min * 1.05 {
        let degradation = (last.1.min - best_result.min) / best_result.min * 100.0;
        println!(
            "warning:    {:.0}% degradation at {} threads vs {} threads",
            degradation, last.0, best_threads,
        );
    }

    // Peak RSS
    if let Some(rss) = get_peak_rss() {
        println!();
        println!("peak RSS: {}", format_bytes(rss));
    }

    println!();
    println!("done.");
}

// ---------------------------------------------------------------------------
// Result type
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
struct ThreadResult {
    min: f64,
    mean: f64,
    max: f64,
    entries: u64,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn fmin(v: &[f64]) -> f64 {
    v.iter().cloned().fold(f64::INFINITY, f64::min)
}

#[cfg(target_os = "macos")]
fn fmax(v: &[f64]) -> f64 {
    v.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
}

#[cfg(target_os = "macos")]
fn get_peak_rss() -> Option<u64> {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    let ret = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    if ret == 0 {
        Some(usage.ru_maxrss as u64)
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

struct Args {
    path: std::path::PathBuf,
    iterations: usize,
    warmup: bool,
    max_threads: Option<usize>,
}

fn parse_args() -> Args {
    let mut args_iter = std::env::args().skip(1);
    let mut path: Option<std::path::PathBuf> = None;
    let mut iterations: usize = 3;
    let mut warmup = false;
    let mut max_threads: Option<usize> = None;

    while let Some(arg) = args_iter.next() {
        match arg.as_str() {
            "-n" | "--iterations" => {
                if let Some(val) = args_iter.next() {
                    iterations = val.parse().unwrap_or_else(|_| {
                        eprintln!("error: invalid iteration count: {}", val);
                        std::process::exit(1);
                    });
                }
            }
            "--warmup" => warmup = true,
            "--max-threads" => {
                if let Some(val) = args_iter.next() {
                    max_threads = Some(val.parse().unwrap_or_else(|_| {
                        eprintln!("error: invalid thread count: {}", val);
                        std::process::exit(1);
                    }));
                }
            }
            other if other.starts_with('-') => {
                eprintln!("error: unknown option: {}", other);
                eprintln!(
                    "usage: bench_threads [-n N] [--warmup] [--max-threads N] [PATH]"
                );
                std::process::exit(1);
            }
            _ => {
                path = Some(std::path::PathBuf::from(arg));
            }
        }
    }

    let path = path.unwrap_or_else(|| {
        std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("/"))
    });

    if !path.is_dir() {
        eprintln!("error: not a directory: {}", path.display());
        std::process::exit(1);
    }

    Args {
        path,
        iterations,
        warmup,
        max_threads,
    }
}
