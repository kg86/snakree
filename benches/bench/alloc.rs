//! Heap and resident-memory measurement: a counting allocator and the baseline taken around a measured structure.

use std::alloc::{GlobalAlloc, Layout, System};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Allocator wrapper that tracks live and peak heap bytes.
struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn add_live(n: usize) {
    let live = LIVE.fetch_add(n, Ordering::Relaxed) + n;
    PEAK.fetch_max(live, Ordering::Relaxed);
}

// SAFETY: every call is forwarded unchanged to `System`; the counters only observe sizes.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc(layout);
        if !p.is_null() {
            add_live(layout.size());
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc_zeroed(layout);
        if !p.is_null() {
            add_live(layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = System.realloc(ptr, layout, new_size);
        if !p.is_null() {
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
            add_live(new_size);
        }
        p
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

/// Starts a peak measurement and returns the live bytes at that moment.
fn reset_peak() -> usize {
    let live = LIVE.load(Ordering::Relaxed);
    PEAK.store(live, Ordering::Relaxed);
    live
}

/// Returns the peak heap growth since `base`, as returned by `reset_peak`.
fn peak_since(base: usize) -> usize {
    PEAK.load(Ordering::Relaxed).saturating_sub(base)
}

static RSS_PEAK: AtomicUsize = AtomicUsize::new(0);

/// Returns the resident set size of this process in bytes, or 0 if it cannot be read.
fn rss_bytes() -> usize {
    let kib = if cfg!(target_os = "linux") {
        std::fs::read_to_string("/proc/self/status").ok().and_then(|s| {
            let line = s.lines().find(|l| l.starts_with("VmRSS:"))?;
            line.split_whitespace().nth(1)?.parse::<usize>().ok()
        })
    } else {
        // There is no portable std API; `ps` reports the resident size in KiB on macOS and BSD.
        Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<usize>().ok())
    };
    kib.unwrap_or(0) * 1024
}

/// Records the current resident size; call it while the measured structure is still alive.
pub fn sample_rss() {
    RSS_PEAK.fetch_max(rss_bytes(), Ordering::Relaxed);
}

/// Memory baseline taken before a structure is built.
pub struct Baseline {
    heap: usize,
    rss: usize,
}

impl Baseline {
    pub fn start() -> Self {
        let rss = rss_bytes();
        RSS_PEAK.store(rss, Ordering::Relaxed);
        Baseline { heap: reset_peak(), rss }
    }

    /// Returns the growth of the peak heap bytes and of the sampled resident size.
    pub fn finish(&self) -> Mem {
        Mem { heap: peak_since(self.heap), rss: RSS_PEAK.load(Ordering::Relaxed) as i64 - self.rss as i64 }
    }
}

#[derive(Clone, Copy)]
pub struct Mem {
    pub heap: usize,
    /// Signed, because the OS can reclaim other pages of the process while the tree grows.
    pub rss: i64,
}
