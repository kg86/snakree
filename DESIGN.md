# snakree design notes

An implementation of sliding-window suffix trees based on Ukkonen's online construction.
This document records why the types and the API are the way they are.
For usage, see the rustdoc (`cargo doc --open`).

The base online construction algorithm is Esko Ukkonen, [*On-line construction of suffix trees*](https://www.cs.helsinki.fi/u/ukkonen/SuffixT1withFigs.pdf), Algorithmica 14(3), 249–260, 1995, [doi:10.1007/BF01206331](https://doi.org/10.1007/BF01206331).

The reference paper for the sliding window and primary leaf pointers is Laurentius Leonard, Shunsuke Inenaga, Hideo Bannai, Takuya Mieno, *Sliding Suffix Trees Revisited*, <https://arxiv.org/abs/2307.01412>.

---

## 1. Overview

The structure maintains, online, an (implicit) suffix tree over a **window** `T[begin..end)`, which is a part of the stream `T[0..end)`.

| Operation | Meaning |
|---|---|
| `push(c)` | Appends one byte at the right end of the window (`end` increases) |
| `pop_front()` | Removes one byte from the left end of the window (`begin` increases) |

The only public tree is **`Snakree`**, which implements a suffix tree whose two ends move independently, using the paper's leaf-pointer scheme.
The paper handles windows of fixed width, but if both ends can move independently, a fixed-width window is expressed as "if full, `pop_front` and then `push`".
For that reason there is no separate type for fixed-width windows.
A fixed-width type would be unusable when the client wants to decide when to delete, for example to keep a match source.

The tree is an implicit suffix tree, which has no unique terminator, because online construction requires it.
The window grows at its right end, so the end of the text is not known when a suffix is inserted, and a terminator cannot be appended in advance.
The cost of this choice is described in section 7.

"snakree" is a proper name for this data structure, meant to convey that it indexes only the inside of the window and is maintained online while both ends move.
Its meaning is defined in the crate and type docs.

---

## 2. Public API policy

The public API consists of the concrete type `Snakree` and its methods, and no trait is public.
Derived operations such as `window`, `path`, `descend`, `contains` and `occurrence_start`, and convenient update operations such as `extend_from_slice`, are all methods of `Snakree`.
`find` combines a successful `descend` with `occurrence_start`, returning an arbitrary occurrence rather than promising occurrence order.
`range` exposes the absolute window bounds, while `slices` borrows any range inside them as two slices in stream order to accommodate the circular buffer.
`copy_range` appends an absolute range to a caller-owned vector, following the append semantics of `window`, `path` and `label`.
Reversed or out-of-window ranges panic, consistent with the error policy below.
Users have only one implementation to use, so the needs of differential testing are kept out of the public API.

The naive O(d²) reference implementation `NaiveTree` and the `TestTree` trait exist only under `cfg(test)` (section 8).
Derived operations are not shared logic, so `Snakree` computes `path` and `label` directly from its internal state, and tests can compare against an oracle built only from `edge` and `Point`.
This avoids the problem of errors in shared logic going undetected.

**Generic index types are out of scope.**
Both the public API and the internal node references are fixed to `NodeId`, whose underlying type is `u32`.

- The number of nodes is at most 2d for a window length d > 0 (an empty window has only the root).
  The public constant `MAX_WINDOW_LEN` is `u32::MAX as usize / 2`, or 2^31 - 1 bytes (2 GiB minus one byte), ensuring enough node IDs for any input while reserving ID zero for NIL.
  The length of the whole stream is not bounded by this: positions are `usize`, so the stream can be up to `usize::MAX` bytes (2^64 - 1 on 64-bit targets, about 16 EiB), and a window of less than 2 GiB can slide over it.
- The public accessors that may find no node (`parent`, `child` and `suffix_link`) return `Option<NodeId>`, so the type forces the caller to handle absence.
  `NIL` (value 0, with valid nodes numbered from 1) exists only inside the crate, as the sentinel in the per-node reference fields.
  Those fields must stay plain `u32`, because `Option<NodeId>` would double them to 8 bytes.
- The motivation for `u32` is the size of the node record.
  `TreeNode` holds eight node references (32 bytes), three `usize` fields (24 bytes) and the `primary` flag, which together with padding fit in one 64-byte cache line.
  With 64-bit references it would no longer fit, and the cost would be paid by every node in exchange for a window length that most clients never need.

Nodes live in an arena (`NodeArena`) and refer to each other by index rather than by pointer or `Rc`.
This keeps the tree free of `unsafe` and of self-references, lets freed slots be reused through a free list, and makes it possible to validate an ID against the arena (the checks described under the error policy below).

**Error policy.**
Absence that a correct caller can run into is expressed in the return type: `pop_front` on an empty window returns `None` and leaves the tree unchanged, and the accessors above return `Option<NodeId>`.
Violations of the documented contract, and exhaustion of the limits above (window length, stream positions, buffer capacity), panic instead of returning an error.
Limits are checked before numbering or advancing, so that a wrap-around cannot produce wrong references silently.
- Public accessors panic if the given ID is out of range or already freed.
  They do not recognize the same number issued by a different tree, or a number reused after being freed.
  A `NodeId` is valid only until the next `push` or `pop_front` of the tree that issued it.
- A `push` when `len() == MAX_WINDOW_LEN` panics before changing any state, even when the larger window's contents would need fewer nodes, so callers can evict a byte and retry if the cumulative stream position still permits another append.
  The arena retains a defensive node-ID overflow check, but the enforced window limit prevents valid updates from reaching it.

---

## 3. Suffix start positions are absolute (the circular buffer problem)

The text of the window is kept in a circular buffer of capacity C.
The private `WindowText` type owns the buffer and its absolute `begin` and `end` positions, including capacity growth and overflow checks.
`Snakree` delegates text access to it and advances the window's beginning only after contraction has finished repairing the tree, while the old text is still available.
The question is whether a node refers to a position in the text by **an offset within the buffer** or by **an absolute stream position** (the number of bytes pushed before that byte, which does not depend on `C`).

Let `T` be the whole input stream, so that `T[p]` is the byte at absolute position `p`, and let `buf` be the circular buffer, an array of `C` slots.
`T[p]` is stored in `buf[p % C]`.
When the buffer grows from `C` to `2C`, the same byte moves to `buf[p % 2C]`.
The new index `p % 2C` is either `p % C` or `p % C + C`, depending on which lap of the buffer `p` is on.
For example, with `C = 4` and the window `[6, 10)`, `T[6]` moves from `buf[2]` to `buf[6]`, while `T[8]` stays in `buf[0]`.

```text
      buf index:  0   1   2   3   4   5   6   7
before (C = 4):  p8  p9  p6  p7
after  (C = 8):  p8  p9   .   .   .   .  p6  p7
                  =   =                   ^   ^
               unchanged             moved by +C
```

`pN` stands for `T[N]`, and `.` is an unused element of `buf`.
`T[8]` and `T[9]` keep their indices, and `T[6]` and `T[7]` move to `index + C`.
So an offset stored in a node changes for some nodes and not for others.
Even though the lap could be recomputed from `begin`, the stored values themselves are stale after growth, so every node would have to be traversed and rewritten.
An absolute position `p` does not change when the buffer grows, and each access recomputes the slot as `p % C` with the current `C`.
The alternative of keeping a base and making the buffer non-circular needs a rebase when the window reaches the buffer's end, which comes to the same thing.
A rebase copies the window's contents back to the start of the buffer and moves the base accordingly, so every stored offset shifts by the same amount.

| Option for the suffix start position stored in a leaf | Rewriting node-side position information when the buffer grows | Memory for position information |
|---|---|---|
| **Keep an absolute `usize` position** | Not needed | 8B/leaf |
| Keep a `u32` offset within the buffer | Needed | 4B/leaf |
| Keep the low 32 bits of the absolute position | Not needed | 4B/leaf |

**→ The absolute position is adopted.**
It avoids rewriting the position information of every leaf when the buffer grows.
Allocating and copying the text buffer itself remains, but with doubling growth it costs amortized O(1) per appended byte.

The 32-bit variant is possible because every live leaf starts inside the window.
Restoring `begin + (stored - begin as u32)` with wrapping subtraction yields the absolute position without rewriting any node, provided the window is shorter than 2^32.
The 32-bit variant is not measurably faster, so it is not used for speed.
Narrowing `suffix_start` alone leaves `TreeNode` at 64 bytes because of padding; it shrinks to 48 bytes only when `depth` and `child_count` are also narrowed to `u32`.
This memory reduction is not adopted either.
`MAX_WINDOW_LEN` already keeps the window below 2^32, so adopting it would add no window-length limit and would not reduce the cumulative stream-position limit.

The implementation does not store the start position of each edge directly, and derives it from the suffix start position `suffix_start` of a leaf and the depths of the internal nodes.
If `k` is the start position of a leaf in the subtree of `v`, the edge is `[k + depth(parent(v)), k + depth(v))`.
The depth of a leaf is `end - suffix_start`, so its end is always `end`.
Primary leaf pointers (next section) give a valid leaf in O(1), so there is no need to traverse all old edge labels and update them.
Positions, lengths, depths and offsets are all `usize`, in bytes.

The private `NodeArena` type owns the node array, free list, live-node count, and ID validation.
It initializes the reserved NIL slot and live root directly; `Snakree` initializes the root's PLP and maintains all tree relationships.
The buffer and the node array are never shrunk, and deleted nodes are reused through a free list.
The allocated area is proportional to the maximum window length so far, not to the current window length.

### Scope of the complexity guarantee

The worst-case O(1) in the reference paper is a guarantee for maintaining leaf pointers at each node insertion and deletion, and for obtaining fresh edge labels using them.
It does not mean worst-case O(1) for `push` as a whole.
For example, appending `b` to `aaaa…a` creates many branches at once.
The construction time in the paper is O(n log σ) overall, and appending characters is evaluated by amortized complexity.

---

## 4. Primary / secondary and primary leaf pointers

To obtain the occurrence position of an edge label inside the current window, `Snakree` makes it possible to reach a leaf of the subtree from each node in O(1).
If every node held a simple leaf pointer, deleting a leaf could require updating many ancestors that point to that leaf.
So it uses the path decomposition by primary leaf pointers (PLP) from the paper.
This document only summarizes the mechanism as implemented here.
For the rationale of the scheme and the proofs of its invariants and complexity, see the paper.

### Primary / secondary is an attribute of the edge entering from the parent

Every node other than the root is classified as primary or secondary.
A node `v` is primary if `parent(v) -> v` is a primary edge.
Therefore "the primary child of `p`" refers to the destination of the primary edge leaving `p`.

The invariants maintained are as follows.

- Each node with children has exactly one primary child.
- All other children are secondary.
- The root has no parent, so it is always regarded as secondary.
- Leaves also have the primary / secondary distinction.
  It is not an attribute of internal nodes only.

`TreeNode.primary` says whether the node itself is the primary child of its parent.
The ID of the primary child is not stored separately on the parent side.

"Primary leaf pointer" is a single term, PLP, and does not mean "a pointer to a primary leaf".
The leaf at the end of a PLP may be primary or secondary.
For this reason, this document and the code comments use "primary leaf" alone only for a leaf that has primary status, and call the leaf a PLP points to the "PLP endpoint leaf".

### Primary paths and PLP

Following primary children repeatedly from a secondary node `s` reaches a leaf `u`, since the tree is finite.
This route is called the primary path of `s`, and `PLP(s) = u` is stored.
The primary path of a secondary leaf has length 0, and its PLP points to itself.

The choice of primary edges decomposes a non-empty tree into disjoint primary paths.
Each path starts at a secondary node and ends at a leaf.
Therefore, in a non-empty tree the following hold.

- Secondary nodes and leaves are in one-to-one correspondence.
- Exactly one PLP points to each leaf.
- The inverse pointer `iplp`, from a leaf back to the owner of the PLP, is also uniquely determined.
  This is why the PLP to update can be identified in O(1) when a leaf is deleted.

The implementation stores the endpoint leaf in `plp` of a secondary node, and the start secondary node in `iplp` of a leaf.
Therefore `PLP(s)` is defined only for a secondary node `s`, and `plp` of a primary node has no meaning.
It is never read even if NIL or a stale value from before the node became primary remains.
Only for the empty tree, `PLP(root) = root` by convention.

For example, for the window `aab`, one valid choice of primary edges is as follows.
The choice is not fixed by the window, and it changes as the tree is updated.

```
root (secondary, PLP = leaf0)
├─ "a" ── X (primary)
│         ├─ "ab" ── leaf0 = "aab" (primary)
│         └─ "b"  ── leaf1 = "ab"  (secondary, PLP = leaf1)
└─ "b" ── leaf2 = "b" (secondary, PLP = leaf2)
```

The primary paths are `root → X → leaf0`, `leaf1` and `leaf2`.
They start at the three secondary nodes (the root, `leaf1` and `leaf2`) and end at the three leaves, one for each.

### Getting a leaf from an arbitrary node

To build the fresh label of the edge `parent(v) -> v`, it is enough to know one leaf in the subtree of `v`.
`leaf_pointer(v)` returns it in O(1) by the following rules.

| Kind of `v` | Leaf returned |
|---|---|
| Leaf | `v` itself |
| Secondary internal node | `PLP(v)` |
| Primary internal node | `PLP(y)` of a secondary child `y` |

At the boundaries of update operations, a primary internal node is a branching node of the suffix tree, so it has two or more children.
There is only one primary child, so one of the first two children must be secondary.
Therefore the implementation can choose `y` by looking at the first two children at most, without searching all children.

With `k` the suffix start position of the obtained leaf, the label is the range given in section 3.
A leaf represents a suffix inside the current window, so not only this range but also the root-to-parent path just before it is inside the window.

### What a tree update changes

Node insertion, edge splitting and leaf deletion change the endpoint of an existing primary path as little as possible, and re-link PLP and `iplp` only when it does change.

- When adding a leaf to an existing node, if it already has children, the new leaf is made secondary and `PLP(leaf) = leaf` is created.
  If it is the first child, the leaf is made primary and becomes the endpoint of the parent's PLP.
  This "no children" case happens only at the root.
- When splitting an edge, if the original child is primary, the new branching node is also made primary, preserving the original primary path.
  If the original child is secondary, the branching node is also secondary, the new leaf is made primary, and the PLP of the branching node is pointed to the new leaf.
- When deleting a primary leaf, all remaining children of the parent are secondary, and the outcome depends on the parent.
  - If the parent becomes childless (only the root), `PLP(root) = root` (Case 3-1).
  - If the parent is the root, still has two or more children, or is primary and is merged away, the first remaining child `y` is promoted to primary.
    The PLP of the unique owner, found from `iplp` of the deleted leaf, is pointed to the endpoint of the new primary path through `y` (Cases 3-2, 4-1, 4-2).
  - If the parent is secondary and becomes unary, it disappears through merging, so the remaining child is kept secondary with its existing PLP (Case 4-3).
- Deleting a secondary leaf normally does not change the primary path.
  Only when a secondary parent also becomes unary and disappears, the secondary status and the PLP of the parent must be handed over to the remaining child.

The detailed branches of deletion, including edge merging when a parent becomes unary, correspond to Cases 3-1–5-3 in sections 4.3–4.5 of the paper.
In every case the number of PLPs changed is constant, so maintaining PLPs and obtaining leaf pointers take worst-case O(1) per node insertion or deletion.

---

## 5. `Point` — a position in the tree

A `Point` represents a position on a root-to-leaf path, including the middle of an edge.
It has `node()` and `offset()`.
`node()` is the lower end (the destination) of the edge that contains the point, and when the point is exactly on an explicit node, it is that node.
`offset()` is the number of bytes of that edge's label that lie above the point, counted from the top of the edge.
The path label of the point is as follows.

```
path(parent(p.node())) + label(p.node())[..p.offset()]
```

The invariant is `0 < offset <= edge_len(node)` (offset 0 only for the root).
`offset == edge_len` means being on an explicit end node.
The fields are private, so a user cannot rewrite the coordinates and break the invariant.
`descend` returns a new `Point` without changing the original one.

### Why measure from the top of the edge

**The edge of a leaf grows on every `push`** (an open leaf whose end is `end`), so a distance measured from the bottom end would point at a different string after each `push`.
For example, if a leaf edge is labeled `abc` and the point is after `ab`, the distance from the bottom is 1.
After `push('d')` the label is `abcd`, and a distance of 1 from the bottom now means `abc`.
A distance measured from the top is 2 both before and after, and `label[..2]` is `ab` in both cases.
So with the top as the reference, the string a point represents does not change when the target edge simply grows.
However, `push` and `pop_front` can also split edges, merge them and delete nodes.
The implementation corrects and normalizes the internal active point according to structural changes.

The field is named `offset` because `depth` would collide with `Point::depth` and `pos` means an absolute stream position in this crate.
It is an exclusive end index into the edge label, so `offset == edge_len` meaning "on the node" follows the same reasoning as `s[..s.len()]`.

### Separating the public `Point` from the internal `Locus`

`Point<'a>` borrows the tree it was obtained from and provides methods (`on_node`, `depth`, `descend`, `path`).
On the other hand, the position the tree itself holds (the active point) is represented by a private type `Locus { node, offset }` that does not borrow the tree.

- **No self-reference is created.**
  If the tree held a `Point`, it would be self-referential.
  A `Locus` is just data.
- **The lifetime is expressed in the type.**
  A public `Point` holds an immutable borrow of the tree, so the borrow checker enforces "valid until the next update".
  A `NodeId` holds no borrow and relies on documentation (section 2).
- **Internal checks do not go through the public accessors.**
  `Snakree` uses the unchecked `on_node` on a `Locus`.

---

## 6. Why `repeated_suffix` is a public method

`Snakree::repeated_suffix() -> (usize, usize)` returns the length of the current longest repeated suffix and the position of an earlier occurrence.
If the length is positive, it guarantees `begin <= pos < end - length` and that the strings at the two positions match.
If there is no repeated suffix, it returns `(0, 0)`.
How to choose among occurrences of the same length is unspecified.

What the user needs is the length of the longest repeated suffix and an earlier occurrence position, not the internal state of Ukkonen's algorithm (the active point) used to compute them.
Making the query a method has the following advantages.

- The promises about the active point, its lifetime and its normal form can be kept out of the public API.
- The reference implementation answers independently by exhaustive search, so the query logic is covered by differential testing.

`Snakree` answers in O(1) from the active point and does not modify the tree.
The active point spells a suffix of the window that also occurs earlier, so it is a proper prefix of some leaf's path, and the subtree below `active.node` contains a leaf strictly deeper than the repeated suffix.
(If the point is on an explicit node, that node is internal for the same reason.)
The suffix of the leaf obtained by `leaf_pointer` starts with the repeated suffix and ends at `end`, so its start position is before `end - length`.

---

## 7. Out of scope

- `count(pattern)` (the number of occurrences) and `locate(pattern)` (listing all occurrence positions) are not supported.
  In an implicit suffix tree the end of a repeated suffix is not a leaf, so the leaves of a subtree can neither count nor enumerate occurrences.
  A unique terminator cannot avoid this either (section 1).
- Generic index types (section 2).

---

## 8. Implementation and verification

`Snakree` performs right-end appends with Ukkonen's active reference pair and suffix links.
Explicit leaves are managed in a queue in the order of suffix start positions.
A left-end deletion removes the leaf at the head, and merges the edge if the parent becomes unary.
If the active point is in the middle of the edge of that leaf, the leaf is shortened for the active suffix and moved to the tail of the queue, and the active point is moved to the suffix one character shorter.

The invariants and update methods for primary / secondary and PLP are as in section 4.
Children are managed in sibling lists, and searching for a child scans at most 256 edges.
The sibling list was chosen because it keeps every node a fixed-size record (section 2) whatever its number of children.
A per-node table of 256 child slots would multiply the memory by the number of nodes and defeat the 64-byte node.
Nodes with many children are few in practice, and the lookup cost is bounded by the constant alphabet size.
The list is doubly linked (`prev_sib`), so that a child, such as a deleted leaf, is unlinked in O(1) without scanning its siblings.
Edge labels and PLP handling are worst-case O(1), and updates as a whole are amortized O(1) for this fixed byte alphabet.

`NaiveTree` rebuilds a compacted trie on every update by inserting all suffixes in decreasing length.
Suffix links are computed from path strings, and the longest repeated suffix and its occurrence position are computed by direct search.
It does not share the update logic of `Snakree`.

Differential tests compare edge labels, parent-child relations, paths, suffix links, the longest repeated suffix and query results, over exhaustive enumeration of short operation sequences, random window expansion and contraction, and fuzz-style inputs.
The active point of `Snakree` is private, so the tests read the internal state directly and check that its spelling matches the longest repeated suffix of the reference implementation.
`NodeId`s and the absolute reference positions of edge labels need not match between the implementations.
PLPs and inverse pointers, the uniqueness of primary, the leaf queue and the free list are also verified.
The empty window, all 256 byte values, long repeats, wrap-around and growth of the circular buffer, and node reuse are also test targets.
Operation sequences that slide at a fixed width are also differential-tested with several widths.

---

## 9. Keeping `child_count`

`TreeNode.child_count` is kept.
The alternative is to remove the counter, decide whether a node is unary from the first two sibling links, and keep `num_children()` by counting the sibling list.
The deletion predicates of that alternative were checked against sections 4.3–4.5 of the paper, and the differential tests passed.

On Apple Silicon, a greedy match-finding workload on the first 1 MiB of `xml` and `mozilla` from Silesia (five alternating runs, medians) was about 8% and 10% slower, respectively.
These are small inputs and do not cover all window widths, but they give no grounds for adopting the change for speed.
The alternative is therefore not adopted.
It should be reconsidered only when there is a concrete memory target and measurements on large windows, and the public child-count query is kept.
