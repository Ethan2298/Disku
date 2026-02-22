//! Benchmark: cold cache vs warm cache scan performance.
//!
//! Measures the real-world "first scan" experience by using `sudo purge`
//! to flush the macOS disk cache between runs.
//!
//! Usage:
//!   bench_cache [OPTIONS] [PATH]
//!
//! Options:
//!   -n, --iterations N   Runs per mode (default: 3)
//!   --no-purge           Skip cold-cache tests (just run warm-cache)
//!
//! NOTE: Cold-cache tests require sudo access for `purge`. The benchmark
//! will prompt for your password on the first run. Use --no-purge to skip
//! if you don't have sudo access.

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("error: this benchmark requires macOS");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
fn main() {
    use std::sync::atomic::Ordering;

    let args = parse_args();

    println!("=== cold vs warm cache benchmark ===");
    println!("target:     {}", args.path.display());
    println!("iterations: {}", args.iterations);
    println!();

    let can_purge = if args.no_purge {
        println!("note: cold-cache tests disabled (--no-purge)");
        println!();
        false
    } else {
        // Test if we can run purge
        print!("checking sudo access... ");
        if try_purge() {
            println!("ok");
            true
        } else {
            println!("failed (skipping cold-cache tests)");
            println!("hint: run with sudo or use --no-purge");
            false
        }
    };

    // --- Warm cache: run multiple times, cache primed after first ---
    println!();
    println!("=== warm cache (getattrlistbulk) ===");
    {
        // Prime the cache
        print!("priming cache... ");
        let progress = disku_core::scanner::ScanProgress::new();
        let _ = disku_core::mac_scanner::scan_bulk(&args.path, &progress);
        println!("done");
    }

    let mut warm_times = Vec::new();
    let mut warm_entries = 0u64;

    for i in 0..args.iterations {
        let progress = disku_core::scanner::ScanProgress::new();
        let start = std::time::Instant::now();
        let tree = disku_core::mac_scanner::scan_bulk(&args.path, &progress);
        let elapsed = start.elapsed().as_secs_f64();

        let files = progress.files_scanned.load(Ordering::Relaxed);
        let dirs = progress.dirs_scanned.load(Ordering::Relaxed);
        warm_entries = files + dirs;
        let rate = warm_entries as f64 / elapsed;

        println!(
            "  run {}/{}: {:.3}s | {} entries | {:.0} entries/sec",
            i + 1,
            args.iterations,
            elapsed,
            warm_entries,
            rate,
        );

        warm_times.push(elapsed);
        std::hint::black_box(&tree);
    }

    // --- Cold cache: purge between each run ---
    let mut cold_times = Vec::new();

    if can_purge {
        println!();
        println!("=== cold cache (getattrlistbulk) ===");

        for i in 0..args.iterations {
            print!("  purging disk cache... ");
            if !try_purge() {
                println!("failed, aborting cold tests");
                break;
            }
            println!("done");

            let progress = disku_core::scanner::ScanProgress::new();
            let start = std::time::Instant::now();
            let tree = disku_core::mac_scanner::scan_bulk(&args.path, &progress);
            let elapsed = start.elapsed().as_secs_f64();

            let files = progress.files_scanned.load(Ordering::Relaxed);
            let dirs = progress.dirs_scanned.load(Ordering::Relaxed);
            let entries = files + dirs;
            let rate = entries as f64 / elapsed;

            println!(
                "  run {}/{}: {:.3}s | {} entries | {:.0} entries/sec",
                i + 1,
                args.iterations,
                elapsed,
                entries,
                rate,
            );

            cold_times.push(elapsed);
            std::hint::black_box(&tree);
        }
    }

    // --- Warm cache: jwalk for comparison ---
    println!();
    println!("=== warm cache (jwalk) ===");
    {
        // Prime cache again after potential purge
        print!("priming cache... ");
        let progress = disku_core::scanner::ScanProgress::new();
        let _ = disku_core::scanner::scan(&args.path, &progress);
        println!("done");
    }

    let mut jwalk_warm_times = Vec::new();

    for i in 0..args.iterations {
        let progress = disku_core::scanner::ScanProgress::new();
        let start = std::time::Instant::now();
        let tree = disku_core::scanner::scan(&args.path, &progress);
        let elapsed = start.elapsed().as_secs_f64();

        let files = progress.files_scanned.load(Ordering::Relaxed);
        let dirs = progress.dirs_scanned.load(Ordering::Relaxed);
        let entries = files + dirs;
        let rate = entries as f64 / elapsed;

        println!(
            "  run {}/{}: {:.3}s | {} entries | {:.0} entries/sec",
            i + 1,
            args.iterations,
            elapsed,
            entries,
            rate,
        );

        jwalk_warm_times.push(elapsed);
        std::hint::black_box(&tree);
    }

    // --- Cold cache: jwalk ---
    let mut jwalk_cold_times = Vec::new();

    if can_purge {
        println!();
        println!("=== cold cache (jwalk) ===");

        for i in 0..args.iterations {
            print!("  purging disk cache... ");
            if !try_purge() {
                println!("failed, aborting cold tests");
                break;
            }
            println!("done");

            let progress = disku_core::scanner::ScanProgress::new();
            let start = std::time::Instant::now();
            let tree = disku_core::scanner::scan(&args.path, &progress);
            let elapsed = start.elapsed().as_secs_f64();

            let files = progress.files_scanned.load(Ordering::Relaxed);
            let dirs = progress.dirs_scanned.load(Ordering::Relaxed);
            let entries = files + dirs;
            let rate = entries as f64 / elapsed;

            println!(
                "  run {}/{}: {:.3}s | {} entries | {:.0} entries/sec",
                i + 1,
                args.iterations,
                elapsed,
                entries,
                rate,
            );

            jwalk_cold_times.push(elapsed);
            std::hint::black_box(&tree);
        }
    }

    // --- Summary ---
    println!();
    println!("=== summary ===");
    println!(
        "{:<24} {:>8} {:>8} {:>8} {:>12}",
        "mode", "min", "mean", "max", "entries/sec"
    );
    println!("{}", "-".repeat(64));

    print_row("bulk warm", &warm_times, warm_entries);
    if !cold_times.is_empty() {
        print_row("bulk cold", &cold_times, warm_entries);
    }
    print_row("jwalk warm", &jwalk_warm_times, warm_entries);
    if !jwalk_cold_times.is_empty() {
        print_row("jwalk cold", &jwalk_cold_times, warm_entries);
    }

    // Analysis
    println!();
    let bulk_warm_min = fmin(&warm_times);
    let jwalk_warm_min = fmin(&jwalk_warm_times);

    println!(
        "bulk vs jwalk (warm):  {:.2}x speedup",
        jwalk_warm_min / bulk_warm_min,
    );

    if !cold_times.is_empty() && !jwalk_cold_times.is_empty() {
        let bulk_cold_min = fmin(&cold_times);
        let jwalk_cold_min = fmin(&jwalk_cold_times);

        println!(
            "bulk vs jwalk (cold):  {:.2}x speedup",
            jwalk_cold_min / bulk_cold_min,
        );

        let bulk_cold_penalty = bulk_cold_min / bulk_warm_min;
        let jwalk_cold_penalty = jwalk_cold_min / jwalk_warm_min;

        println!();
        println!(
            "cold cache penalty (bulk):  {:.1}x slower ({:.3}s -> {:.3}s)",
            bulk_cold_penalty, bulk_warm_min, bulk_cold_min,
        );
        println!(
            "cold cache penalty (jwalk): {:.1}x slower ({:.3}s -> {:.3}s)",
            jwalk_cold_penalty, jwalk_warm_min, jwalk_cold_min,
        );

        if bulk_cold_penalty < jwalk_cold_penalty {
            println!();
            println!(
                "verdict: bulk scanner handles cold cache better ({:.1}x vs {:.1}x penalty)",
                bulk_cold_penalty, jwalk_cold_penalty,
            );
        } else if jwalk_cold_penalty < bulk_cold_penalty {
            println!();
            println!(
                "verdict: jwalk handles cold cache better ({:.1}x vs {:.1}x penalty)",
                jwalk_cold_penalty, bulk_cold_penalty,
            );
        } else {
            println!();
            println!("verdict: both scanners handle cold cache similarly");
        }
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
// Purge helper
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn try_purge() -> bool {
    match std::process::Command::new("sudo")
        .args(["-n", "purge"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        Ok(status) => {
            if status.success() {
                // Small sleep to let the cache flush settle
                std::thread::sleep(std::time::Duration::from_millis(500));
                return true;
            }
            // -n failed (needs password), try interactive
            match std::process::Command::new("sudo")
                .arg("purge")
                .status()
            {
                Ok(s) => {
                    if s.success() {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                    s.success()
                }
                Err(_) => false,
            }
        }
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn print_row(label: &str, times: &[f64], entries: u64) {
    if times.is_empty() {
        return;
    }
    let min = fmin(times);
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    let max = fmax(times);
    let rate = entries as f64 / min;
    println!(
        "{:<24} {:>7.3}s {:>7.3}s {:>7.3}s {:>11.0}",
        label, min, mean, max, rate,
    );
}

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
    no_purge: bool,
}

fn parse_args() -> Args {
    let mut args_iter = std::env::args().skip(1);
    let mut path: Option<std::path::PathBuf> = None;
    let mut iterations: usize = 3;
    let mut no_purge = false;

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
            "--no-purge" => no_purge = true,
            other if other.starts_with('-') => {
                eprintln!("error: unknown option: {}", other);
                eprintln!("usage: bench_cache [-n N] [--no-purge] [PATH]");
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
        no_purge,
    }
}
