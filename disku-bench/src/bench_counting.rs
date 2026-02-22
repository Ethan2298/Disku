//! Benchmark: does tracking file/folder counts slow down scanning?
//!
//! Compares three modes of the macOS getattrlistbulk scanner:
//!   1. Raw walk    — syscall + parse only, no counting, no tree building
//!   2. Count only  — syscall + parse + atomic counter increments (no tree)
//!   3. Full scan   — the real scan_bulk() with counting + tree building
//!
//! Usage:
//!   bench_counting [OPTIONS] [PATH]
//!
//! Options:
//!   -n, --iterations N   Runs per mode (default: 5)
//!   --path PATH          Directory to scan (default: $HOME)
//!   --warmup             Run one warmup pass before measuring

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("error: this benchmark requires macOS (getattrlistbulk)");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
fn main() {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    let args = parse_args();

    println!("=== counting overhead benchmark ===");
    println!("target:     {}", args.path.display());
    println!("iterations: {}", args.iterations);
    println!();

    // Optional warmup to prime the filesystem cache
    if args.warmup {
        print!("warmup... ");
        let progress = disku_core::scanner::ScanProgress::new();
        let _ = disku_core::mac_scanner::scan_bulk(&args.path, &progress);
        println!("done");
        println!();
    }

    // --- Mode 1: Raw walk (syscall only, no counting, no tree) ---
    let raw_times = bench_n(args.iterations, "raw walk", || {
        let (files, dirs) = raw_walk(&args.path);
        (files, dirs)
    });

    // --- Mode 2: Count only (syscall + atomic increments, no tree) ---
    let count_times = bench_n(args.iterations, "count only", || {
        let file_count = Arc::new(AtomicU64::new(0));
        let dir_count = Arc::new(AtomicU64::new(0));
        let current_path = Arc::new(Mutex::new(String::new()));

        raw_walk_with_counting(
            &args.path,
            &file_count,
            &dir_count,
            &current_path,
        );

        let f = file_count.load(Ordering::Relaxed);
        let d = dir_count.load(Ordering::Relaxed);
        (f, d)
    });

    // --- Mode 3: Full scan (counting + tree building) ---
    let full_times = bench_n(args.iterations, "full scan", || {
        let progress = disku_core::scanner::ScanProgress::new();
        let tree = disku_core::mac_scanner::scan_bulk(&args.path, &progress);
        let f = progress.files_scanned.load(Ordering::Relaxed);
        let d = progress.dirs_scanned.load(Ordering::Relaxed);
        std::hint::black_box(&tree);
        (f, d)
    });

    // --- Summary ---
    println!();
    println!("=== summary ===");
    println!(
        "{:<14} {:>8} {:>8} {:>8}",
        "mode", "min", "mean", "max"
    );
    println!("{}", "-".repeat(44));
    print_summary("raw walk", &raw_times);
    print_summary("count only", &count_times);
    print_summary("full scan", &full_times);

    println!();
    let raw_min = fmin(&raw_times);
    let count_min = fmin(&count_times);
    let full_min = fmin(&full_times);

    let counting_overhead_ms = (count_min - raw_min) * 1000.0;
    let counting_overhead_pct = if raw_min > 0.0 {
        (count_min - raw_min) / raw_min * 100.0
    } else {
        0.0
    };
    let tree_overhead_ms = (full_min - count_min) * 1000.0;
    let tree_overhead_pct = if count_min > 0.0 {
        (full_min - count_min) / count_min * 100.0
    } else {
        0.0
    };

    println!("counting overhead:    {:>+7.1}ms ({:>+.2}%)", counting_overhead_ms, counting_overhead_pct);
    println!("tree build overhead:  {:>+7.1}ms ({:>+.2}%)", tree_overhead_ms, tree_overhead_pct);
    println!();

    if counting_overhead_pct.abs() < 2.0 {
        println!("verdict: counting adds negligible overhead (within noise)");
    } else if counting_overhead_pct < 5.0 {
        println!("verdict: counting adds minor overhead (<5%)");
    } else {
        println!("verdict: counting adds measurable overhead ({:.1}%)", counting_overhead_pct);
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
// Raw walk — pure syscall traversal, no counting, no tree
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn raw_walk(root: &std::path::Path) -> (u64, u64) {
    let root_dev = get_dev(root);
    let mut files: u64 = 0;
    let mut dirs: u64 = 0;
    raw_walk_recursive(root, root_dev, &mut files, &mut dirs);
    (files, dirs)
}

#[cfg(target_os = "macos")]
fn raw_walk_recursive(
    dir_path: &std::path::Path,
    root_dev: Option<u64>,
    files: &mut u64,
    dirs: &mut u64,
) {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = match CString::new(dir_path.as_os_str().as_bytes()) {
        Ok(p) => p,
        Err(_) => return,
    };
    let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY) };
    if fd < 0 {
        return;
    }

    let alist = bulk_attrlist();
    let mut buf = vec![0u8; 256 * 1024];
    let mut subdirs: Vec<std::path::PathBuf> = Vec::new();

    loop {
        let count = unsafe {
            getattrlistbulk(
                fd,
                &alist as *const BulkAttrList,
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len(),
                0,
            )
        };
        if count <= 0 {
            break;
        }

        let mut offset = 0usize;
        for _ in 0..count {
            if offset + 4 > buf.len() {
                break;
            }
            let entry_len =
                u32::from_ne_bytes(buf[offset..offset + 4].try_into().unwrap()) as usize;
            if entry_len == 0 || offset + entry_len > buf.len() {
                break;
            }

            if let Some((name, is_dir, _size)) = parse_entry_minimal(&buf[offset..offset + entry_len]) {
                if is_dir {
                    *dirs += 1;
                    let child_path = dir_path.join(&name);
                    if let Some(rd) = root_dev {
                        if get_dev(&child_path) != Some(rd) {
                            offset += entry_len;
                            continue;
                        }
                    }
                    subdirs.push(child_path);
                } else {
                    *files += 1;
                }
            }
            offset += entry_len;
        }
    }

    unsafe { libc::close(fd) };

    // Recurse into subdirectories (single-threaded to isolate counting overhead)
    for subdir in subdirs {
        raw_walk_recursive(&subdir, root_dev, files, dirs);
    }
}

// ---------------------------------------------------------------------------
// Walk with counting — same as raw_walk but with atomic increments + mutex
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn raw_walk_with_counting(
    root: &std::path::Path,
    file_count: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    dir_count: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    current_path: &std::sync::Arc<std::sync::Mutex<String>>,
) {
    let root_dev = get_dev(root);
    walk_counting_recursive(root, root_dev, file_count, dir_count, current_path);
}

#[cfg(target_os = "macos")]
fn walk_counting_recursive(
    dir_path: &std::path::Path,
    root_dev: Option<u64>,
    file_count: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    dir_count: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    current_path: &std::sync::Arc<std::sync::Mutex<String>>,
) {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::sync::atomic::Ordering;

    // Update current path (same as the real scanner)
    if let Ok(mut cp) = current_path.try_lock() {
        *cp = dir_path.to_string_lossy().to_string();
    }

    let c_path = match CString::new(dir_path.as_os_str().as_bytes()) {
        Ok(p) => p,
        Err(_) => return,
    };
    let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY) };
    if fd < 0 {
        return;
    }

    let alist = bulk_attrlist();
    let mut buf = vec![0u8; 256 * 1024];
    let mut subdirs: Vec<std::path::PathBuf> = Vec::new();

    loop {
        let count = unsafe {
            getattrlistbulk(
                fd,
                &alist as *const BulkAttrList,
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len(),
                0,
            )
        };
        if count <= 0 {
            break;
        }

        let mut offset = 0usize;
        for _ in 0..count {
            if offset + 4 > buf.len() {
                break;
            }
            let entry_len =
                u32::from_ne_bytes(buf[offset..offset + 4].try_into().unwrap()) as usize;
            if entry_len == 0 || offset + entry_len > buf.len() {
                break;
            }

            if let Some((name, is_dir, _size)) = parse_entry_minimal(&buf[offset..offset + entry_len]) {
                if is_dir {
                    dir_count.fetch_add(1, Ordering::Relaxed);
                    let child_path = dir_path.join(&name);
                    if let Some(rd) = root_dev {
                        if get_dev(&child_path) != Some(rd) {
                            offset += entry_len;
                            continue;
                        }
                    }
                    subdirs.push(child_path);
                } else {
                    file_count.fetch_add(1, Ordering::Relaxed);
                }
            }
            offset += entry_len;
        }
    }

    unsafe { libc::close(fd) };

    for subdir in subdirs {
        walk_counting_recursive(&subdir, root_dev, file_count, dir_count, current_path);
    }
}

// ---------------------------------------------------------------------------
// Shared helpers — minimal getattrlistbulk bindings
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
const ATTR_BIT_MAP_COUNT: u16 = 5;
#[cfg(target_os = "macos")]
const ATTR_CMN_RETURNED_ATTRS: u32 = 0x80000000;
#[cfg(target_os = "macos")]
const ATTR_CMN_NAME: u32 = 0x00000001;
#[cfg(target_os = "macos")]
const ATTR_CMN_OBJTYPE: u32 = 0x00000008;
#[cfg(target_os = "macos")]
const ATTR_CMN_ERROR: u32 = 0x20000000;
#[cfg(target_os = "macos")]
const ATTR_FILE_DATALENGTH: u32 = 0x00000200;
#[cfg(target_os = "macos")]
#[allow(dead_code)]
const VREG: u32 = 1;
#[cfg(target_os = "macos")]
const VDIR: u32 = 2;

#[cfg(target_os = "macos")]
#[repr(C, packed(4))]
struct BulkAttrList {
    bitmapcount: u16,
    reserved: u16,
    commonattr: u32,
    volattr: u32,
    dirattr: u32,
    fileattr: u32,
    forkattr: u32,
}

#[cfg(target_os = "macos")]
extern "C" {
    fn getattrlistbulk(
        dirfd: libc::c_int,
        alist: *const BulkAttrList,
        attribute_buffer: *mut libc::c_void,
        buffer_size: libc::size_t,
        options: u64,
    ) -> libc::c_int;
}

#[cfg(target_os = "macos")]
fn bulk_attrlist() -> BulkAttrList {
    BulkAttrList {
        bitmapcount: ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: ATTR_CMN_RETURNED_ATTRS | ATTR_CMN_NAME | ATTR_CMN_OBJTYPE | ATTR_CMN_ERROR,
        volattr: 0,
        dirattr: 0,
        fileattr: ATTR_FILE_DATALENGTH,
        forkattr: 0,
    }
}

#[cfg(target_os = "macos")]
fn parse_entry_minimal(data: &[u8]) -> Option<(String, bool, u64)> {
    use std::ffi::CStr;

    const ATTR_SET_SIZE: usize = 20;
    if data.len() < 4 + ATTR_SET_SIZE {
        return None;
    }

    let mut pos = 4;
    let ret_commonattr = u32::from_ne_bytes(data[pos..pos + 4].try_into().ok()?);
    let ret_fileattr = u32::from_ne_bytes(data[pos + 12..pos + 16].try_into().ok()?);
    pos += ATTR_SET_SIZE;

    if ret_commonattr & ATTR_CMN_ERROR != 0 {
        let err = u32::from_ne_bytes(data[pos..pos + 4].try_into().ok()?);
        pos += 4;
        if err != 0 {
            return None;
        }
    }

    if ret_commonattr & ATTR_CMN_NAME == 0 {
        return None;
    }
    let name_ref_offset = i32::from_ne_bytes(data[pos..pos + 4].try_into().ok()?);
    let _name_ref_length = u32::from_ne_bytes(data[pos + 4..pos + 8].try_into().ok()?);
    let name_data_start = (pos as i32 + name_ref_offset) as usize;
    pos += 8;

    let name = if name_data_start < data.len() {
        let name_slice = &data[name_data_start..];
        match CStr::from_bytes_until_nul(name_slice) {
            Ok(cs) => cs.to_string_lossy().to_string(),
            Err(_) => {
                let end = name_slice.iter().position(|&b| b == 0).unwrap_or(name_slice.len());
                String::from_utf8_lossy(&name_slice[..end]).to_string()
            }
        }
    } else {
        return None;
    };

    if name == "." || name == ".." {
        return None;
    }

    let obj_type = if ret_commonattr & ATTR_CMN_OBJTYPE != 0 {
        let t = u32::from_ne_bytes(data[pos..pos + 4].try_into().ok()?);
        pos += 4;
        t
    } else {
        return None;
    };

    let is_dir = obj_type == VDIR;
    let size = if !is_dir && (ret_fileattr & ATTR_FILE_DATALENGTH != 0) {
        u64::from_ne_bytes(data[pos..pos + 8].try_into().ok()?)
    } else {
        0
    };

    Some((name, is_dir, size))
}

#[cfg(target_os = "macos")]
fn get_dev(path: &std::path::Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(path).map(|m| m.dev()).ok()
}

// ---------------------------------------------------------------------------
// Bench harness
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn bench_n<F>(n: usize, label: &str, f: F) -> Vec<f64>
where
    F: Fn() -> (u64, u64),
{
    use std::time::Instant;

    println!("--- {} ({} runs) ---", label, n);
    let mut times = Vec::with_capacity(n);

    for i in 0..n {
        let start = Instant::now();
        let (files, dirs) = f();
        let elapsed = start.elapsed().as_secs_f64();
        times.push(elapsed);

        let total = files + dirs;
        let rate = total as f64 / elapsed;
        println!(
            "  run {}/{}: {:.3}s | files: {} | dirs: {} | {:.0} entries/sec",
            i + 1,
            n,
            elapsed,
            files,
            dirs,
            rate,
        );
    }

    times
}

#[cfg(target_os = "macos")]
fn print_summary(label: &str, times: &[f64]) {
    let min = fmin(times);
    let max = fmax(times);
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    println!(
        "{:<14} {:>7.3}s {:>7.3}s {:>7.3}s",
        label, min, mean, max
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
    warmup: bool,
}

fn parse_args() -> Args {
    let mut args_iter = std::env::args().skip(1);
    let mut path: Option<std::path::PathBuf> = None;
    let mut iterations: usize = 5;
    let mut warmup = false;

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
            other if other.starts_with('-') => {
                eprintln!("error: unknown option: {}", other);
                eprintln!("usage: bench_counting [-n N] [--warmup] [PATH]");
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
    }
}
