use crate::leaf::LeafQueue;
use crate::node::{NodeArena, TreeNode, ROOT_ID};
use crate::point::{Locus, NodeId, MAX_NODE_ID};
use crate::text::WindowText;

/// The maximum window length in bytes: 2^31 - 1 (2 GiB minus one byte).
/// A nonempty window of length `d` has at most `2 * d` nodes, so this bound
/// keeps every node ID representable by `u32`, including the root and reserving
/// zero for NIL.
/// [`Snakree::push`] panics before changing the tree if its window is already
/// this long, regardless of its contents or number of nodes.
/// Memory allocation may fail below this limit; the cumulative stream position
/// is separately limited to `usize::MAX`.
pub const MAX_WINDOW_LEN: usize = MAX_NODE_ID / 2;

/// Maintains the implicit suffix tree of the window `T[begin..end)` of a
/// stream `T`, online: [`push`](Snakree::push) appends one byte to the right
/// end of the window and [`pop_front`](Snakree::pop_front) removes one from the
/// left end. It uses Ukkonen's online construction,
/// <https://www.cs.helsinki.fi/u/ukkonen/SuffixT1withFigs.pdf>, and primary
/// leaf pointers following Laurentius Leonard, Shunsuke Inenaga, Hideo Bannai,
/// Takuya Mieno, Sliding Suffix Trees Revisited,
/// <https://arxiv.org/abs/2307.01412>.
///
/// Accessors that may find no node, such as `parent` and `child`, return an
/// `Option`. Accessors given an out-of-range ID or a released node panic.
/// `NodeId`s are invalidated by the next `push` or `pop_front`.
/// The window length is limited to [`MAX_WINDOW_LEN`] bytes.
///
/// Child lookup scans at most 256 siblings (the byte alphabet). Edge, depth
/// and leaf-pointer maintenance take constant time. Updates take amortized
/// constant time for this fixed alphabet; an individual update may visit many
/// nodes. Storage is reused and is proportional to the largest window seen so
/// far.
pub struct Snakree {
    /// Owns node storage, allocation, and ID validation.
    pub(crate) arena: NodeArena,
    /// A circular buffer holding the current sliding window.
    pub(crate) text: WindowText,
    /// Tracks explicit suffix leaves in eviction order.
    pub(crate) leaves: LeafQueue,
    /// At operation boundaries, represents the longest suffix of the window
    /// that occurs at least twice. It is therefore never exactly at a leaf,
    /// though it may lie within an edge whose destination is a leaf.
    pub(crate) active: Locus,
    /// The number of suffixes represented implicitly without their own leaf.
    /// During `push`, it also counts the suffix extensions still being
    /// processed. At operation boundaries, it equals the active point's
    /// string depth and the length of the longest repeating suffix.
    /// Consequently, at those boundaries it is zero if and only if the active
    /// point is the root, meaning the window has no non-empty suffix that
    /// occurs at least twice.
    pub(crate) implicit_suffix_count: usize,
}

impl Default for Snakree {
    fn default() -> Self {
        Self::new()
    }
}

impl Extend<u8> for Snakree {
    fn extend<I: IntoIterator<Item = u8>>(&mut self, iter: I) {
        for c in iter {
            self.push(c);
        }
    }
}

impl<'a> Extend<&'a u8> for Snakree {
    fn extend<I: IntoIterator<Item = &'a u8>>(&mut self, iter: I) {
        for &c in iter {
            self.push(c);
        }
    }
}

impl Snakree {
    /// Creates an empty dynamic-window suffix tree. It allocates the reserved
    /// nil-node slot and the root.
    pub fn new() -> Self {
        let mut t = Snakree {
            arena: NodeArena::new(),
            text: WindowText::default(),
            leaves: LeafQueue::new(),
            active: Locus { node: ROOT_ID, offset: 0 },
            implicit_suffix_count: 0,
        };
        t.set_plp(ROOT_ID, ROOT_ID);
        t
    }

    #[inline(always)]
    pub(crate) fn n(&self, v: NodeId) -> &TreeNode {
        self.arena.get(v)
    }

    #[inline(always)]
    pub(crate) fn n_mut(&mut self, v: NodeId) -> &mut TreeNode {
        self.arena.get_mut(v)
    }

    /// Reports whether `l` sits exactly on its node, the unchecked
    /// counterpart of `Point::on_node` for loci the tree stores itself.
    #[inline]
    pub(crate) fn on_node(&self, l: Locus) -> bool {
        l.offset == self.raw_edge_len(l.node)
    }

    /// Returns a leaf in `v`'s subtree. A leaf returns itself, a secondary
    /// internal node returns its PLP, and a primary internal node returns the
    /// PLP of a secondary child (one of its first two children must be
    /// secondary).
    #[inline]
    pub(crate) fn leaf_pointer(&self, v: NodeId) -> NodeId {
        if self.raw_is_leaf(v) {
            return v;
        }
        let n = self.n(v);
        if !n.primary {
            return n.plp;
        }
        let mut child = n.first_child;
        if self.n(child).primary {
            child = self.n(child).next_sib;
        }
        self.n(child).plp
    }

    /// Makes `leaf` the PLP endpoint of secondary node `v` and records the
    /// inverse. Former counterparts are not cleared: callers must ensure their
    /// stale fields become unused through a change of primary status or node
    /// kind, or through release. In particular, adding the first leaf leaves
    /// the root's old inverse unused because the root becomes internal.
    #[inline]
    fn set_plp(&mut self, v: NodeId, leaf: NodeId) {
        self.n_mut(v).plp = leaf;
        self.n_mut(leaf).iplp = v;
    }

    /// Promotes secondary node `v` to primary and clears its now-unused
    /// forward PLP. Callers that hand `v`'s former PLP endpoint to another
    /// node must read it before calling this.
    #[inline]
    fn mark_primary(&mut self, v: NodeId) {
        let n = self.n_mut(v);
        n.primary = true;
        n.plp = NodeId::NIL;
    }

    /// Demotes primary node `v` to secondary, making `leaf` the endpoint of
    /// its new primary path, as a secondary node must own one.
    #[inline]
    fn mark_secondary(&mut self, v: NodeId, leaf: NodeId) {
        self.n_mut(v).primary = false;
        self.set_plp(v, leaf);
    }

    /// Makes `child` the new first child of `parent` under key `c`, splicing
    /// it into `parent`'s sibling list.
    fn attach_child(&mut self, parent: NodeId, child: NodeId, c: u8) {
        let first_child = self.n(parent).first_child;
        let n = self.n_mut(child);
        n.parent = parent;
        n.key = c;
        n.prev_sib = NodeId::NIL;
        n.next_sib = first_child;
        if !first_child.is_nil() {
            self.n_mut(first_child).prev_sib = child;
        }
        let p = self.n_mut(parent);
        p.first_child = child;
        p.child_count += 1;
    }

    /// Removes `child` from its parent and sibling list, leaving its other
    /// fields untouched so callers can reattach or release it afterward.
    fn detach_child(&mut self, child: NodeId) {
        let n = self.n(child);
        let (p, prev, next) = (n.parent, n.prev_sib, n.next_sib);
        if prev.is_nil() {
            self.n_mut(p).first_child = next;
        } else {
            self.n_mut(prev).next_sib = next;
        }
        if !next.is_nil() {
            self.n_mut(next).prev_sib = prev;
        }
        self.n_mut(p).child_count -= 1;
        let n = self.n_mut(child);
        n.parent = NodeId::NIL;
        n.prev_sib = NodeId::NIL;
        n.next_sib = NodeId::NIL;
    }

    /// Allocates a leaf under `p` with the requested primary status and enqueues it.
    /// A primary leaf becomes the endpoint of secondary parent `p`'s PLP; a secondary leaf owns a PLP pointing to itself.
    /// The caller must ensure `p` is secondary and has no other primary child when `primary` is true.
    fn new_leaf(&mut self, p: NodeId, c: u8, start: usize, primary: bool) -> NodeId {
        let v = self.arena.acquire();
        self.n_mut(v).suffix_start = start;
        if primary {
            self.mark_primary(v);
            self.set_plp(p, v);
        } else {
            self.set_plp(v, v);
        }
        self.attach_child(p, v, c);
        self.leaves.enqueue(&mut self.arena, v);
        v
    }

    /// Splits `v`'s incoming edge at `offset` and adds a new leaf along `c`.
    /// Returns the new branching node, which inherits `v`'s primary status;
    /// `v` keeps its status when reattached below it. Following Section 4.2
    /// of the cited paper:
    ///
    ///   - Case 2-1: `v` is primary, so the branch is primary and the new leaf
    ///     is secondary. Existing primary paths keep their leaf endpoints.
    ///   - Case 2-2: `v` is secondary, so the branch is secondary and the new
    ///     leaf becomes primary, with PLP(branch) pointing to it.
    fn split_edge(&mut self, v: NodeId, offset: usize, c: u8, start: usize) -> NodeId {
        let (b, _) = self.raw_edge(v);
        let split_at = self.at(b + offset);
        let parent = self.n(v).parent;
        let edge_key = self.n(v).key;
        let primary = self.n(v).primary;
        let branch = self.arena.acquire();
        let parent_depth = self.n(parent).depth;
        let bn = self.n_mut(branch);
        bn.depth = parent_depth + offset;
        bn.suffix_link = ROOT_ID;
        bn.primary = primary;
        self.detach_child(v);
        self.attach_child(parent, branch, edge_key);
        self.attach_child(branch, v, split_at);
        self.new_leaf(branch, c, start, !primary);
        branch
    }

    /// Returns the absolute start of the longest suffix still pending an
    /// explicit leaf, or `end()` when there is none.
    #[inline]
    fn pending_suffix_start(&self) -> usize {
        self.end() - self.implicit_suffix_count
    }

    /// Appends `c` to the right end of the window, increasing `end` and `len`
    /// by one. Takes amortized constant time for the byte alphabet; a single
    /// call may visit many nodes.
    ///
    /// # Panics
    ///
    /// Panics before modifying the tree if the window already contains
    /// [`MAX_WINDOW_LEN`] bytes, or if the stream position or buffer capacity
    /// would overflow. The window limit applies regardless of its contents
    /// and ensures that node IDs cannot overflow.
    pub fn push(&mut self, c: u8) {
        if self.len() >= MAX_WINDOW_LEN {
            panic!("snakree: maximum window length exceeded");
        }
        // Follows Ukkonen's algorithm (single-phase extension).
        // implicit_suffix_count counts the suffixes of the current window that
        // still need c added to their implicit path; the loop below processes
        // them from longest to shortest, using the active point to remember
        // where the previous suffix's matching path ended so each successive
        // suffix can be found in amortized constant time via suffix links
        // (set_active_to_next_suffix) rather than walking from the root.
        //
        // Each iteration falls into one of Ukkonen's three extension rules:
        //
        //   - Rule 2, no matching edge: the active node has no child starting
        //     with c, so a new leaf is created directly off it.
        //   - Rule 2, mid-edge mismatch: c continues the path but diverges
        //     partway along an existing edge, so that edge is split and a new
        //     leaf hangs off the new branching node. This is also where
        //     primary leaf pointers (plp/primary), which Snakree adds on top
        //     of Ukkonen's algorithm, must be kept consistent — see the paper
        //     cited on Snakree for why.
        //   - Rule 3, already present: c already continues the path (a
        //     "show stopper"), so the suffix needs no new node and extension
        //     for this phase stops early.
        //
        // Rule 1 (extending a leaf) needs no explicit code: leaf edges are
        // represented as extending to end(), so growing the window
        // already extends every existing leaf.
        let pos = self.end();
        self.text.push(c);
        self.implicit_suffix_count += 1;
        // last holds the most recently created branching node still awaiting
        // a suffix link, so it can be linked to the next branching node found
        // (or to the active reference pair's explicit base, if the phase ends
        // via rule 3).
        let mut last = NodeId::NIL;
        while self.implicit_suffix_count > 0 {
            if self.on_node(self.active) {
                if !last.is_nil() {
                    self.n_mut(last).suffix_link = self.active.node;
                    last = NodeId::NIL;
                }
                // The active point is exactly at the active node, so extend
                // from it.
                let next = self.raw_child(self.active.node, c);
                if !next.is_nil() {
                    // Rule 3: c already starts an outgoing edge.
                    self.active = Locus { node: next, offset: 1 };
                    break;
                }
                // Rule 2: no child starts with c, so extend directly from the
                // active node.
                // Cases 1-1 and 1-2: only the first child is primary.
                let primary = self.n(self.active.node).first_child.is_nil();
                self.new_leaf(self.active.node, c, self.pending_suffix_start(), primary);
                self.implicit_suffix_count -= 1;
                self.set_active_to_next_suffix(self.active.node, pos, 0);
            } else {
                let (b, _) = self.raw_edge(self.active.node);
                if self.at(b + self.active.offset) == c {
                    // Rule 3: c already continues the path here. Every
                    // shorter suffix extended by c is also present, so this
                    // phase ends without adding a node.
                    // last is always nil here: last was just split off with two
                    // distinct continuations xα·a and xα·c, so α is branching
                    // and the active point is on an explicit node, not mid-edge.
                    debug_assert!(last.is_nil());
                    self.active.offset += 1;
                    break;
                }
                // Rule 2, mid-edge: split the edge at the active point and
                // hang a new leaf off the new branching node, while preserving
                // the primary path through the active node (see paper cited on
                // Snakree).
                let branch = self.split_edge(self.active.node, self.active.offset, c, self.pending_suffix_start());
                if !last.is_nil() {
                    self.n_mut(last).suffix_link = branch;
                }
                last = branch;
                self.implicit_suffix_count -= 1;
                let parent = self.n(branch).parent;
                self.set_active_to_next_suffix(parent, b, self.active.offset);
            }
        }
    }

    /// Appends every byte of `s` to the right end of the window.
    /// Uses [`push`](Self::push) for each byte; if an append panics, bytes
    /// appended before it remain in the window.
    pub fn extend_from_slice(&mut self, s: &[u8]) {
        for &c in s {
            self.push(c);
        }
    }

    /// Sets the active point to the next shorter suffix of the point reached
    /// by consuming `length` bytes from `base`, using `text[begin..]` as an
    /// occurrence of that path label — the same `(base, begin, length)`
    /// convention `set_active` takes below. The edge may have just been
    /// split, so `begin` is an occurrence of its old label rather than a value
    /// re-derived from the current topology.
    fn set_active_to_next_suffix(&mut self, mut base: NodeId, mut begin: usize, mut length: usize) {
        if base == ROOT_ID {
            if length > 0 {
                begin += 1;
                length -= 1;
            }
        } else {
            base = self.n(base).suffix_link;
        }
        self.set_active(base, begin, length);
    }

    /// Stores the canonical destination-and-offset representation of the
    /// point reached by consuming `length` bytes from `base`, using
    /// `text[begin..]` as an occurrence of that path label.
    fn set_active(&mut self, mut base: NodeId, mut begin: usize, mut length: usize) {
        if length == 0 {
            self.active = Locus { node: base, offset: self.raw_edge_len(base) };
            return;
        }
        loop {
            let v = self.raw_child(base, self.at(begin));
            let edge_len = self.raw_edge_len(v);
            if length <= edge_len {
                self.active = Locus { node: v, offset: length };
                return;
            }
            base = v;
            begin += edge_len;
            length -= edge_len;
        }
    }

    /// Removes and returns the oldest byte in the window, or returns `None`
    /// and leaves the tree unchanged when the window is empty. The oldest
    /// suffix leaf is removed, or reused for the active suffix when that
    /// suffix would otherwise disappear. Only the deleted leaf's parent can
    /// become unary and need merging.
    pub fn pop_front(&mut self) -> Option<u8> {
        if self.is_empty() {
            return None;
        }
        let c = self.at(self.begin());
        let leaf = self.leaves.dequeue(&mut self.arena);
        if self.active.node == leaf {
            // The active point is never a leaf, so it lies strictly inside
            // this leaf's incoming edge. Thus the active suffix is both a
            // suffix and a prefix of the window: deleting the oldest suffix
            // leaf also removes its prefix occurrence. Reuse that leaf for the
            // active suffix by moving its start to the active suffix's start
            // and appending it as the newest explicit suffix. This performs
            // the logical deletion of the window's longest suffix and
            // insertion of the active suffix without changing the topology.
            //
            // At most this one implicit suffix needs a leaf. Every proper
            // suffix of the active suffix still has its occurrence inside the
            // prefix occurrence after the first byte is removed, as well as
            // its occurrence at the window's end, so those shorter suffixes
            // remain repeating and implicit.
            let parent = self.n(leaf).parent;
            let (b, _) = self.raw_edge(leaf);
            let offset = self.active.offset;
            self.n_mut(leaf).suffix_start = self.pending_suffix_start();
            self.leaves.enqueue(&mut self.arena, leaf);
            self.implicit_suffix_count -= 1;
            self.set_active_to_next_suffix(parent, b, offset);
        } else {
            self.delete_leaf(leaf);
        }
        self.text.discard_front();
        Some(c)
    }

    /// Merges `parent`, which has become unary, into its one remaining child.
    /// Splices it out by reattaching that child `pc` directly under `parent`'s
    /// own parent `pp`, on the edge formed by concatenating `parent`'s
    /// incoming edge with `pc`'s. Called by `delete_leaf` for a non-root
    /// parent after preparing the child's primary status and PLP for this
    /// reattachment.
    ///
    /// Only points represented on the two merged edges need adjustment. If
    /// `active.node` is `parent`, the point is on `parent`'s incoming edge
    /// (including its endpoint): change node to `pc` and keep offset. If
    /// `active.node` is `pc`, prepend `parent`'s former edge length to offset.
    /// Points on edges farther down the subtree retain their representation.
    ///
    /// No surviving internal node can have a suffix link to `parent`: if
    /// `parent` spells X, a branching node spelling aX would imply two
    /// distinct right extensions of X still in the window, contradicting
    /// `parent`'s being unary. Thus releasing `parent` requires no incoming
    /// suffix-link repairs.
    fn merge_unary_node(&mut self, parent: NodeId) {
        let (pp, pc) = (self.n(parent).parent, self.n(parent).first_child);
        let parent_edge_len = self.raw_edge_len(parent);
        if self.active.node == parent {
            self.active.node = pc;
        } else if self.active.node == pc {
            self.active.offset += parent_edge_len;
        }
        let ch = self.n(parent).key;
        self.detach_child(pc);
        self.detach_child(parent);
        self.attach_child(pp, pc, ch);
        self.arena.release(parent);
    }

    /// Deletes leaf `u` and adjusts primary paths locally, using `iplp[u]` to
    /// find the owner of the path ending at `u`.
    ///
    /// Requires `u` to have been removed from the leaf FIFO and not to contain
    /// the active point on its incoming edge. If the non-root parent becomes
    /// unary, merges it before returning, restoring the primary paths and
    /// adjusting the active point for the merged edge. Primary status and
    /// PLPs are prepared for the merged topology before the merge; that
    /// intermediate state is confined to this operation. Case numbers below
    /// refer to Sections 4.3-4.5 of the cited paper.
    fn delete_leaf(&mut self, u: NodeId) {
        let parent = self.n(u).parent;
        self.detach_child(u);
        if self.n(u).primary {
            if self.n(parent).first_child.is_nil() {
                // Case 3-1: empty root
                self.set_plp(parent, parent);
            } else {
                let y = self.n(parent).first_child; // all remaining children are secondary
                let pn = self.n(parent);
                if parent == ROOT_ID || pn.child_count > 1 || pn.primary {
                    // Cases 3-2, 4-1, 4-2: redirect the surviving owner's
                    // primary path through y, promoting y to primary.
                    self.set_plp(self.n(u).iplp, self.n(y).plp);
                    self.mark_primary(y);
                }
                // Otherwise, Case 4-3: the secondary parent will be merged
                // away. Leave y secondary with its existing PLP.
            }
        } else if parent != ROOT_ID && self.n(parent).child_count == 1 && !self.n(parent).primary {
            // Case 5-3: y inherits the removed secondary parent's path.
            let y = self.n(parent).first_child;
            self.mark_secondary(y, self.n(parent).plp);
        }
        // Cases 3-3, 5-1, 5-2 need no PLP or primary-status changes.
        self.arena.release(u);
        if parent != ROOT_ID && self.n(parent).child_count == 1 {
            self.merge_unary_node(parent);
        }
    }
}
