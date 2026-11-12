//! Command-line options of the benchmark.

use std::path::PathBuf;

/// Parsed command-line options.
pub struct Options {
    pub size: usize,
    pub windows: Vec<usize>,
    pub reps: usize,
    pub seed: u64,
    pub queries: usize,
    pub pattern_lens: Vec<usize>,
    pub near_miss: bool,
    pub limit: Option<usize>,
    pub build_limit: usize,
    pub files: Vec<PathBuf>,
    pub no_synthetic: bool,
    pub only: Option<String>,
    pub row: Option<usize>,
}

const HELP: &str = "\
Options (pass after `--`):
  --size BYTES         length of each synthetic input (default 4194304)
  --window BYTES       sliding window width; repeatable (default 65536, 1048576 and 16777216)
                       rows whose window is longer than the input are skipped
  --reps N             timed repetitions per row, the median is reported (default 5)
  --seed N             seed of the synthetic inputs and queries (default 1)
  --queries N          number of queries per search row and pattern length (default 200000)
  --pattern-len N      length of the search patterns; repeatable (default 16)
  --near-miss          also time near misses, hit patterns whose last byte is replaced so that they are absent
  --file PATH          add a real-data input; repeatable
  --corpus DIR         add every file in DIR, such as the output of fetch-corpora.sh
  --no-synthetic       skip the synthetic inputs
  --limit BYTES        truncate every input to this many bytes
  --build-limit BYTES  bytes appended by the build scenario (default 8388608, the tree needs ~130 bytes per byte)
  --only NAME          run only scenarios containing NAME (build, slide, search)
";

pub fn parse_args() -> Options {
    let mut o = Options {
        size: 4 << 20,
        windows: Vec::new(),
        reps: 5,
        seed: 1,
        queries: 200_000,
        pattern_lens: Vec::new(),
        near_miss: false,
        limit: None,
        build_limit: 8 << 20,
        files: Vec::new(),
        no_synthetic: false,
        only: None,
        row: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = |name: &str| args.next().unwrap_or_else(|| panic!("{name} requires a value"));
        let num = |s: String| -> usize { s.parse().expect("expected an integer") };
        match a.as_str() {
            "--size" => o.size = num(value("--size")),
            "--window" => o.windows.push(num(value("--window"))),
            "--reps" => o.reps = num(value("--reps")).max(1),
            "--seed" => o.seed = num(value("--seed")) as u64,
            "--queries" => o.queries = num(value("--queries")),
            "--pattern-len" => o.pattern_lens.push(num(value("--pattern-len")).max(1)),
            "--limit" => o.limit = Some(num(value("--limit"))),
            "--build-limit" => o.build_limit = num(value("--build-limit")),
            "--file" => o.files.push(value("--file").into()),
            "--corpus" => {
                let dir = value("--corpus");
                let mut found: Vec<PathBuf> = std::fs::read_dir(&dir)
                    .unwrap_or_else(|e| panic!("cannot read {dir}: {e}"))
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
                    .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                    .map(|e| e.path())
                    .collect();
                found.sort();
                o.files.extend(found);
            }
            "--near-miss" => o.near_miss = true,
            "--no-synthetic" => o.no_synthetic = true,
            "--only" => o.only = Some(value("--only")),
            "--row" => o.row = Some(num(value("--row"))),
            "--help" | "-h" => {
                print!("{HELP}");
                std::process::exit(0);
            }
            // `cargo bench` appends `--bench`.
            "--bench" => {}
            other => panic!("unknown option {other}; see --help"),
        }
    }
    if o.pattern_lens.is_empty() {
        o.pattern_lens = vec![16];
    }
    if o.windows.is_empty() {
        o.windows = vec![64 << 10, 1 << 20, 16 << 20];
    }
    o
}
