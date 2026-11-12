#!/bin/sh
# Runs a named benchmark scenario and writes its files to a directory.
#
#   benches/scripts/run-scenario.sh NAME OUTDIR [--dry-run]
#
# A scenario is a shell file benches/scenarios/NAME.sh that sets `description`,
# `files` (corpus files in benches/data) and `bench_args` (options of the benchmark).
# OUTDIR receives env.txt, corpora-check.txt, scenario.txt, result.tsv and charts/.
# With --dry-run, the command is printed and nothing is run.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
name=${1:-}
out=${2:-}
dry=${3:-}
if [ -z "$name" ] || [ -z "$out" ]; then
    echo "usage: $0 NAME OUTDIR [--dry-run]" >&2
    echo "scenarios: $(ls "$here/../scenarios" | sed 's/\.sh$//' | tr '\n' ' ')" >&2
    exit 2
fi
scenario=$here/../scenarios/$name.sh
[ -f "$scenario" ] || { echo "unknown scenario: $name" >&2; exit 2; }

description=
files=
bench_args=
# shellcheck disable=SC1090
. "$scenario"

cd "$root"
file_args=""
for f in $files; do file_args="$file_args --file benches/data/$f"; done
cmd="cargo bench --bench bench -- $bench_args$file_args"

if [ "$dry" = --dry-run ]; then
    echo "scenario: $name ($description)"
    echo "$cmd > $out/result.tsv"
    exit 0
fi

mkdir -p "$out/charts"
{
    echo "scenario: $name"
    echo "description: $description"
    echo "command: $cmd"
} >"$out/scenario.txt"
{
    echo "branch: $(git branch --show-current)"
    echo "commit: $(git rev-parse HEAD)"
    echo "dirty: $(git status --porcelain --untracked-files=no | wc -l | tr -d ' ') tracked files changed"
    rustc -Vv
    uname -a
    if command -v sw_vers >/dev/null 2>&1; then sw_vers; fi
    if command -v sysctl >/dev/null 2>&1; then
        sysctl -n machdep.cpu.brand_string hw.memsize hw.ncpu 2>/dev/null || true
    fi
} >"$out/env.txt"

# Verifies the checksums of every corpus file; nothing is downloaded when the files are present.
benches/scripts/fetch-corpora.sh >"$out/corpora-check.txt"

# shellcheck disable=SC2086
$cmd >"$out/result.tsv"
benches/py/plot.py scaling "$out/result.tsv" "$out/charts"
