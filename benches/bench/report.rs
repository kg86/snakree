//! The tab-separated result table.
//!
//! `COLUMNS` is the single definition of the table layout; `compare.sh` and `plot.py` look columns up by these names.

use crate::alloc::Mem;

const COLUMNS: [&str; 12] = [
    "scenario",
    "dataset",
    "input_bytes",
    "window",
    "ops",
    "median_ms",
    "rate",
    "unit",
    "peak_heap_bytes",
    "rss_bytes",
    "heap_per_window_byte",
    "pattern_len",
];

/// One measured row of the table.
pub struct Report {
    pub scenario: &'static str,
    pub dataset: String,
    pub input: usize,
    pub window: Option<usize>,
    pub ops: usize,
    pub unit: &'static str,
    pub secs: f64,
    pub mem: Mem,
    pub pattern_len: Option<usize>,
}

fn or_dash(v: Option<usize>) -> String {
    v.map_or("-".to_string(), |v| v.to_string())
}

impl Report {
    /// Returns the cells in the order of `COLUMNS`.
    fn cells(&self) -> [String; COLUMNS.len()] {
        [
            self.scenario.to_string(),
            self.dataset.clone(),
            self.input.to_string(),
            or_dash(self.window),
            self.ops.to_string(),
            format!("{:.4}", self.secs * 1e3),
            format!("{:.2}", self.ops as f64 / self.secs / 1e6),
            self.unit.to_string(),
            self.mem.heap.to_string(),
            self.mem.rss.to_string(),
            format!("{:.1}", self.mem.heap as f64 / self.window.unwrap_or(self.ops).max(1) as f64),
            or_dash(self.pattern_len),
        ]
    }

    /// Prints the row to standard output.
    pub fn print(&self) {
        println!("{}", self.cells().join("\t"));
    }
}

/// Prints the header line of the table.
pub fn print_header() {
    println!("{}", COLUMNS.join("\t"));
}
