# Local development notes

## Viewing the API documentation in a browser

Generate rustdoc at the repository root and open it in a browser.
No additional tools need to be installed.

```sh
cargo doc --no-deps --open
```

Without `--open`, the output is written to `target/doc/snakree/index.html`.

If you do not use a browser, read the doc comments (`///`) in the source directly.

## Check commands

Run the same checks as CI locally.

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --release
```

## Benchmarks

`benches/bench/` measures speed and memory.
It is not part of CI, and it needs no additional crates.

```sh
benches/scripts/fetch-corpora.sh
cargo bench --bench bench -- --corpus benches/data
cargo bench --bench bench -- --no-synthetic --file benches/data/xml --window 1048576 --reps 9
cargo bench --bench bench -- --help
```

### Corpora

`benches/scripts/fetch-corpora.sh` downloads the corpora into `benches/data`, which is ignored by Git.
It fetches the 12 Silesia files, the 50 MB files `dna`, `english`, `proteins`, `sources` and `dblp.xml` from Pizza&Chili, and from the Pizza&Chili repetitive corpus the real `cere` and the artificial `fib41` (Fibonacci), `tm29` (Thue–Morse) and `rs.13` (run-rich) strings.
The repetitive files are streamed and cut to 64 MiB (`MAX_BYTES`).
The artificial strings are the pathological cases.
Every file is verified against `benches/corpora.sha256`, which records the hashes observed on the first download, not hashes published by the corpus authors.
`UPDATE=1 benches/scripts/fetch-corpora.sh` rewrites that file.

### Scenarios

Every scenario runs on every input, and window-based ones run for every `--window` that is shorter than the input.
The default windows are 64 KiB, 1 MiB and 16 MiB; the last one exceeds the CPU cache.

- `build` appends the input with `extend_from_slice` and no eviction, up to `--build-limit` bytes.
- `slide` fills a window of fixed width, then times one `push` and one `pop_front` per remaining byte.
- `search-hit` builds the first window bytes once, then times `find` for each `--pattern-len` (repeatable, default 16) on patterns taken from the window.
  With `--near-miss`, `search-near-miss` is timed as well: a near miss is a hit pattern whose last byte is replaced so that the pattern is absent.
  It is off by default because measurements showed it to be as fast as a hit.

The synthetic inputs are `random` (uniform bytes) and `repetitive` (a random 4 KiB block repeated with about 0.1% mutated bytes), generated from `--seed`.
`--no-synthetic` skips them, and `--limit` truncates every input.

### Output

Output is one tab-separated row per measurement.
Every row runs in a fresh child process.
`median_ms` is the median of `--reps` runs, in which setup such as filling the window is not timed.
`peak_heap_bytes` is the largest heap growth observed by a counting global allocator, excluding the input buffers.
`rss_bytes` is the growth of the resident set size, sampled while the tree is alive at the end of each run; it is read from `/proc/self/status` on Linux and from `ps` elsewhere.
It is not a true peak, but it includes the memory the allocator keeps beyond what was requested.
It can be negative when the OS reclaims other pages of the process while the tree grows, so treat small or negative values as noise and rely on `peak_heap_bytes`.
`pattern_len` is the length of the search pattern, or `-` for the other scenarios.
`heap_per_window_byte` divides the heap peak by the window width, or by the appended bytes for `build`.
Search rows report the memory of the built tree.

Speed depends on the machine, so compare numbers only within one machine and one toolchain, and record both with the result.

### Scenarios

A scenario gives a name to a set of benchmark options.
`benches/scripts/run-scenario.sh NAME OUTDIR` runs it and writes the environment, the checksum check, the command, `result.tsv` and the charts into `OUTDIR`; `--dry-run` only prints the command.
The scenarios are the files `benches/scenarios/NAME.sh`, which set `description`, `files` (corpus files in `benches/data`) and `bench_args`.

- `silesia-quick`: every benchmark scenario on the 12 Silesia files cut to 4 MiB, windows of 64 KiB and 1 MiB, pattern lengths 1 to 64, and one repetition.
  It is for trying out the experiment process quickly, not for accurate numbers.

### Viewing results

Save the output and view it as a table or as CSV.

```sh
cargo bench --bench bench -- --corpus benches/data > result.tsv
column -t -s"$(printf '\t')" result.tsv | less -S
grep -v '^#' result.tsv | tr '\t' ',' > result.csv
```

The first line starts with `#` and records the options, so drop it for spreadsheets.
No field contains a comma or a tab.

### Comparing two implementations

`benches/scripts/ab.sh` runs the same benchmark code on two revisions and prints the ratios.

```sh
benches/scripts/ab.sh main                        # main against the working tree
benches/scripts/ab.sh v1 v2 -- --corpus benches/data --reps 9
```

The current `benches/bench/` directory is copied into temporary worktrees, so both revisions run the same workload.
`BASE.tsv` and `NEW.tsv` are kept in the current directory, and `benches/scripts/compare.sh BASE.tsv NEW.tsv` compares any two result files.
In the output, `time` and `heap` are NEW / BASE, so a value below 1 means NEW is faster or smaller.
The last line is the geometric mean of the time ratios.

Differences of a few percent are within noise on a laptop.
Use `--reps 9` or more, keep the machine otherwise idle, and repeat the comparison with the order of the revisions swapped before trusting a small difference.

### Charts

`benches/py/plot.py` draws SVG charts from the result files.
It is run by [uv](https://docs.astral.sh/uv/), which provides the Python interpreter from the script's inline metadata, so no virtual environment has to be set up or activated.
The script has no third-party dependencies: it writes the SVG directly, so the charts follow the viewer's light or dark scheme and show values on hover, which matplotlib cannot do in an SVG.

```sh
mkdir -p charts
benches/py/plot.py scaling result.tsv charts          # scaling, search, window-effect, memory, build
benches/py/plot.py compare BASE.tsv NEW.tsv charts    # compare-time.svg, compare-heap.svg
```

- `scaling.svg` shows `slide` throughput as a dumbbell chart with one dot per row: one row per input, one panel per window, on a shared log axis.
- `search.svg` shows `find` throughput for hits and near misses in the same form, for patterns of 16 bytes (or the middle measured length).
- `window-effect.svg` shows, per input, the `slide` speed at the largest window as a share of the speed at the smallest window.
- `memory.svg` shows, per input, the RSS growth of the `slide` run as a share of the heap peak at the largest window; the heap peak itself is the same for every input and is stated in the subtitle.
- `build.svg` ranks the inputs by construction throughput.
- `compare-time.svg` and `compare-heap.svg` show NEW / BASE per row on a log axis centred on 1, with the geometric mean in the subtitle.
  Rows whose heap size did not change are omitted from the heap chart.

Equivalently, `uv run benches/py/plot.py ...` works without the executable bit.
Every chart names the quantity and the unit of its x axis below the axis.
The files follow the light or dark scheme of the viewer, and hovering a mark in a browser shows its value.
`ab.sh` keeps `BASE.tsv` and `NEW.tsv`, so `benches/py/plot.py compare BASE.tsv NEW.tsv charts` draws its comparison.

