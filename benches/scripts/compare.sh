#!/bin/sh
# Compares two benchmark result files produced by `cargo bench --bench bench`.
#
#   benches/scripts/compare.sh BASE.tsv NEW.tsv
#
# Rows are matched on scenario, dataset, input size, window and pattern length.
# Columns are found by the names in the header line, so their order does not matter.
# `time` is NEW / BASE of median_ms, so a value below 1 means NEW is faster.
# `heap` is the same ratio for peak_heap_bytes.
set -eu
[ $# -eq 2 ] || { echo "usage: $0 BASE.tsv NEW.tsv" >&2; exit 2; }

awk -F'\t' -v OFS='\t' '
    /^#/ { next }
    $1 == "scenario" {
        delete col
        for (i = 1; i <= NF; i++) col[$i] = i
        next
    }
    {
        key = $col["scenario"] FS $col["dataset"] FS $col["input_bytes"] FS $col["window"] FS $col["pattern_len"]
        ms = $col["median_ms"]; heap = $col["peak_heap_bytes"]
    }
    FNR == NR { base_ms[key] = ms; base_heap[key] = heap; next }
    {
        if (!(key in base_ms)) { next }
        b = base_ms[key]
        printf "%s\t%s\t%s\t%s\t%s\t%.3f\t%.3f\t%.3f\t%.3f\n", $col["scenario"], $col["dataset"], $col["input_bytes"], $col["window"], $col["pattern_len"], b, ms, ms / b, (base_heap[key] > 0 ? heap / base_heap[key] : 0)
        n++
        lt += log(ms / b)
    }
    END {
        if (n > 0) printf "# geometric mean of time ratios over %d rows: %.3f\n", n, exp(lt / n)
    }
' "$1" "$2" | { printf 'scenario\tdataset\tinput_bytes\twindow\tpattern_len\tbase_ms\tnew_ms\ttime\theap\n'; cat; }
