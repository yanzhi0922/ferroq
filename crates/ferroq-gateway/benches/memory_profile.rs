//! Memory profiling benchmark.
//!
//! This is not a criterion benchmark — it's a standalone binary that prints
//! memory usage at different stages. Run with:
//!
//! ```sh
//! cargo bench --bench memory_profile -p ferroq-gateway
//! ```

mod helpers;

use ferroq_gateway::bus::EventBus;
use ferroq_gateway::dedup::DedupFilter;
use ferroq_gateway::stats::RuntimeStats;
use std::sync::Arc;

/// Get current process memory usage (RSS) in bytes.
/// Falls back to 0 on unsupported platforms.
fn rss_bytes() -> u64 {
    #[cfg(target_os = "windows")]
    {
        // Use GetProcessMemoryInfo via std
        use std::mem::{size_of, zeroed};
        #[repr(C)]
        #[allow(non_snake_case)]
        struct ProcessMemoryCounters {
            cb: u32,
            PageFaultCount: u32,
            PeakWorkingSetSize: usize,
            WorkingSetSize: usize,
            QuotaPeakPagedPoolUsage: usize,
            QuotaPagedPoolUsage: usize,
            QuotaPeakNonPagedPoolUsage: usize,
            QuotaNonPagedPoolUsage: usize,
            PagefileUsage: usize,
            PeakPagefileUsage: usize,
        }
        unsafe extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn K32GetProcessMemoryInfo(
                hProcess: *mut std::ffi::c_void,
                ppsmemCounters: *mut ProcessMemoryCounters,
                cb: u32,
            ) -> i32;
        }
        unsafe {
            let mut pmc: ProcessMemoryCounters = zeroed();
            pmc.cb = size_of::<ProcessMemoryCounters>() as u32;
            let handle = GetCurrentProcess();
            if K32GetProcessMemoryInfo(handle, &mut pmc, pmc.cb) != 0 {
                return pmc.WorkingSetSize as u64;
            }
        }
        0
    }
    #[cfg(target_os = "linux")]
    {
        // Read /proc/self/statm
        if let Ok(statm) = std::fs::read_to_string("/proc/self/statm") {
            if let Some(rss_pages) = statm.split_whitespace().nth(1) {
                if let Ok(pages) = rss_pages.parse::<u64>() {
                    return pages * 4096;
                }
            }
        }
        0
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        0
    }
}

fn fmt_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn main() {
    println!("=== ferroq Memory Profile ===\n");

    let baseline = rss_bytes();
    println!("[baseline]       RSS = {}", fmt_bytes(baseline));

    // Create core components
    let bus = Arc::new(EventBus::new());
    let stats = Arc::new(RuntimeStats::new());
    let dedup = Arc::new(DedupFilter::new(60));

    let after_core = rss_bytes();
    println!(
        "[core init]      RSS = {} (delta +{})",
        fmt_bytes(after_core),
        fmt_bytes(after_core.saturating_sub(baseline))
    );

    // Subscribe 4 protocol servers
    let _subscribers: Vec<_> = (0..4).map(|_| bus.subscribe()).collect();

    let after_subs = rss_bytes();
    println!(
        "[4 subscribers]  RSS = {} (delta +{})",
        fmt_bytes(after_subs),
        fmt_bytes(after_subs.saturating_sub(after_core))
    );

    // Simulate 1000 events through dedup
    for i in 0..1000 {
        let event = helpers::make_message_event(123456, i);
        let _ = dedup.is_duplicate(&event);
        stats.record_event_for("bench");
    }

    let after_1k = rss_bytes();
    println!(
        "[1k events]      RSS = {} (delta +{})",
        fmt_bytes(after_1k),
        fmt_bytes(after_1k.saturating_sub(after_subs))
    );

    // Simulate 10,000 events
    for i in 1000..10_000 {
        let event = helpers::make_message_event(123456, i);
        let _ = dedup.is_duplicate(&event);
        stats.record_event_for("bench");
    }

    let after_10k = rss_bytes();
    println!(
        "[10k events]     RSS = {} (delta +{})",
        fmt_bytes(after_10k),
        fmt_bytes(after_10k.saturating_sub(after_1k))
    );

    // Simulate 100,000 events (stress test)
    for i in 10_000..100_000 {
        let event = helpers::make_message_event(123456, i);
        let _ = dedup.is_duplicate(&event);
        stats.record_event_for("bench");
    }

    let after_100k = rss_bytes();
    println!(
        "[100k events]    RSS = {} (delta +{})",
        fmt_bytes(after_100k),
        fmt_bytes(after_100k.saturating_sub(after_10k))
    );

    // Hold 1000 events in the bus channel
    for i in 0..1000 {
        let event = helpers::make_1kb_message_event(123456, i);
        bus.publish(event);
    }

    let after_bus_full = rss_bytes();
    println!(
        "[bus 1k×1KB]     RSS = {} (delta +{})",
        fmt_bytes(after_bus_full),
        fmt_bytes(after_bus_full.saturating_sub(after_100k))
    );

    println!(
        "\n[total]          RSS = {} (total delta +{})",
        fmt_bytes(after_bus_full),
        fmt_bytes(after_bus_full.saturating_sub(baseline))
    );

    println!("\n=== Memory Profile Complete ===");
}
