# Improvement ideas

This file records undecided improvement proposals and the validation needed to decide whether to adopt them.
[DESIGN.md](DESIGN.md) is authoritative for the current specification and design decisions.
When a proposal is adopted or rejected, reflect the conclusion and its reason in `DESIGN.md`, and delete it from this file if appropriate.

## Share storage for `depth` and `suffix_start`

**Status:** Not yet examined

`TreeNode.depth` is valid only for internal nodes, and `TreeNode.suffix_start` is valid only for non-root leaves.
They might be shareable as a single `usize` field.
In Rust this can be implemented without introducing `unsafe` or a union, by using a plain `usize` whose meaning switches according to the node kind.

### Expected benefits

- `TreeNode` might shrink from 64 bytes to 56 bytes.
- Memory use and cache footprint of the node array can be reduced for large windows.

### Concerns

- The meaning of the field depends on the node kind.
- The type system cannot prevent confusing an internal-node value with a leaf value.
- A name such as `depth_or_suffix_start` is less readable than the current separate fields.
- It changes the current layout in which one node fits in 64 bytes, the size of a typical cache line, so actual speed does not necessarily improve.

### Required invariants

- For a non-root leaf, only `suffix_start` is valid.
- For an internal node, only `depth` is valid.
- A non-root leaf is never turned into an internal node in place.
  When splitting an edge, a new branch node is allocated and the original leaf stays a leaf.
- Before an internal node would become a non-root leaf, it is merged and freed as a unary node.
- Even if the root structurally becomes a leaf in the empty tree, the shared field is not read as `suffix_start`.

### Validation items

- Confirm with `std::mem::size_of::<TreeNode>()` that the size actually becomes 56 bytes.
- Compare memory use on large windows.
- Compare benchmarks for construction, sliding update, and query.
- Confirm that the above invariants on node kind and valid field are preserved on every update path.

## Share storage for `plp` and `iplp`

**Status:** Not yet examined

`TreeNode.plp` is the pointer from a secondary node to the endpoint leaf of its primary path, and `TreeNode.iplp` is the inverse pointer from that endpoint leaf to the owner of the PLP.
By the primary path decomposition, exactly one PLP ends at each leaf, so only one inverse pointer is needed at a leaf.

An internal node needs only the forward `plp` and does not use `iplp`.
A leaf can return itself directly as a leaf of its subtree, so it does not need to store a forward `plp` and needs to store only the inverse `iplp`.
For a secondary leaf, `PLP(leaf) = leaf` and the owner of that PLP is the leaf itself, so the forward and inverse values coincide.
For this reason the two might be shareable as a single `NodeId` field.

The meaning of the shared field would be as follows.

| Node kind | Meaning of the shared field |
|---|---|
| Secondary internal node | Endpoint leaf of the primary path |
| Primary internal node | Unused |
| Leaf | Owner of the PLP that ends at this leaf |
| Root of the empty tree | The root itself |

If implemented, the places that read the shared field directly should be limited, and the meaning should be switched through helpers that fetch the forward PLP endpoint and the inverse owner respectively.
`set_plp(owner, leaf)` would also update the shared fields so that, if the owner is an internal node, the endpoint is recorded on the owner side and the owner is recorded on the leaf side.

### Expected benefits

- One `NodeId` field can be removed.
- This change alone might shrink `TreeNode` from 64 bytes to 56 bytes.
- Combined with sharing `depth` and `suffix_start`, it might shrink to 48 bytes including alignment.
- Memory use and cache footprint of the node array can be reduced for large windows.

### Concerns

- The meaning of the field depends on leaf / internal and primary / secondary status.
- When a leaf is promoted to primary, the slot corresponding to `plp` cannot be cleared as before.
  That slot must continue to hold the inverse PLP owner.
- After an internal node is made primary, the old endpoint left in its shared field has no meaning.
  It must be decided whether to clear the unused value explicitly, or to tolerate the stale value by never reading it outside the helpers.
- The root of the empty tree structurally becomes a leaf, but it must be handled separately from an ordinary suffix leaf.

### Required invariants

- Primary / secondary edges decompose the tree into disjoint primary paths.
- Secondary nodes and leaves are in one-to-one correspondence, and exactly one PLP owner ends at each leaf.
- The leaf pointer query does not read the shared field for the leaf itself and can return that leaf directly.
- For a secondary leaf, the endpoint and the owner are both the leaf itself.
- A non-root node never changes in place between leaf and internal node.
  Edge splits allocate a new branch node, and a unary internal node is freed before it would become a leaf.

### Validation items

- For all insertion and deletion cases in Sections 4.1--4.5 of the paper, confirm that the meaning of the shared field is preserved across each update.
- In particular, when a primary leaf is deleted and a secondary sibling is promoted, confirm that the owner of the deleted leaf is stored as the inverse pointer of the new endpoint leaf.
- Confirm with `std::mem::size_of::<TreeNode>()` that the size becomes 56 bytes alone, and 48 bytes when combined with sharing `depth` / `suffix_start`.
- Run the deletion case tests, the differential tests, and random Extend / Contract sequences.
- Compare memory use on large windows and benchmarks for construction, sliding update, and query.

## Cache the suffix start of the PLP endpoint in `suffix_start`

**Status:** Not yet examined

The current processing to obtain an edge label finds the `NodeId` of the PLP endpoint leaf from the target node and reads `suffix_start` of that leaf.
For a secondary internal node this is the dependent load `node -> plp -> leaf.suffix_start`, and for a primary internal node it goes through `plp` of a secondary child, so an indirect reference to the endpoint leaf is likewise needed.

Keeping `plp` and `iplp` because they are needed to re-link PLPs, consider caching in `suffix_start` of a secondary internal node the start position of the suffix represented by the PLP endpoint leaf.
`suffix_start` is currently unused for internal nodes, so its meaning can be switched as follows without adding a field.

| Node kind | Meaning of `suffix_start` |
|---|---|
| Leaf | Start position of the suffix the leaf represents |
| Secondary internal node | Start position of the suffix the PLP endpoint leaf represents |
| Primary internal node | Unused |
| Root of a non-empty tree | Start position of the suffix the root's PLP endpoint leaf represents |
| Root of the empty tree | Unused |

For the edge to a secondary internal node `v`, the cached position can be used as `k` to return `[k + Depth(Parent(v)), k + Depth(v))` directly.
For a primary internal node `v`, a secondary child `y` is chosen from the first two children, and `y.suffix_start` is used as `k`.
If `y` is a leaf it holds its own suffix start position, and if `y` is an internal node it holds the suffix start position of its PLP endpoint, so in either case the endpoint leaf itself need not be referenced.

`set_plp(owner, leaf)` also sets `owner.suffix_start = leaf.suffix_start`, in addition to `plp` and `iplp`.
On the `pop_front` path that reuses a leaf for the active suffix and changes `suffix_start` of the same leaf node, the owner is obtained from `iplp` and its cache is updated at the same time.
The depth of a leaf is derived from `End - suffix_start` as before, and not all leaves are updated on every `push`.

### Expected benefits

- One dependent load from the `edge` hot path to the PLP endpoint leaf can be removed.
- On large trees where the endpoint leaf is not in the CPU cache, edge label retrieval and rescan might become faster.
- The existing `suffix_start` slot is reused, so `TreeNode` can stay at 64 bytes.
- `plp` is kept, so the worst-case O(1) of insertion and deletion using the inverse PLP does not change.
- It is compatible with the `plp` / `iplp` sharing proposal.

### Concerns

- Each PLP creation or re-link adds one `usize` write.
- It introduces a new failure mode in which `plp` and the cached suffix start position become inconsistent.
- Leaf reuse changes neither the topology nor the `NodeId` of the PLP endpoint, so checking only ordinary PLP updates makes it easy to overlook the cache update.
- The effect may be small on shallow trees dominated by leaf edges, or on inputs where child lookup dominates.
- `suffix_start` of an internal node holds a valid value, so this is not compatible with the `depth` / `suffix_start` sharing proposal above.
- The meaning of the field depends on leaf / internal and primary / secondary status, which is more complex than the current leaf-only meaning.

### Required invariants

- For a secondary internal node `v`, `v.suffix_start == nodes[v.plp].suffix_start` always holds.
- For a leaf, primary or secondary, `suffix_start` holds the start position of the suffix it represents.
- `suffix_start` of a primary internal node is never read.
  A stale cache left after promotion from secondary to primary has no meaning.
- The secondary child used to obtain the edge label of a primary internal node can be chosen from the first two children in worst-case O(1).
- When a leaf is reused and `suffix_start` of the PLP endpoint leaf changes, the cache of the owner pointed to by `iplp` of that leaf is changed within the same update.

### Validation items

- For all insertion and deletion cases in Sections 4.1--4.5 of the paper that go through `set_plp`, check that the PLP endpoint and the cached value agree.
- Test separately the leaf reuse for the active suffix, the transition to the empty tree, and the re-creation of the root's PLP.
- Confirm with differential tests and fuzz tests that all edge labels and the active point match those before the change.
- Confirm with `std::mem::size_of::<TreeNode>()` that the size stays at 64 bytes.
- In addition to sliding update, compare a query benchmark that iterates over the `Edge` of every internal node, a construction benchmark with many rescans, and large windows that exceed the CPU cache.
- Measure the overall performance, including the extra writes on update, several times.

## Remove `child_count`

**Status:** Deferred after measurement.
The counter is retained: a controlled five-round comparison was about 8–10% slower without it on two Silesia prefixes.
See `DESIGN.md` section 9 for the measured conditions and decision.
The following describes the remaining memory-oriented experiment; removing the public `num_children()` API is not required.

If `num_children` is removed from the public API, the only distinction the current update processing needs from `TreeNode.child_count` is whether a node has 0, 1, or 2 or more children.
These three states can be determined in worst-case O(1) by examining the first two elements of the sibling list.

```rust
fn has_one_child(&self, v: NodeId) -> bool {
    let child = self.n(v).first_child;
    !child.is_nil() && self.n(child).next_sib.is_nil()
}

fn has_at_least_two_children(&self, v: NodeId) -> bool {
    let child = self.n(v).first_child;
    !child.is_nil() && !self.n(child).next_sib.is_nil()
}
```

The correspondence is as follows.

| Current check | Check using the sibling list |
|---|---|
| `child_count == 0` | `first_child == NodeId::NIL` |
| `child_count == 1` | `first_child != NodeId::NIL && first_child.next_sib == NodeId::NIL` |
| `child_count > 1` | `first_child.next_sib != NodeId::NIL` |

The `child_count > 2` used in the tests that classify the paper's deletion cases can also be decided in constant time by checking whether a third sibling exists.
Debugging code that needs the exact number of children scans and counts `children`.

### Expected benefits

- One `usize` field per node can be removed.
- The writes that update the counter in `attach` / `detach` become unnecessary.
- Inconsistency between the sibling list and the counter can no longer occur.
- This change alone might shrink `TreeNode` from 64 bytes to 56 bytes.
- Combined with sharing `depth` / `suffix_start` and `plp` / `iplp`, it might shrink to 40 bytes including alignment.

### Concerns

- Removing `num_children` is a breaking change to the public API.
- Deciding the number of children reads `next_sib` of the child node in addition to `first_child`, so there is one more dependent load than comparing with the counter.
  Including the smaller fields and fewer writes on update, which is better cannot be decided without measuring.
- If processing that frequently needs the exact number of children is added in the future, a scan will be needed each time.

### Required invariants

- The sibling list is correctly terminated, and `next_sib` of the last child is `NodeId::NIL`.
- The main update algorithm does not need the exact number of children and needs to distinguish only 0, 1, and 2 or more.
- The unary check after `delete_leaf` is done after the target leaf is removed from the sibling list.

### Validation items

- Check the API impact of removing `num_children` from the interface, both implementations, helpers, and tests.
- Replace each `child_count` condition with a sibling-list check, and confirm that the deletion case tests of Sections 4.3--4.5 of the paper go through the same cases.
- Confirm with `std::mem::size_of::<TreeNode>()` that the size becomes 56 bytes alone, and 40 bytes when combined with the other two sharing proposals.
- Compare benchmarks for construction, sliding update, and query, and measure the trade-off between the dependent load and the reduced cache footprint.

## Introduce edge-oriented suffix links into the sliding suffix tree

**Status:** Not yet examined

Consider applying the edge-oriented top-down scheme of Larsson, Fuglsang, and Karlsson, *Efficient Representation for Online Suffix Tree Construction*, to `Snakree`.
With ordinary node-oriented suffix links, moving the active point to the next suffix requires a branch lookup for the target outgoing edge right after following the node's suffix link.
An edge-oriented suffix link keeps the result of this first lookup directly as a link from edge to edge, which reduces branch lookups during rescan.

Let the string marking an edge `e` be the shortest string that edge represents.
If the string marking `e` is `aA` and `A` marks another edge `e'`, the edge suffix link of `e` points to `e'`.
In the current structure, where a node and its incoming edge are represented by the same record, the edge link might also be stored as the `NodeId` of the destination node.

This scheme was proposed for right-end extension only, so applying it to a sliding suffix tree requires additionally defining how to maintain edge links under left-end deletion and under edge merging of unary internal nodes.

### Left-end leaf deletion

When the oldest leaf, which represents the whole window, is deleted, no incoming edge suffix link points to its leaf edge.
Let `A` be the shortest string marking the leaf edge.
Then `A` is unique in the window, and its only occurrence starts at the left end of the window.
If a source edge of an incoming link existed, that edge would be marked by `cA`, so a character `c` inside the window would precede `A`, which is a contradiction.
Therefore the leaf edge itself can be deleted without repairing any incoming link.

### Edge merging of a unary internal node

When leaf deletion makes `u` in `p -> u -> v` unary, the record of the upper edge `p -> u` is kept with its label extended to `p -> v`, and the record of the lower edge `u -> v` is deleted.
This policy is the inverse of the edge-oriented top-down scheme, which keeps the original record as the upper edge when splitting an edge, and it preserves existing incoming links that point to the upper edge.

On the other hand, an incoming edge suffix link may exist for the lower edge being deleted.
For example, for the window `abaaab`, the relevant part has the following shape.

```text
root
 `- "a" -> node "a"
      |- "baaab" -> leaf("abaaab")  // leaf removed by left-end deletion
      `- "a"     -> node "aa"
           |- "ab" -> leaf("aaab")
           `- "b"  -> leaf("aab")
```

The edge `"aa" -> "aaab"` is marked by `aaa`, so its edge suffix link points to the lower edge `"a" -> "aa"` marked by `aa`.
If, after deleting `leaf("abaaab")`, `root -> "a" -> "aa"` is merged into `root -> "aa"` and the lower record is freed, this link must be re-linked to the upper record that was kept.

Candidate maintenance methods include the following.

- Keep an inverse edge-link list per edge, and re-link incoming links to the upper edge when a lower edge is deleted.
- Temporarily keep the deleted edge record as a forwarding record, and shortcut the link lazily.
- Choose which record to keep according to the numbers of incoming links of the upper and lower edges.
- Separate edge identity from destination node identity, so that references to both stay stable after edge merging.

The source edge is marked by `cA`, so the number of incoming links to edges marked by the same `A` is at most one per character of the alphabet, that is, at most `sigma`.
For the byte alphabet this is at most 256, but re-linking all incoming links on every update costs `O(sigma)` for a general alphabet.
The main question is whether this repair can be done while keeping `O(d)` space and `O(n log sigma)` total time.

### Relation to PLP

The strongly-fresh edge labels from PLP and the definition of edge-oriented suffix links are compatible.
PLP provides the occurrence position of an edge label inside the window, and an edge suffix link is determined by the root-to-edge path label, so the meaning of a link does not change whichever descendant leaf PLP chooses.
By strong freshness, the edge label used for rescan and the root-to-parent path just before it also exist inside the window.

However, if the upper edge record is kept while a node and its incoming edge are still represented by the same record, it must be decided how to hand over what the lower node held before merging: the child set, `depth` / `suffix_start`, the primary status, the PLP state, and references from the leaf FIFO and the active point.
This is a problem of the current representation, which assigns edge identity and node identity to the same `NodeId`, not a problem of PLP correctness.
The child-set indirection of the original paper is also a candidate.

### Expected benefits

- The first `child` lookup in the rescan after `set_active_to_next_suffix` can be skipped.
- The number of branch lookups that scan the sibling list can be reduced.
- If it can replace node-oriented suffix links, there is no need to keep both schemes at the same time.
- If the deletion and edge-merging rules for edge-oriented suffix links in a sliding suffix tree can be made explicit, the result might be an independent research contribution in addition to an implementation improvement.

### Concerns

- How to repair incoming links to the lower edge deleted by unary merging is undecided.
- Inverse lists and forwarding records may add per-node space, writes on update, and branches on reference, which can offset the benefit of fewer branch lookups.
- The current public `suffix_link` has node-oriented semantics, so it would have to be removed or its API changed.
  It is not guaranteed that a node suffix link can be returned from an edge link in worst-case O(1).
- Keeping the upper edge record is the inverse of the current `merge_unary_node`, and a way is needed to keep it consistent with the PLP, the leaf FIFO, and the active point, which refer to `NodeId`s.
- The performance results of the original paper are for an extension-only suffix tree, and the same improvement is not guaranteed for sliding updates.

### Validation items

- Implement the split rule of the edge-oriented top-down scheme and the suffix transition of `push`, and compare the number of branch lookups and the construction benchmark with the node-oriented version.
- Confirm with an invariant and a test that no incoming edge suffix link exists for a leaf edge deleted at the left end.
- With examples including `abaaab`, confirm that incoming links to the lower edge are correctly re-linked to the kept edge on unary merging.
- Evaluate the time and space complexity of the number of incoming links to a lower edge and the repair cost, for both the fixed byte alphabet and a general alphabet.
- Compare node size and the number of references for the proposal that keeps edge and node records identical, the child-set indirection proposal, and the proposal that separates edge identity.
- Confirm with differential tests that all insertion and deletion cases of PLP, the active point, the leaf FIFO, and the strongly-fresh invariant of edge labels are preserved after edge splits and merges.
- Decide whether to remove the `suffix_link` API or replace it with edge-oriented semantics.
