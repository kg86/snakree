# AGENTS.md

## What this repository implements

snakree implements online, window-based construction of implicit suffix trees over a sliding window of a byte stream.
`push` appends a byte at the right end of the window, `pop_front` evicts a byte from the left end, and both run in amortized constant time per call.
The implementation is a Rust crate at the repository root.

`Snakree` (src/snakree.rs) is the implementation agents will most often touch.
It follows the primary-leaf-pointer algorithm described in:

Laurentius Leonard, Shunsuke Inenaga, Hideo Bannai, Takuya Mieno,
*Sliding Suffix Trees Revisited*
<https://arxiv.org/abs/2307.01412>

`NaiveTree` (src/naive.rs) is a test-only O(d²) reference implementation used for differential testing against `Snakree`.
It does not implement the paper's algorithm and needs no correctness scrutiny beyond obvious straightforwardness.

## When to consult the paper

When implementing, modifying, or reviewing anything that touches:

- active-point/canonization logic (`active`, `implicit_suffix_count`, `on_node`, `set_active`, `set_active_to_next_suffix`),
- primary leaf pointers (`plp`, `iplp`, `primary`, `leaf_pointer`, `set_plp`), or
- the extension/contraction control flow in `push` and `pop_front`,

check the corresponding argument or invariant in the paper linked above rather than relying on intuition alone.
Comments in src/snakree.rs cite the paper for exactly this reason.
When in doubt about whether a change preserves an invariant, re-derive it from the paper instead of guessing.

## Text files

Text files managed in this repository, including Markdown, are written in English.
In Markdown and other prose text files, write one sentence per line and do not hard-wrap within a sentence.

## Doc comment style

Doc comments start with the description itself, not a restatement of the name they document.
Write "Returns the parent of v" rather than "Parent returns the parent of v".
The name is already given by the comment's position directly above the field or method, so restating it is redundant.
## Building and testing

Run Cargo commands from the repository root:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --release
```

CI runs the same three commands.

## Commits, pull requests, and experiment evidence

Commit and push code, tooling, and documentation changes directly to `main` by default.
Use a pull request against `main` only when the user explicitly requests one.
When a pull request is requested, work on a topic branch and merge using a merge commit so the pull request remains visible in the history.

Keep experiment artifacts on the long-lived `experiments` branch and never merge that branch into `main`.
When a change depends on measurements, summarize the method, results, limitations, and adoption decision in its commit message or accompanying documentation, or in the pull request description when one is requested.
Link to the experiment README using its full result commit hash so the evidence stays fixed as the branch advances.
A branch link may also be included for browsing current experiments.
Before a measurement, merge the committed code from `main` or a topic branch into `experiments`.
Use a topic branch when proposed code needs measurements before adoption on `main`; this does not require a pull request unless the user requests one.
Run and record the experiment there, then link the committed results when adopting the change on `main`.

## Running experiments

Before running an experiment, ask the user for what the experiment needs and cannot be decided from the request, and name the default that would be used otherwise:

- the purpose;
- the inputs, such as synthetic data and corpus files;
- the parameters, such as window widths, `--reps` (3 by default, 1 for a pilot run), `--limit` and `--build-limit`;
- the revisions to compare, if it is a comparison;
- the expected running time and memory use, when they are large.

Write the purpose and the method in the experiment README before the run, and add the results after it finishes.

## Other references

- experiments/README.md defines how experiments, including benchmark runs, are run and recorded.
  Read it before running one.
- README.md and DESIGN.md describe the public API, rationale, and testing approach.
  Read those for project conventions before making structural changes.
- Esko Ukkonen, *On-line construction of suffix trees*, Algorithmica 14(3), 1995, is the base algorithm `Snakree` extends with sliding-window support.
