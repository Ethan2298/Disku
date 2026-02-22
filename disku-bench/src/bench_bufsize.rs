//! Benchmark: what's the optimal getattrlistbulk buffer size?
//!
//! Tests buffer sizes from 16 KB to 2 MB to find the sweet spot between
//! fewer syscalls (large buffer) and cache/TLB pressure (too large).
//!
//! Usage:
//!   bench_bufsize [OPTIONS] [PATH]
//!
//! Options:
//!   -n, --iterations N   Runs per buffer size (default: 3)
//!   --warmup             Run one warmup pass before measuring

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("error: this benchmark requires macOS (getattrlistbulk)");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
fn main() {
    let args = parse_args();

    println!("=== buffer size benchmark ===");
    println!("target:     {}", args.path.display());
    println!("iterations: {}", args.iterations);
    println!();

    // Warmup to prime filesystem cache
    if args.warmup {
        print!("warmup... ");
        let progress = disku_core::scanner::ScanProgress::new();
        let _ = disku_core::mac_scanner::scan_bulk(&args.path, &progress);
        println!("done");
        println!();
    }

    let buf_sizes: Vec<usize> = vec![
        16 * 1024,
        32 * 1024,
        64 * 1024,
        128 * 1024,
        256 * 1024,  // current default
        512 * 1024,
        1024 * 1024,
        2048 * 1024,
    ];

    let mut all_results: Vec<(usize, BufSizeResult)> = Vec::new();

    for &buf_size in &buf_sizes {
        let label = format_buf_size(buf_size);
        println!("--- {} ({} runs) ---", label, args.iterations);

        let mut times = Vec::new();
        let mut syscalls_total = 0u64;
        let mut entries_total = 0u64;

        for i in 0..args.iterations {
            let start = std::time::Instant::now();
            let stats = walk_with_bufsize(&args.path, buf_size);
            let elapsed = start.elapsed().as_secs_f64();
            times.push(elapsed);

            let rate = stats.entries as f64 / elapsed;
            println!(
                "  run {}/{}: {:.3}s | {} entries | {} syscalls | {:.0} entries/sec",
                i + 1,
                args.iterations,
                elapsed,
                stats.entries,
                stats.syscalls,
                rate,
            );

            syscalls_total = stats.syscalls;
            entries_total = stats.entries;
        }

        let min = fmin(&times);
        let mean = times.iter().sum::<f64>() / times.len() as f64;
        let max = fmax(&times);

        all_results.push((buf_size, BufSizeResult {
            min,
            mean,
            max,
            syscalls: syscalls_total,
            entries: entries_total,
        }));

        println!();
    }

    // Summary table
    println!("=== summary ===");
    println!(
        "{:<10} {:>8} {:>8} {:>8} {:>10} {:>14}",
        "buf size", "min", "mean", "max", "syscalls", "entries/call"
    );
    println!("{}", "-".repeat(66));

    let best_min = all_results
        .iter()
        .map(|(_, r)| r.min)
        .fold(f64::INFINITY, f64::min);

    for (buf_size, result) in &all_results {
        let entries_per_call = if result.syscalls > 0 {
            result.entries as f64 / result.syscalls as f64
        } else {
            0.0
        };
        let marker = if (result.min - best_min).abs() < 0.001 {
            " <-- best"
        } else {
            ""
        };
        println!(
            "{:<10} {:>7.3}s {:>7.3}s {:>7.3}s {:>10} {:>13.1}{}",
            format_buf_size(*buf_size),
            result.min,
            result.mean,
            result.max,
            result.syscalls,
            entries_per_call,
            marker,
        );
    }

    // Recommendation
    println!();
    let (best_size, _) = all_results
        .iter()
        .min_by(|(_, a), (_, b)| a.min.partial_cmp(&b.min).unwrap())
        .unwrap();
    let current = 256 * 1024;
    let (_, current_result) = all_results
        .iter()
        .find(|(s, _)| *s == current)
        .unwrap();
    let (_, best_result) = all_results
        .iter()
        .find(|(s, _)| s == best_size)
        .unwrap();

    if *best_size == current {
        println!("verdict: current 256 KB buffer is already optimal");
    } else {
        let diff_pct = (current_result.min - best_result.min) / current_result.min * 100.0;
        println!(
            "verdict: {} is {:.1}% faster than current 256 KB",
            format_buf_size(*best_size),
            diff_pct,
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
// Walk with configurable buffer size
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
struct WalkStats {
    entries: u64,
    syscalls: u64,
}

#[cfg(target_os = "macos")]
fn walk_with_bufsize(root: &std::path::Path, buf_size: usize) -> WalkStats {
    let root_dev = get_dev(root);
    let mut stats = WalkStats {
        entries: 0,
        syscalls: 0,
    };
    walk_bufsize_recursive(root, buf_size, root_dev, &mut stats);
    stats
}

#[cfg(target_os = "macos")]
fn walk_bufsize_recursive(
    dir_path: &std::path::Path,
    buf_size: usize,
    root_dev: Option<u64>,
    stats: &mut WalkStats,
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
    let mut buf = vec![0u8; buf_size];
    let mut subdirs: Vec<std::path::PathBuf> = Vec::new();

    loop {
        let count = unsafe {
            getattrlistbulk(
                fd,
                &alist as *const BulkAttrList,
                buf.as_mut_ptr() as *mut libc::c_void,
                buf_size,
                0,
            )
        };

        if count <= 0 {
            break;
        }
        stats.syscalls += 1;

        let mut offset = 0usize;
        for _ in 0..count {
            if offset + 4 > buf_size {
                break;
            }
            let entry_len =
                u32::from_ne_bytes(buf[offset..offset + 4].try_into().unwrap()) as usize;
            if entry_len == 0 || offset + entry_len > buf_size {
                break;
            }

            if let Some((name, is_dir, _size)) =
                parse_entry_minimal(&buf[offset..offset + entry_len])
            {
                stats.entries += 1;
                if is_dir {
                    let child_path = dir_path.join(&name);
                    if let Some(rd) = root_dev {
                        if get_dev(&child_path) != Some(rd) {
                            offset += entry_len;
                            continue;
                        }
                    }
                    subdirs.push(child_path);
                }
            }
            offset += entry_len;
        }
    }

    unsafe { libc::close(fd) };

    for subdir in subdirs {
        walk_bufsize_recursive(&subdir, buf_size, root_dev, stats);
    }
}

// ---------------------------------------------------------------------------
// Result type
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
struct BufSizeResult {
    min: f64,
    mean: f64,
    max: f64,
    syscalls: u64,
    entries: u64,
}

// ---------------------------------------------------------------------------
// Shared helpers
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
                let end = name_slice
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(name_slice.len());
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

fn format_buf_size(size: usize) -> String {
    if size >= 1024 * 1024 {
        format!("{} MB", size / (1024 * 1024))
    } else {
        format!("{} KB", size / 1024)
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
    let mut iterations: usize = 3;
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
                eprintln!("usage: bench_bufsize [-n N] [--warmup] [PATH]");
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
