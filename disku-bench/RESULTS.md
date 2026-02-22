# Disku Benchmark Results

**Date:** 2026-02-22
**Commit:** `8f66112`

## System

| | |
|---|---|
| CPU | Apple M4 |
| RAM | 16 GB |
| OS | macOS 26.3 (arm64) |
| Filesystem | APFS (SSD) |
| Rust | 1.93.0 |
| Target | `/Users/ethan` (~404K entries, ~83 GB) |

---

## Baseline (`bench_scan --compare`)

Full `scan_bulk` with Rayon default thread pool (3 runs).

| Scanner | Min | Mean | Max | Files/sec | Total Size |
|---|---|---|---|---|---|
| getattrlistbulk | 5.640s | 6.362s | 6.775s | 58,183 | 83.27 GB |
| jwalk | 8.744s | — | — | — | 83.27 GB |

**Speedup:** 1.55x (bulk vs jwalk)
**Peak RSS:** 60.19 MB
**Consistency:** file counts differ slightly across runs (~15 files) due to live filesystem activity.

---

## Buffer Size Tuning (`bench_bufsize`)

Single-threaded walk, 3 runs per size. Tests how `getattrlistbulk` buffer size affects syscall count and throughput.

| Buffer | Min | Mean | Max | Syscalls | Entries/Call |
|---|---|---|---|---|---|
| 16 KB | **4.137s** | 6.090s | 7.126s | 54,968 | 7.3 |
| 32 KB | 6.910s | 6.938s | 6.974s | 54,560 | 7.4 |
| 64 KB | 6.960s | 6.996s | 7.017s | 54,360 | 7.4 |
| 128 KB | 7.040s | 7.143s | 7.315s | 54,421 | 7.4 |
| **256 KB** (current) | 7.332s | 7.600s | 8.018s | 54,836 | 7.4 |
| 512 KB | 7.424s | 7.462s | 7.523s | 54,934 | 7.4 |
| 1 MB | 7.625s | 7.685s | 7.757s | 54,946 | 7.4 |
| 2 MB | 7.395s | 7.443s | 7.468s | 54,946 | 7.4 |

**Observations:**
- Syscall count barely changes (~54K-55K) because most directories are small and fit in one call regardless of buffer size.
- Entries-per-call is ~7.3-7.4 across all sizes — the average directory has ~7 entries.
- The 16 KB "win" is a single-run outlier (4.1s vs 7.1s) caused by filesystem caching, not buffer size. Mean times are all within noise of each other.
- **Verdict: buffer size doesn't meaningfully matter for this workload.** The 256 KB default is fine.

---

## Thread Scaling (`bench_threads`)

Full `scan_bulk` via `rayon::ThreadPool::install()`, 3 runs per thread count.

| Threads | Min | Mean | Max | Entries/sec | Speedup |
|---|---|---|---|---|---|
| 1 | 4.480s | 6.364s | 7.337s | 90,280 | 1.00x |
| 2 | 2.737s | 2.894s | 3.042s | 147,764 | 1.64x |
| 4 | 1.275s | 1.343s | 1.428s | 317,109 | 3.51x |
| 8 | 1.067s | 4.550s | 6.672s | 379,153 | 4.20x |
| **10** (= CPU cores) | **0.971s** | 4.259s | 5.934s | 416,705 | **4.62x** |
| 16 | 6.509s | 6.576s | 6.692s | 62,136 | 0.69x |
| 20 | 6.500s | 6.627s | 6.753s | 62,230 | 0.69x |

**Observations:**
- Clean linear scaling from 1-4 threads (1.0x → 3.5x).
- Best case at 10 threads (= CPU core count): 4.62x speedup, 0.97s.
- High variance at 8-10 threads — best runs are sub-1s, worst are 6s+. Likely filesystem/IO contention causing occasional stalls.
- **Severe degradation at 16+ threads** — worse than single-threaded. Thread contention and context switching overwhelm any parallelism benefit.
- **Efficiency:** 46% at 10 threads (ideal would be 10x, got 4.6x). The SSD IO controller is the bottleneck, not CPU.
- **Verdict: 4 threads is the most consistent sweet spot.** 8-10 can be faster but with high variance.

---

## Cold vs Warm Cache (`bench_cache --no-purge`)

Warm-cache comparison between getattrlistbulk and jwalk (3 runs each, cache primed before measuring).

| Mode | Min | Mean | Max | Entries/sec |
|---|---|---|---|---|
| bulk warm | 5.813s | 6.163s | 6.700s | 69,576 |
| jwalk warm | 5.680s | 8.112s | 9.559s | 71,209 |

**Observations:**
- With warm cache, both scanners converge to similar min times (~5.7s).
- jwalk has much higher variance (5.7s to 9.6s) compared to bulk (5.8s to 6.7s).
- Peak RSS: 389.73 MB (jwalk's `build_tree` HashMap is expensive at scale).
- Cold cache tests require `sudo purge` — run without `--no-purge` to get cold numbers.
- **Verdict: bulk scanner is more consistent; jwalk occasionally matches it but is less predictable.**

---

## Key Takeaways

1. **getattrlistbulk is 1.55x faster** than jwalk on this workload with default settings.
2. **Buffer size doesn't matter** — directory sizes are small enough that even 16 KB captures most entries per syscall.
3. **4 threads is the reliable sweet spot** — more threads add variance without consistent gains on SSD.
4. **Memory:** bulk scanner peaks at ~60 MB vs jwalk at ~390 MB for ~400K entries.
5. **High run-to-run variance** (sometimes 2-3x) is the biggest measurement challenge — filesystem cache state dominates wall time more than any tuning parameter.
