//! Helpers shared by the unit tests.

use crate::point::NodeId;
use crate::snakree::Snakree;

/// The operations `Snakree` and `NaiveTree` share, so differential tests can
/// drive and inspect both through the same code.
pub(crate) trait TestTree {
    fn name(&self) -> &'static str;
    fn push(&mut self, c: u8);
    fn pop_front(&mut self) -> Option<u8>;
    fn begin(&self) -> usize;
    fn end(&self) -> usize;
    fn len(&self) -> usize;
    fn at(&self, i: usize) -> u8;
    fn root(&self) -> NodeId;
    fn num_nodes(&self) -> usize;
    fn parent(&self, v: NodeId) -> Option<NodeId>;
    fn child(&self, v: NodeId, c: u8) -> Option<NodeId>;
    fn num_children(&self, v: NodeId) -> usize;
    fn suffix_link(&self, v: NodeId) -> Option<NodeId>;
    fn is_leaf(&self, v: NodeId) -> bool;
    fn edge(&self, v: NodeId) -> (usize, usize);
    fn depth(&self, v: NodeId) -> usize;
    fn repeated_suffix(&self) -> (usize, usize);
}

impl TestTree for Snakree {
    fn name(&self) -> &'static str {
        "Snakree"
    }
    fn push(&mut self, c: u8) {
        Snakree::push(self, c)
    }
    fn pop_front(&mut self) -> Option<u8> {
        Snakree::pop_front(self)
    }
    fn begin(&self) -> usize {
        Snakree::begin(self)
    }
    fn end(&self) -> usize {
        Snakree::end(self)
    }
    fn len(&self) -> usize {
        Snakree::len(self)
    }
    fn at(&self, i: usize) -> u8 {
        Snakree::at(self, i)
    }
    fn root(&self) -> NodeId {
        Snakree::root(self)
    }
    fn num_nodes(&self) -> usize {
        Snakree::num_nodes(self)
    }
    fn parent(&self, v: NodeId) -> Option<NodeId> {
        Snakree::parent(self, v)
    }
    fn child(&self, v: NodeId, c: u8) -> Option<NodeId> {
        Snakree::child(self, v, c)
    }
    fn num_children(&self, v: NodeId) -> usize {
        Snakree::num_children(self, v)
    }
    fn suffix_link(&self, v: NodeId) -> Option<NodeId> {
        Snakree::suffix_link(self, v)
    }
    fn is_leaf(&self, v: NodeId) -> bool {
        Snakree::is_leaf(self, v)
    }
    fn edge(&self, v: NodeId) -> (usize, usize) {
        Snakree::edge(self, v)
    }
    fn depth(&self, v: NodeId) -> usize {
        Snakree::depth(self, v)
    }
    fn repeated_suffix(&self) -> (usize, usize) {
        Snakree::repeated_suffix(self)
    }
}

/// Appends the label of the edge entering `v` to `dst`, reading it through
/// `edge` and `at` only. It serves as an oracle for `Snakree::label`.
pub(crate) fn label_of(t: &dyn TestTree, v: NodeId, dst: &mut Vec<u8>) {
    let (b, e) = t.edge(v);
    dst.extend((b..e).map(|i| t.at(i)));
}

/// Returns the path label of `v` by concatenating edge labels from the root.
/// It serves as an oracle for `Snakree::path`.
pub(crate) fn path_of(t: &dyn TestTree, v: NodeId) -> Vec<u8> {
    if v == t.root() {
        return Vec::new();
    }
    let mut dst = path_of(t, t.parent(v).expect("non-root node has a parent"));
    label_of(t, v, &mut dst);
    dst
}

/// Returns the window, reading it through `at` only.
pub(crate) fn window_of(t: &dyn TestTree) -> Vec<u8> {
    (t.begin()..t.end()).map(|i| t.at(i)).collect()
}

/// Returns the first position of `needle` in `haystack`.
pub(crate) fn index_of(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Reports whether `needle` occurs in `haystack`.
pub(crate) fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    index_of(haystack, needle).is_some()
}

/// A small deterministic generator (SplitMix64) for reproducible random
/// tests.
pub(crate) struct Rng(pub(crate) u64);

impl Rng {
    pub(crate) fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    pub(crate) fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}
