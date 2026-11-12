# snakree

Sliding-window suffix trees in Rust, based on Ukkonen's online construction.

`Snakree` provides online extension and contraction using primary leaf pointers.
Create one with `Snakree::new()`.

Positions, lengths, depths and offsets all use `usize`, measured in bytes.

## What it is

A suffix tree over a *window* of a stream, maintained online.
The window is the half-open range `T[begin..end)` of a stream `T`, and two operations move it:

- `push(c)` widens the window by appending a byte at its right end, moving `end` forward.
- `pop_front()` narrows the window by evicting the byte at its left end, moving `begin` forward.

`Snakree` is the leaf-pointer implementation described by the reference paper, Leonard et al., *Sliding Suffix Trees Revisited* (<https://arxiv.org/abs/2307.01412>).
The two ends of its window move independently; for a fixed-width window, call `pop_front` before `push` once the window is full.

Node references use `NodeId`, backed by `u32`, in both the public API and the implementation.
The public constant `snakree::MAX_WINDOW_LEN` limits the window to 2^31 - 1 bytes (2 GiB minus one byte), ensuring enough node IDs for any input.
When the window is already at this limit, `push` panics before changing the tree, regardless of its contents or number of nodes.
Memory allocation may fail below this limit, and the cumulative stream position is separately limited to `usize::MAX`.
Generic index widths and pattern occurrence counting are outside the scope of this project.

The tree is an *implicit* suffix tree: a suffix that also occurs earlier in the window has no leaf of its own, so leaves do not enumerate all occurrences of a pattern and cannot be used to count them.
`contains`, `descend` and `find` are unaffected.
`find` returns one occurrence only.
`count` (the number of occurrences) and `locate` (listing all occurrence positions), as provided by FM-index and suffix array libraries, are not supported.
A unique terminator cannot be used to avoid this, because the window grows at its right end.
See the crate documentation (`cargo doc --open`) for details.

## Usage

The crate is not published on crates.io; add it as a git dependency in `Cargo.toml`:

```toml
[dependencies]
snakree = { git = "https://github.com/kg86/snakree" }
```

```rust
let mut t = snakree::Snakree::new();
t.extend_from_slice(b"banana");
assert!(t.contains(b"nan"));
t.pop_front(); // the window is now "anana"
```

## Finding matches and reading text

`find(pattern)` returns the absolute byte position of an arbitrary occurrence, or `None` if absent.
It does not guarantee the first or last occurrence; an empty pattern returns `Some(begin())`.
`range()` returns the current window as `begin()..end()`.
`slices(range)` borrows an absolute byte range without copying, returning two slices in stream order because the circular buffer may wrap.
`copy_range(range, &mut dst)` appends that range to a byte vector, while `window(&mut dst)` appends the entire window.
Both range accessors accept empty ranges at either endpoint and panic on reversed or out-of-window ranges.

For example, to copy a match with up to five bytes of surrounding context:

```rust
let mut t = snakree::Snakree::new();
t.extend_from_slice(b"before banana after");
let pattern = b"banana";
if let Some(pos) = t.find(pattern) {
    let start = pos.saturating_sub(5).max(t.begin());
    let end = (pos + pattern.len()).saturating_add(5).min(t.end());
    let mut context = Vec::new();
    t.copy_range(start..end, &mut context);
    assert_eq!(context, b"fore banana afte");
}
```

Positions and context lengths are in bytes, so UTF-8 character boundaries must be handled by the caller.
Borrowed slices prevent updates to the tree while they are in use; copied bytes can be retained after the window advances.

## Repeated suffix queries

The `Snakree::repeated_suffix()` method returns the absolute position of an earlier occurrence and the length of the longest repeated suffix, or `(0, 0)`.
It lets a client such as a greedy LZ77 parser find matches without changing Ukkonen's update algorithm.

## Design

See [DESIGN.md](DESIGN.md) for the rationale behind the API: why there is no separate fixed-width window type, why positions are plain `usize`, how `Point` addresses a locus, and why edge labels store absolute positions.

`Snakree` uses a growing circular text buffer and reuses deleted node slots.
Allocated storage follows the largest window seen so far.
Children are stored as sibling lists, with lookup scanning at most 256 byte-valued edges.
Updates take amortized constant time for this fixed alphabet; individual updates can do more work.
Edge labels are always inside the current window.
`NodeId`s must be reacquired after each `push` or `pop_front`, and a `Point` borrows its tree, so the borrow checker enforces this for points.

Within the crate, [`snakree.rs`](src/snakree.rs) holds the tree state and update operations, [`query.rs`](src/query.rs) groups tree accessors and queries with their read-only helpers, and [`text.rs`](src/text.rs) encapsulates the circular buffer and absolute window bounds in the private `WindowText` type.
[`node.rs`](src/node.rs) encapsulates node storage, allocation, reuse, and ID validation in the private `NodeArena` type.
[`point.rs`](src/point.rs) defines node handles and operations on tree loci.

Unresolved improvement proposals and their validation criteria are tracked in [IDEAS.md](IDEAS.md).

## Toolchain

`rust-toolchain.toml` selects the stable channel; run `rustup update stable` to update the compiler, formatter, and Clippy.
CI tests stable and the minimum supported Rust version, 1.75, with the lockfile.

## Tests

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --release
```

Tests compare `Snakree` with a test-only O(d²) reference tree that is rebuilt after each update, after exhaustive and randomized update sequences, including topology, suffix links, repeated suffixes, queries, leaf-pointer invariants, buffer wrapping/growth, and node reuse.

See [DEVELOPMENT.md](DEVELOPMENT.md) for local API documentation setup.

## Reference

Esko Ukkonen, [*On-line construction of suffix trees*](https://www.cs.helsinki.fi/u/ukkonen/SuffixT1withFigs.pdf), Algorithmica 14(3), 249–260, 1995, [doi:10.1007/BF01206331](https://doi.org/10.1007/BF01206331).

Laurentius Leonard, Shunsuke Inenaga, Hideo Bannai, Takuya Mieno, *Sliding Suffix Trees Revisited*, <https://arxiv.org/abs/2307.01412>.

## Citation

If you use snakree in academic work, please cite it.
GitHub shows the citation under "Cite this repository", which reads [CITATION.cff](CITATION.cff).

## License

MIT. See [LICENSE](LICENSE).
