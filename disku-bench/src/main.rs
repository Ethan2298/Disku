//! Benchmark and stress-test binary for disku's macOS scanner.
//!
//! Usage:
//!   bench_scan [OPTIONS] [PATH]
//!
//! Options:
//!   -n, --iterations N   Number of benchmark runs (default: 5)
//!   --single             Single run mode (for use with `leaks --atExit`)
//!   --compare            Also run jwalk scanner and compare results

#[cfg(target_os = "macos")]
fn main() {
    let args = parse_args();

    println!("=== disku benchmark ===");
    println!("target:     {}", args.path.display());
    println!("iterations: {}", args.iterations);
    println!("compare:    {}", args.compare);
    println!();

    // Collect per-run results
    let mut results: Vec<RunResult> = Vec::new();

    for i in 0..args.iterations {
        if args.iterations > 1 {
            println!("--- run {}/{} ---", i + 1, args.iterations);
        }

        let result = run_mac_scan(&args.path);
        println!(
            "  time: {:.3}s | files: {} | dirs: {} | errors: {} | size: {} | {:.0} files/sec",
            result.wall_secs,
            result.file_count,
            result.dir_count,
            result.errors,
            format_bytes(result.total_size),
            result.files_per_sec,
        );

        results.push(result);
    }

    println!();

    // Aggregate report
    if results.len() > 1 {
        print_aggregate(&results);
        print_consistency(&results);
    }

    // Peak RSS
    if let Some(rss) = get_peak_rss() {
        println!("peak RSS:   {}", format_bytes(rss));
    }

    // Cross-scanner comparison
    if args.compare {
        println!();
        println!("=== jwalk comparison ===");
        let jwalk_result = run_jwalk_scan(&args.path);
        let mac_best = results
            .iter()
            .map(|r| r.wall_secs)
            .fold(f64::INFINITY, f64::min);

        println!(
            "  jwalk:    {:.3}s | files: {} | dirs: {} | size: {}",
            jwalk_result.wall_secs,
            jwalk_result.file_count,
            jwalk_result.dir_count,
            format_bytes(jwalk_result.total_size),
        );
        println!(
            "  mac_scan: {:.3}s (best of {})",
            mac_best, results.len()
        );

        if jwalk_result.wall_secs > 0.0 {
            let speedup = jwalk_result.wall_secs / mac_best;
            println!("  speedup:  {:.2}x", speedup);
        }

        // Tolerance checks
        println!();
        let size_diff = if jwalk_result.total_size > 0 {
            ((results[0].total_size as f64 - jwalk_result.total_size as f64)
                / jwalk_result.total_size as f64)
                .abs()
        } else {
            0.0
        };
        let total_files_mac = results[0].file_count + results[0].dir_count;
        let total_files_jwalk = jwalk_result.file_count + jwalk_result.dir_count;
        let count_diff = if total_files_jwalk > 0 {
            ((total_files_mac as f64 - total_files_jwalk as f64) / total_files_jwalk as f64).abs()
        } else {
            0.0
        };

        if size_diff <= 0.01 {
            println!(
                "  size:  OK (diff {:.2}% <= 1% tolerance)",
                size_diff * 100.0
            );
        } else {
            println!(
                "  size:  WARN (diff {:.2}% > 1% tolerance) mac={} jwalk={}",
                size_diff * 100.0,
                format_bytes(results[0].total_size),
                format_bytes(jwalk_result.total_size),
            );
        }

        if count_diff <= 0.001 {
            println!(
                "  count: OK (diff {:.3}% <= 0.1% tolerance)",
                count_diff * 100.0
            );
        } else {
            println!(
                "  count: WARN (diff {:.3}% > 0.1% tolerance) mac={} jwalk={}",
                count_diff * 100.0,
                total_files_mac,
                total_files_jwalk,
            );
        }
    }

    println!();
    println!("done.");
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("error: bench_scan requires macOS (getattrlistbulk)");
    std::process::exit(1);
}

// -- Argument parsing --

struct Args {
    path: std::path::PathBuf,
    iterations: usize,
    compare: bool,
}

fn parse_args() -> Args {
    let mut args_iter = std::env::args().skip(1);
    let mut path: Option<std::path::PathBuf> = None;
    let mut iterations: usize = 5;
    let mut single = false;
    let mut compare = false;

    while let Some(arg) = args_iter.next() {
        match arg.as_str() {
            "-n" | "--iterations" => {
                if let Some(val) = args_iter.next() {
                    iterations = val.parse().unwrap_or_else(|_| {
                        eprintln!("error: invalid iteration count: {}", val);
                        std::process::exit(1);
                    });
                } else {
                    eprintln!("error: --iterations requires a value");
                    std::process::exit(1);
                }
            }
            "--single" => single = true,
            "--compare" => compare = true,
            other if other.starts_with('-') => {
                eprintln!("error: unknown option: {}", other);
                eprintln!("usage: bench_scan [--iterations N] [--single] [--compare] [PATH]");
                std::process::exit(1);
            }
            _ => {
                path = Some(std::path::PathBuf::from(arg));
            }
        }
    }

    if single {
        iterations = 1;
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
        compare,
    }
}

// -- Run result --

struct RunResult {
    wall_secs: f64,
    file_count: u64,
    dir_count: u64,
    total_size: u64,
    errors: u64,
    files_per_sec: f64,
}

// -- Scanner runners --

#[cfg(target_os = "macos")]
fn run_mac_scan(path: &std::path::Path) -> RunResult {
    use std::sync::atomic::Ordering;

    let progress = disku_core::scanner::ScanProgress::new();
    let start = std::time::Instant::now();
    let tree = disku_core::mac_scanner::scan_bulk(path, &progress);
    let wall_secs = start.elapsed().as_secs_f64();

    let files_scanned = progress.files_scanned.load(Ordering::Relaxed);
    let errors = progress.errors.load(Ordering::Relaxed);

    let (file_count, dir_count, total_size) = tree_stats(&tree);

    RunResult {
        wall_secs,
        file_count,
        dir_count,
        total_size,
        errors,
        files_per_sec: files_scanned as f64 / wall_secs,
    }
}

#[cfg(target_os = "macos")]
fn run_jwalk_scan(path: &std::path::Path) -> RunResult {
    use std::sync::atomic::Ordering;

    let progress = disku_core::scanner::ScanProgress::new();
    let start = std::time::Instant::now();
    let tree = disku_core::scanner::scan(path, &progress);
    let wall_secs = start.elapsed().as_secs_f64();

    let files_scanned = progress.files_scanned.load(Ordering::Relaxed);
    let errors = progress.errors.load(Ordering::Relaxed);

    let (file_count, dir_count, total_size) = tree_stats(&tree);

    RunResult {
        wall_secs,
        file_count,
        dir_count,
        total_size,
        errors,
        files_per_sec: files_scanned as f64 / wall_secs,
    }
}

// -- Tree stats --

fn tree_stats(node: &disku_core::tree::FileNode) -> (u64, u64, u64) {
    let mut files: u64 = 0;
    let mut dirs: u64 = 0;
    let mut size: u64 = 0;
    tree_stats_recursive(node, &mut files, &mut dirs, &mut size);
    (files, dirs, size)
}

fn tree_stats_recursive(
    node: &disku_core::tree::FileNode,
    files: &mut u64,
    dirs: &mut u64,
    size: &mut u64,
) {
    if node.is_dir {
        *dirs += 1;
        for child in &node.children {
            tree_stats_recursive(child, files, dirs, size);
        }
    } else {
        *files += 1;
        *size += node.size;
    }
}

// -- Aggregate report --

fn print_aggregate(results: &[RunResult]) {
    let times: Vec<f64> = results.iter().map(|r| r.wall_secs).collect();
    let min = times.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = times.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let mean = times.iter().sum::<f64>() / times.len() as f64;

    println!("wall time:  min={:.3}s  mean={:.3}s  max={:.3}s", min, mean, max);
}

fn print_consistency(results: &[RunResult]) {
    let first_files = results[0].file_count;
    let first_dirs = results[0].dir_count;
    let first_size = results[0].total_size;

    let all_match = results.iter().all(|r| {
        r.file_count == first_files && r.dir_count == first_dirs && r.total_size == first_size
    });

    if all_match {
        println!(
            "consistency: OK (files={}, dirs={}, size={})",
            first_files,
            first_dirs,
            format_bytes(first_size)
        );
    } else {
        println!("consistency: WARN -- results differ across runs:");
        for (i, r) in results.iter().enumerate() {
            println!(
                "  run {}: files={} dirs={} size={} ({} bytes)",
                i + 1,
                r.file_count,
                r.dir_count,
                format_bytes(r.total_size),
                r.total_size,
            );
        }
    }
}

// -- Peak RSS via getrusage (libc) --

fn get_peak_rss() -> Option<u64> {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    let ret = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    if ret == 0 {
        // ru_maxrss is in bytes on macOS, kilobytes on Linux
        #[cfg(target_os = "macos")]
        { Some(usage.ru_maxrss as u64) }
        #[cfg(not(target_os = "macos"))]
        { Some(usage.ru_maxrss as u64 * 1024) }
    } else {
        None
    }
}

// -- Formatting --

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
