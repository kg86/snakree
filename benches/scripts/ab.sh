#!/bin/sh
# Benchmarks two revisions with the current benchmark code and compares them.
#
#   benches/scripts/ab.sh BASE_REV [NEW_REV] [-- BENCH_OPTIONS...]
#
# NEW_REV defaults to the working tree, including uncommitted changes.
# The current benches/bench/ directory is copied into each revision, so both run the same workload.
# The revisions must provide the public API used by the benchmark.
# Results are kept in BASE.tsv and NEW.tsv in the current directory.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
base_rev=${1:?usage: $0 BASE_REV [NEW_REV] [-- BENCH_OPTIONS...]}
shift
new_rev=
if [ $# -gt 0 ] && [ "$1" != -- ]; then new_rev=$1; shift; fi
[ "${1:-}" = -- ] && shift

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"; git -C "$root" worktree prune' EXIT

prepare() { # rev dir
    git -C "$root" worktree add -q --detach "$2" "$1"
    rm -rf "$2/benches/bench" "$2/benches/bench.rs"
    mkdir -p "$2/benches"
    cp -R "$root/benches/bench" "$2/benches/bench"
    grep -q 'name = "bench"' "$2/Cargo.toml" ||
        printf '\n[[bench]]\nname = "bench"\nharness = false\n' >>"$2/Cargo.toml"
}

run() { # dir out
    (cd "$1" && cargo bench -q --bench bench -- "$@") >"$2"
}

prepare "$base_rev" "$tmp/base"
if [ -n "$new_rev" ]; then prepare "$new_rev" "$tmp/new"; new_dir=$tmp/new; else new_dir=$root; fi

(cd "$tmp/base" && cargo bench -q --bench bench -- "$@") >BASE.tsv
(cd "$new_dir" && cargo bench -q --bench bench -- "$@") >NEW.tsv
"$here/compare.sh" BASE.tsv NEW.tsv
