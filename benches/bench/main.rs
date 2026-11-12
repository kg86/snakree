//! Reproducible benchmark for construction, fixed-width sliding and search.
//!
//! Run `cargo bench --bench bench -- --help` for the options.
//! Results are printed as tab-separated rows so that they can be pasted into a table.
//! Every row runs in a fresh child process, so that resident memory is not polluted by earlier rows.
//!
//! The benchmark is the directory `benches/bench/`; `ab.sh` copies the whole directory into the revisions it compares.

mod alloc;
mod cli;
mod data;
mod report;
mod scenarios;

use std::process::Command;

fn main() {
    let o = cli::parse_args();
    let rows = scenarios::rows(&o);
    if let Some(i) = o.row {
        scenarios::run_row(&o, &rows[i]);
        return;
    }
    println!(
        "# snakree bench: seed={} reps={} pattern_lens={:?} queries={}",
        o.seed, o.reps, o.pattern_lens, o.queries
    );
    report::print_header();
    let exe = std::env::current_exe().expect("cannot find the benchmark executable");
    let args: Vec<String> = std::env::args().skip(1).collect();
    for i in 0..rows.len() {
        let status = Command::new(&exe)
            .args(&args)
            .args(["--row", &i.to_string()])
            .status()
            .expect("cannot start a child process");
        assert!(status.success(), "row {i} failed");
    }
}
