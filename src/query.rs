use std::ops::Range;

use crate::node::ROOT_ID;
use crate::point::{NodeId, Point};
use crate::snakree::Snakree;

/// Iterates over the out-edges of a node, yielding the first byte of each
/// edge label and the child it leads to. Created by [`Snakree::children`].
pub struct Children<'a> {
    tree: &'a Snakree,
    next: NodeId,
}

impl Iterator for Children<'_> {
    type Item = (u8, NodeId);

    fn next(&mut self) -> Option<(u8, NodeId)> {
        if self.next.is_nil() {
            return None;
        }
        let child = self.next;
        let n = self.tree.n(child);
        self.next = n.next_sib;
        Some((n.key, child))
    }
}

impl Snakree {
    /// Returns the absolute stream position of the first byte of the window.
    #[inline]
    pub fn begin(&self) -> usize {
        self.text.begin()
    }

    /// Returns the absolute stream position just past the last byte of the
    /// window. Equals the number of bytes ever pushed.
    #[inline]
    pub fn end(&self) -> usize {
        self.text.end()
    }

    /// Returns the half-open absolute byte range of the current window.
    #[inline]
    pub fn range(&self) -> Range<usize> {
        self.begin()..self.end()
    }

    /// Returns the window length, `end - begin`.
    #[inline]
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// Reports whether the window is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Returns `T[i]`. Panics unless `begin <= i < end`.
    #[inline]
    pub fn at(&self, i: usize) -> u8 {
        self.text.at(i)
    }

    /// Appends the current window `T[begin..end)` to `dst`.
    pub fn window(&self, dst: &mut Vec<u8>) {
        self.copy_range(self.range(), dst);
    }

    /// Borrows the bytes in an absolute stream range without copying, in constant time.
    /// Concatenating the returned slices yields `T[range]` in stream order.
    /// The second slice is empty unless the range wraps around the circular buffer.
    /// An empty range returns two empty slices, including at either window endpoint.
    /// The slices borrow the tree, preventing updates while they are in use.
    /// Byte ranges need not fall on UTF-8 character boundaries.
    ///
    /// # Panics
    ///
    /// Panics unless `begin() <= range.start <= range.end <= end()`.
    ///
    /// # Examples
    ///
    /// ```
    /// let mut tree = snakree::Snakree::new();
    /// tree.extend_from_slice(b"banana");
    /// let (first, second) = tree.slices(tree.range());
    /// assert_eq!([first, second].concat(), b"banana");
    /// ```
    pub fn slices(&self, range: Range<usize>) -> (&[u8], &[u8]) {
        self.text.slices(range)
    }

    /// Appends the bytes in an absolute stream range to `dst`, preserving its existing contents.
    /// Takes time proportional to the range length, excluding destination allocation.
    /// An empty range leaves `dst` unchanged.
    ///
    /// # Panics
    ///
    /// Panics unless `begin() <= range.start <= range.end <= end()`.
    ///
    /// # Examples
    ///
    /// ```
    /// let mut tree = snakree::Snakree::new();
    /// tree.extend_from_slice(b"before banana after");
    /// let pattern = b"banana";
    /// if let Some(pos) = tree.find(pattern) {
    ///     let start = pos.saturating_sub(5).max(tree.begin());
    ///     let end = (pos + pattern.len()).saturating_add(5).min(tree.end());
    ///     let mut context = Vec::new();
    ///     tree.copy_range(start..end, &mut context);
    ///     assert_eq!(context, b"fore banana afte");
    /// }
    /// ```
    pub fn copy_range(&self, range: Range<usize>, dst: &mut Vec<u8>) {
        self.append_text(dst, range.start, range.end);
    }

    /// Appends `T[b..e)` to `dst`, for a range inside the window.
    pub(crate) fn append_text(&self, dst: &mut Vec<u8>, b: usize, e: usize) {
        self.text.append_text(dst, b, e);
    }

    /// Returns the root node. The root always exists, including for an empty
    /// window.
    #[inline]
    pub fn root(&self) -> NodeId {
        ROOT_ID
    }

    /// Returns the number of explicit nodes, including the root.
    #[inline]
    pub fn num_nodes(&self) -> usize {
        self.arena.len()
    }

    /// Returns the parent of `v`, or `None` if `v` is the root.
    pub fn parent(&self, v: NodeId) -> Option<NodeId> {
        self.arena.validate(v);
        self.n(v).parent.into_option()
    }

    /// Returns the child of `v` along the out-edge whose label starts with
    /// `c`, or `None` if there is none.
    pub fn child(&self, v: NodeId, c: u8) -> Option<NodeId> {
        self.arena.validate(v);
        self.raw_child(v, c).into_option()
    }

    /// The internal, unchecked counterpart of `child`, avoiding the `NodeId`
    /// validation that `child` performs on every call.
    pub(crate) fn raw_child(&self, v: NodeId, c: u8) -> NodeId {
        let mut child = self.n(v).first_child;
        while !child.is_nil() {
            let n = self.n(child);
            if n.key == c {
                return child;
            }
            child = n.next_sib;
        }
        NodeId::NIL
    }

    /// Returns the number of out-edges of `v`; zero iff `v` is a leaf.
    pub fn num_children(&self, v: NodeId) -> usize {
        self.arena.validate(v);
        self.n(v).child_count
    }

    /// Reports whether `v` has no children. When the window is empty the
    /// root has no children either, so it is reported as a leaf even though
    /// it is not the leaf of any suffix; use `v != root() && is_leaf(v)` to
    /// test for suffix leaves.
    pub fn is_leaf(&self, v: NodeId) -> bool {
        self.arena.validate(v);
        self.raw_is_leaf(v)
    }

    /// Reports whether `v` has no children without performing `NodeId`
    /// validation. Consequently the root of an empty tree satisfies this
    /// structural predicate, although it is not a suffix leaf and is never
    /// part of the leaf FIFO.
    #[inline]
    pub(crate) fn raw_is_leaf(&self, v: NodeId) -> bool {
        self.n(v).first_child.is_nil()
    }

    /// Returns the node reached by the suffix link of `v`, i.e. the node
    /// spelling out `path(v)` with its first byte removed. Returns
    /// `None` for the root and leaves, which have no suffix link.
    pub fn suffix_link(&self, v: NodeId) -> Option<NodeId> {
        self.arena.validate(v);
        if v == ROOT_ID || self.raw_is_leaf(v) {
            return None;
        }
        self.n(v).suffix_link.into_option()
    }

    /// Iterates over the out-edges of `v`, yielding the first byte of each
    /// edge label and the child it leads to. The iteration order is
    /// unspecified.
    pub fn children(&self, v: NodeId) -> Children<'_> {
        self.arena.validate(v);
        Children { tree: self, next: self.n(v).first_child }
    }

    /// Returns the string depth of `v`, i.e. the length of the path label
    /// from the root to `v`. The root has depth 0.
    pub fn depth(&self, v: NodeId) -> usize {
        self.arena.validate(v);
        self.node_depth(v)
    }

    /// Returns a node's string depth, deriving it from the current window end
    /// for leaves because their incoming edges are open and grow with every
    /// `push`.
    #[inline]
    pub(crate) fn node_depth(&self, v: NodeId) -> usize {
        if v != ROOT_ID && self.raw_is_leaf(v) {
            return self.end() - self.n(v).suffix_start;
        }
        self.n(v).depth
    }

    /// Returns the half-open range `[b, e)` of the label of the edge entering
    /// `v`, as absolute positions inside the current window. For the root it
    /// returns an empty range, even when `is_leaf` reports true for an empty
    /// window. For a leaf `e` equals `end`, since leaves are open.
    ///
    /// The range is always fresh: `begin <= b <= e <= end`. For non-root `v`
    /// it is also strongly fresh: if `k = b - depth(parent(v))`, then
    /// `k >= begin` and `T[k..e)` is the entire root-to-`v` path label, not
    /// just the entering edge's label.
    pub fn edge(&self, v: NodeId) -> (usize, usize) {
        self.arena.validate(v);
        self.raw_edge(v)
    }

    /// Returns the length of the label of the edge entering `v`, or 0 for the
    /// root.
    pub fn edge_len(&self, v: NodeId) -> usize {
        self.arena.validate(v);
        self.raw_edge_len(v)
    }

    /// Returns the length of `v`'s incoming edge, or zero for the root,
    /// without validating `v`. Depths suffice; no leaf pointer or label
    /// occurrence is needed.
    #[inline]
    pub(crate) fn raw_edge_len(&self, v: NodeId) -> usize {
        if v == ROOT_ID {
            return 0;
        }
        self.node_depth(v) - self.n(self.n(v).parent).depth
    }

    /// Returns the half-open absolute range of the label of the edge entering
    /// `v`, or an empty range at the window's beginning when `v` is the root.
    /// Unlike `edge`, it accepts a trusted `NodeId` and does not validate it.
    ///
    /// For a non-root node, a descendant leaf supplies an occurrence of the
    /// path to `v` in the current window; its suffix start and the endpoint
    /// depths locate the incoming edge within that occurrence.
    pub(crate) fn raw_edge(&self, v: NodeId) -> (usize, usize) {
        if v == ROOT_ID {
            return (self.begin(), self.begin());
        }
        let k = self.n(self.leaf_pointer(v)).suffix_start;
        (k + self.n(self.n(v).parent).depth, k + self.node_depth(v))
    }

    /// Appends the label of the edge entering `v` to `dst`.
    pub fn label(&self, v: NodeId, dst: &mut Vec<u8>) {
        self.arena.validate(v);
        let (b, e) = self.raw_edge(v);
        self.append_text(dst, b, e);
    }

    /// Appends the path label of `v`, i.e. the string spelled out from the
    /// root to `v`, to `dst`.
    pub fn path(&self, v: NodeId, dst: &mut Vec<u8>) {
        self.arena.validate(v);
        let k = self.raw_occurrence_start(v);
        self.append_text(dst, k, k + self.node_depth(v));
    }

    /// Returns the absolute start position of an occurrence of the path label
    /// of `v` in the window, so that `T[pos..pos + depth(v))` equals
    /// `path(v)`. For a leaf it is the start of the suffix the leaf
    /// represents. For an internal node it is unspecified which occurrence is
    /// returned. The root yields `begin`.
    pub fn occurrence_start(&self, v: NodeId) -> usize {
        self.arena.validate(v);
        self.raw_occurrence_start(v)
    }

    /// The unchecked counterpart of `occurrence_start`. A leaf in `v`'s
    /// subtree begins with the path label of `v`; see Sliding Suffix Trees
    /// Revisited, Section 3, Lemma 1, for why that leaf's suffix lies inside
    /// the window.
    fn raw_occurrence_start(&self, v: NodeId) -> usize {
        if v == ROOT_ID {
            return self.begin();
        }
        self.n(self.leaf_pointer(v)).suffix_start
    }

    /// Returns an earlier occurrence of the longest repeated suffix of the
    /// window, i.e. the longest suffix that occurs at least twice in it, as
    /// `(pos, length)`. If `length > 0`, `begin <= pos < end - length` and
    /// `T[pos..pos + length)` equals `T[end - length..end)`. Ties between
    /// earlier occurrences are unspecified. Returns `(0, 0)` if no nonempty
    /// suffix repeats.
    ///
    /// Answers from the active point in constant time without changing the
    /// tree. The active point is never exactly at a leaf, so its node's
    /// subtree contains a leaf strictly deeper than the active suffix. That
    /// leaf's suffix starts with the active suffix and ends at `end`, so its
    /// start precedes `end - length`.
    #[inline]
    pub fn repeated_suffix(&self) -> (usize, usize) {
        let length = self.implicit_suffix_count;
        if length == 0 {
            return (0, 0);
        }
        (self.n(self.leaf_pointer(self.active.node)).suffix_start, length)
    }

    /// Returns the `Point` at the root.
    pub fn root_point(&self) -> Point<'_> {
        Point { tree: self, node: ROOT_ID, offset: 0 }
    }

    /// Returns the `Point` sitting exactly on the explicit node `v`.
    pub fn point_at(&self, v: NodeId) -> Point<'_> {
        Point { tree: self, node: v, offset: self.edge_len(v) }
    }

    /// Returns the locus of `pattern` in the window, i.e. the `Point`
    /// spelling out `pattern`, and reports whether `pattern` occurs in the
    /// window. The empty pattern locates the root.
    ///
    /// If `pattern` does not occur, the returned `Point` is the locus of the
    /// longest prefix of `pattern` that does occur, so its depth is the length
    /// of that prefix.
    pub fn descend(&self, pattern: &[u8]) -> (Point<'_>, bool) {
        let mut node = ROOT_ID;
        let mut offset = 0;
        let mut i = 0;
        // Invariant: at the top of each iteration, `node` is explicit and
        // `offset` is its full edge length, so the next step is a descent.
        while i < pattern.len() {
            let Some(child) = self.raw_child(node, pattern[i]).into_option() else {
                break;
            };
            // The child lookup consumed the first byte of the edge; the rest
            // of the edge is compared with the pattern in bulk.
            let (b, e) = self.raw_edge(child);
            let len = e - b;
            let m = self.text.common_prefix(b + 1..e, &pattern[i + 1..]);
            node = child;
            offset = 1 + m;
            i += 1 + m;
            // Either the whole edge matched and we move on to the next node,
            // or we stop inside it (mismatch or pattern exhausted).
            if offset < len {
                break;
            }
        }
        (Point { tree: self, node, offset }, i == pattern.len())
    }

    /// Returns the absolute byte position of an occurrence of `pattern`, or `None` if absent.
    /// Which occurrence is returned is unspecified; it need not be the first or last.
    /// An empty pattern returns `Some(begin())`, even when the window is empty.
    /// Takes O(pattern.len()) time over the byte alphabet.
    pub fn find(&self, pattern: &[u8]) -> Option<usize> {
        let (point, found) = self.descend(pattern);
        found.then(|| point.occurrence_start())
    }

    /// Reports whether `pattern` occurs in the window.
    pub fn contains(&self, pattern: &[u8]) -> bool {
        self.descend(pattern).1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Descends `pattern` one byte at a time, as the straightforward reference for `descend`.
    fn descend_by_byte<'a>(tree: &'a Snakree, pattern: &[u8]) -> (Point<'a>, bool) {
        let mut p = tree.root_point();
        for &c in pattern {
            match p.descend(c) {
                Some(q) => p = q,
                None => return (p, false),
            }
        }
        (p, true)
    }

    fn assert_locate_matches_descend(tree: &Snakree, pattern: &[u8]) {
        let (p, found) = tree.descend(pattern);
        let (q, expected) = descend_by_byte(tree, pattern);
        assert_eq!((p.node(), p.offset(), found), (q.node(), q.offset(), expected), "{pattern:?}");
    }

    #[test]
    fn locate_matches_descend_across_ring_buffer_wraparound() {
        let mut tree = Snakree::new();
        let text: Vec<u8> = (0..4000u32).map(|i| b"abcab"[(i % 5) as usize] ^ ((i / 997) as u8)).collect();
        for (k, &c) in text.iter().enumerate() {
            tree.push(c);
            if k >= 100 {
                tree.pop_front();
            }
        }
        let window = &text[text.len() - 100..];
        for start in 0..window.len() {
            for end in start..=window.len().min(start + 40) {
                let pattern = &window[start..end];
                assert!(tree.contains(pattern), "{start}..{end}");
                assert_locate_matches_descend(&tree, pattern);
            }
        }
        // A mismatch after 15 matching bytes locates the 15-byte prefix.
        let mut miss = window[10..30].to_vec();
        miss[15] ^= 0x80;
        assert_locate_matches_descend(&tree, &miss);
        let (p, found) = tree.descend(&miss);
        assert!(!found);
        assert_eq!(p.depth(), 15);
    }
}
