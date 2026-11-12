//! The benchmark scenarios and the list of rows to run.

use std::hint::black_box;
use std::time::Instant;

use snakree::Snakree;

use crate::alloc::{sample_rss, Baseline, Mem};
use crate::cli::Options;
use crate::data::{Rng, Source};
use crate::report::Report;

#[derive(Clone, Copy)]
pub enum Scenario {
    Build,
    Slide,
    Search,
}

/// One unit of work that runs in its own child process.
pub struct Row {
    pub source: Source,
    pub scenario: Scenario,
    pub window: Option<usize>,
}

/// Lists the rows in a fixed order, so that a child process can find its row by index.
pub fn rows(o: &Options) -> Vec<Row> {
    let wants = |name: &str| o.only.as_deref().map_or(true, |s| name.contains(s));
    let mut sources = Vec::new();
    if !o.no_synthetic {
        sources.extend([Source::Random, Source::Repetitive]);
    }
    sources.extend(o.files.iter().cloned().map(Source::File));
    let mut v = Vec::new();
    for source in sources {
        let len = source.len(o);
        let mut push = |scenario, window| v.push(Row { source: source.clone(), scenario, window });
        if wants("build") {
            push(Scenario::Build, None);
        }
        for &w in &o.windows {
            if len <= w {
                continue;
            }
            if wants("slide") {
                push(Scenario::Slide, Some(w));
            }
            if wants("search") && o.pattern_lens.iter().any(|&l| w >= l) {
                push(Scenario::Search, Some(w));
            }
        }
    }
    v
}

/// Runs the row on its input and prints its result lines.
pub fn run_row(o: &Options, r: &Row) {
    let data = r.source.load(o);
    match (r.scenario, r.window) {
        (Scenario::Build, _) => bench_build(o, r, &data),
        (Scenario::Slide, Some(w)) => bench_slide(o, r, &data, w),
        (Scenario::Search, Some(w)) => bench_search(o, r, &data, w),
        _ => unreachable!("every windowed scenario has a window"),
    }
}

/// Runs `body` `reps` times and returns the median time.
/// `body` returns the time spent in the measured part so that setup can be excluded.
fn time_reps(reps: usize, mut body: impl FnMut() -> f64) -> f64 {
    let mut times: Vec<f64> = (0..reps).map(|_| body()).collect();
    times.sort_by(|a, b| a.total_cmp(b));
    times[times.len() / 2]
}

/// Measures a tree workload and returns the median seconds of `run` and the memory it needed.
/// `setup` builds the tree outside the timed part, `run` is the timed part.
fn measure(o: &Options, setup: impl Fn() -> Snakree, mut run: impl FnMut(&mut Snakree)) -> (f64, Mem) {
    let base = Baseline::start();
    let secs = time_reps(o.reps, || {
        let mut t = setup();
        let start = Instant::now();
        run(&mut t);
        let secs = start.elapsed().as_secs_f64();
        sample_rss();
        black_box(&t);
        secs
    });
    (secs, base.finish())
}

/// Describes a finished measurement of `scenario` on the row's input.
struct Outcome {
    scenario: &'static str,
    input: usize,
    window: Option<usize>,
    ops: usize,
    unit: &'static str,
    secs: f64,
    mem: Mem,
}

impl Row {
    fn report(&self, out: Outcome, pattern_len: Option<usize>) -> Report {
        Report {
            scenario: out.scenario,
            dataset: self.source.name(),
            input: out.input,
            window: out.window,
            ops: out.ops,
            unit: out.unit,
            secs: out.secs,
            mem: out.mem,
            pattern_len,
        }
    }
}

/// Builds a tree over the input without eviction.
fn bench_build(o: &Options, r: &Row, data: &[u8]) {
    let data = &data[..data.len().min(o.build_limit)];
    let (secs, mem) = measure(o, Snakree::new, |t| t.extend_from_slice(black_box(data)));
    let out =
        Outcome { scenario: "build", input: data.len(), window: None, ops: data.len(), unit: "Mbyte/s", secs, mem };
    r.report(out, None).print();
}

/// Fills a window of `w` bytes, then times one `push` and one `pop_front` per remaining byte.
fn bench_slide(o: &Options, r: &Row, data: &[u8], w: usize) {
    let (head, tail) = data.split_at(w);
    let fill = || {
        let mut t = Snakree::new();
        t.extend_from_slice(head);
        t
    };
    let (secs, mem) = measure(o, fill, |t| {
        for &c in black_box(tail) {
            t.push(c);
            black_box(t.pop_front());
        }
    });
    let out =
        Outcome { scenario: "slide", input: data.len(), window: Some(w), ops: tail.len(), unit: "Mslide/s", secs, mem };
    r.report(out, None).print();
}

/// Builds the first `w` bytes once, then times `find` for every pattern length.
/// A hit pattern is taken from the window.
/// A near miss is a hit pattern whose last byte is replaced by one that makes the pattern absent,
/// so the search follows the whole pattern before it fails.
/// Memory is that of the built tree.
fn bench_search(o: &Options, r: &Row, data: &[u8], w: usize) {
    let window = &data[..w];
    let mut rng = Rng(o.seed ^ 0xD1B5_4A32_D192_ED03 | 1);
    let base = Baseline::start();
    let mut t = Snakree::new();
    t.extend_from_slice(window);
    sample_rss();
    let mem = base.finish();
    for &len in o.pattern_lens.iter().filter(|&&l| l <= w) {
        let hits: Vec<usize> = (0..o.queries).map(|_| rng.below(w - len + 1)).collect();
        let near: Vec<Vec<u8>> = hits
            .iter()
            .take(if o.near_miss { hits.len() } else { 0 })
            .filter_map(|&i| {
                let mut p = window[i..i + len].to_vec();
                let orig = p[len - 1];
                let start = rng.byte();
                (0..=255u8).find_map(|k| {
                    let b = start.wrapping_add(k);
                    p[len - 1] = b;
                    (b != orig && t.find(&p).is_none()).then(|| p.clone())
                })
            })
            .collect();
        for (scenario, is_hit) in [("search-hit", true), ("search-near-miss", false)] {
            let ops = if is_hit { hits.len() } else { near.len() };
            if ops == 0 {
                continue;
            }
            let secs = time_reps(o.reps, || {
                let start = Instant::now();
                let mut found = 0usize;
                if is_hit {
                    for &i in &hits {
                        found += t.find(black_box(&window[i..i + len])).is_some() as usize;
                    }
                } else {
                    for p in &near {
                        found += t.find(black_box(p)).is_some() as usize;
                    }
                }
                let secs = start.elapsed().as_secs_f64();
                black_box(found);
                secs
            });
            let out = Outcome { scenario, input: data.len(), window: Some(w), ops, unit: "Mquery/s", secs, mem };
            r.report(out, Some(len)).print();
        }
    }
}
