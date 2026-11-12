//! Differential test harness and invariant checks.

use std::collections::{HashMap, HashSet};

use crate::naive::NaiveTree;
use crate::node::ROOT_ID;
use crate::point::{Locus, NodeId, Point};
use crate::snakree::Snakree;
use crate::testutil::{contains, label_of, path_of, window_of, TestTree};

/// Binds `l` to `t`, so tests can inspect a locus the tree keeps internally,
/// such as the active point, through the `Point` methods.
fn point_of(t: &Snakree, l: Locus) -> Point<'_> {
    Point { tree: t, node: l.node, offset: l.offset }
}

/// Formats bytes for failure messages.
pub(super) fn q(b: &[u8]) -> String {
    format!("\"{}\"", b.escape_ascii())
}

/// Owns both implementations and an expected window updated independently of either tree.
pub(super) struct TestHarness {
    pub(super) st: Snakree,
    pub(super) rf: NaiveTree,
    pub(super) window: Vec<u8>,
    pub(super) begin: usize,
    pub(super) end: usize,
}

impl TestHarness {
    pub(super) fn new() -> Self {
        Self { st: Snakree::new(), rf: NaiveTree::new(), window: Vec::new(), begin: 0, end: 0 }
    }

    // Updates deliberately do not query the trees; batch tests choose when to check them.
    pub(super) fn push(&mut self, c: u8) {
        self.st.push(c);
        TestTree::push(&mut self.rf, c);
        self.window.push(c);
        self.end += 1;
    }

    pub(super) fn extend_from_slice(&mut self, bytes: &[u8]) {
        self.st.extend_from_slice(bytes);
        for &c in bytes {
            TestTree::push(&mut self.rf, c);
        }
        self.window.extend_from_slice(bytes);
        self.end += bytes.len();
    }

    #[track_caller]
    pub(super) fn pop_front(&mut self) {
        let expected = self.window.first().copied();
        assert_eq!(self.st.pop_front(), expected, "Snakree eviction at {}", self.begin);
        assert_eq!(TestTree::pop_front(&mut self.rf), expected, "NaiveTree eviction at {}", self.begin);
        if expected.is_some() {
            self.window.remove(0);
            self.begin += 1;
        }
    }

    pub(super) fn check(&self) {
        CheckContext { st: &self.st, rf: &self.rf, window: &self.window, begin: self.begin, end: self.end }.check();
    }
}

/// Borrows both trees and the independent expectations for one verification pass.
pub(super) struct CheckContext<'a> {
    pub(super) st: &'a Snakree,
    pub(super) rf: &'a NaiveTree,
    pub(super) window: &'a [u8],
    pub(super) begin: usize,
    pub(super) end: usize,
}

impl CheckContext<'_> {
    pub(super) fn check(&self) {
        self.check_window();
        let seen = self.check_topology();
        self.check_repeated_suffix();
        self.check_queries();
        check_internals(self.st, &seen);
    }

    fn check_window(&self) {
        let Self { st, rf, window, begin, end } = *self;
        let mut got = Vec::new();
        st.window(&mut got);
        assert!(got == window, "window = {}; want {}", q(&got), q(window));
        for tree in [st as &dyn TestTree, rf as &dyn TestTree] {
            let w = window_of(tree);
            assert!(
                tree.begin() == begin && tree.end() == end && tree.len() == window.len() && w == window,
                "{} window = [{},{}) {}; want [{},{}) {}",
                tree.name(),
                tree.begin(),
                tree.end(),
                q(&w),
                begin,
                end,
                q(window)
            );
            assert!(
                tree.parent(tree.root()).is_none() && tree.suffix_link(tree.root()).is_none(),
                "{} invalid root",
                tree.name()
            );
        }
    }

    fn check_topology(&self) -> HashSet<NodeId> {
        let Self { st, rf, window, .. } = *self;
        let mut seen = HashSet::new();
        self.visit(st.root(), TestTree::root(rf), &mut seen);
        let bound = std::cmp::max(1, 2 * window.len());
        assert!(
            seen.len() == st.num_nodes() && st.num_nodes() == TestTree::num_nodes(rf) && st.num_nodes() <= bound,
            "node counts: reachable = {}, num_nodes = {}, reference = {}; want equal counts <= {}",
            seen.len(),
            st.num_nodes(),
            TestTree::num_nodes(rf),
            bound
        );
        seen
    }

    fn visit(&self, v: NodeId, ref_node: NodeId, seen: &mut HashSet<NodeId>) {
        let Self { st, rf, window, .. } = *self;
        assert!(seen.insert(v), "cycle/duplicate node {v:?} in {}", q(window));
        self.check_node(v, ref_node);
        let mut n = 0;
        for (c, child) in st.children(v) {
            n += 1;
            assert!(n <= 256, "node {v:?}: visited {n} children, want at most 256");
            assert_eq!(st.parent(child), Some(v), "child {child:?}: parent");
            assert_eq!(st.child(v, c), Some(child), "node {v:?}: child({c})");
            let Some(other) = TestTree::child(rf, ref_node, c) else {
                panic!("unexpected child {c} in {}", q(window));
            };
            self.visit(child, other, seen);
        }
        assert_eq!(n, st.num_children(v), "node {v:?}: iterated children vs num_children");
        assert_eq!(st.children(v).next().is_some(), n > 0, "node {v:?}: first child");
    }

    fn check_node(&self, v: NodeId, ref_node: NodeId) {
        let Self { st, rf, window, begin, end } = *self;
        let (b, e) = st.edge(v);
        assert!(
            !(b < begin || e < b || e > end || (v != st.root() && b == e) || st.edge_len(v) != e - b),
            "edge {v:?} = [{b},{e}) outside [{begin},{end}) for {}",
            q(window)
        );
        // Lemma 1 of Sliding Suffix Trees Revisited: the occurrence of the entire
        // path preceding an edge must also remain in the window.
        if v != st.root() {
            let start = b.checked_sub(st.depth(st.parent(v).unwrap()));
            assert!(
                matches!(start, Some(s) if s >= begin && window[s - begin..e - begin] == path_of(st, v)[..]),
                "edge {v:?} is not strongly fresh in {}",
                q(window)
            );
            let (rb, re) = TestTree::edge(rf, ref_node);
            let rstart = rb.checked_sub(TestTree::depth(rf, TestTree::parent(rf, ref_node).unwrap()));
            assert!(
                matches!(rstart, Some(s) if s >= begin && re <= end && s <= re && window[s - begin..re - begin] == path_of(rf, ref_node)[..]),
                "reference edge {ref_node:?} is not strongly fresh in {}",
                q(window)
            );
        }
        let (mut label, mut path) = (Vec::new(), Vec::new());
        st.label(v, &mut label);
        st.path(v, &mut path);
        let mut ref_label = Vec::new();
        label_of(rf, ref_node, &mut ref_label);
        let ref_path = path_of(rf, ref_node);
        assert!(
            label == ref_label && path == ref_path && st.depth(v) == TestTree::depth(rf, ref_node),
            "node {v:?}: label {} path {} depth {}; reference node {ref_node:?}: label {} path {} depth {}; window {}",
            q(&label),
            q(&path),
            st.depth(v),
            q(&ref_label),
            q(&ref_path),
            TestTree::depth(rf, ref_node),
            q(window)
        );
        assert!(
            st.is_leaf(v) == TestTree::is_leaf(rf, ref_node)
                && st.num_children(v) == TestTree::num_children(rf, ref_node),
            "node {v:?}: leaf={} children={}; reference node {ref_node:?}: leaf={} children={}; window {}",
            st.is_leaf(v),
            st.num_children(v),
            TestTree::is_leaf(rf, ref_node),
            TestTree::num_children(rf, ref_node),
            q(window)
        );
        assert!(!(st.is_leaf(v) && v != st.root() && e != end), "closed leaf {v:?}");
        assert!(!st.is_leaf(v) || st.suffix_link(v).is_none(), "leaf {v:?} has a suffix link");
        if !st.is_leaf(v) && v != st.root() {
            let link = st.suffix_link(v);
            assert!(
                st.num_children(v) >= 2 && link.is_some_and(|l| path_of(st, l) == path[1..]),
                "invalid branch/suffix link at {} in {}",
                q(&path),
                q(window)
            );
        }
    }

    fn check_repeated_suffix(&self) {
        let Self { st, rf, window, begin, end } = *self;
        let (_, want) = TestTree::repeated_suffix(rf);
        let p = point_of(st, st.active);
        let mut path = Vec::new();
        p.path(&mut path);
        let tail = &window[window.len() - want..];
        assert!(
            path == tail && p.depth() == st.implicit_suffix_count,
            "active = {} (implicit suffix count {}), want {}, window {}",
            q(&path),
            st.implicit_suffix_count,
            q(tail),
            q(window)
        );
        assert!(!(p.node() != st.root() && p.on_node() && st.is_leaf(p.node())), "active point is a leaf");
        assert!(
            !((p.node() == st.root() && p.offset() != 0)
                || (p.node() != st.root() && (p.offset() == 0 || p.offset() > st.edge_len(p.node())))),
            "invalid active point"
        );
        for tree in [st as &dyn TestTree, rf as &dyn TestTree] {
            let (pos, length) = tree.repeated_suffix();
            assert_eq!(length, want, "{} repeated suffix length", tree.name());
            if length == 0 {
                assert_eq!(pos, 0, "{} empty repeated suffix has a position", tree.name());
            } else {
                assert!(
                    pos >= begin
                        && pos + length < end
                        && window[pos - begin..pos - begin + length] == window[window.len() - length..],
                    "{} invalid repeated suffix ({pos}, {length}) in [{begin},{end}) {}",
                    tree.name(),
                    q(window)
                );
            }
        }
    }

    fn check_queries(&self) {
        let Self { st, window, begin, end, .. } = *self;
        for i in 0..=window.len() {
            let pattern = &window[i..];
            let (point, ok) = st.descend(pattern);
            let mut path = Vec::new();
            point.path(&mut path);
            assert!(ok && path == pattern, "missing suffix {} in {}", q(pattern), q(window));
            let pos = point.occurrence_start();
            let d = point.depth();
            assert!(
                pos >= begin && pos + d <= end && window[pos - begin..pos - begin + d] == *pattern,
                "invalid point occurrence at {pos} for {} in {}",
                q(pattern),
                q(window)
            );
            if !pattern.is_empty() {
                // Count overlapping occurrences too (e.g. "aa" in "aaa"). Exactly
                // the non-repeating suffixes must end at leaves.
                let repeating = contains(&window[..window.len() - 1], pattern);
                let is_leaf = point.on_node() && st.is_leaf(point.node());
                assert!(
                    is_leaf != repeating,
                    "suffix {}: leaf={is_leaf}, repeating={repeating} in {}",
                    q(pattern),
                    q(window)
                );
            }
        }
        let mut extended = window.to_vec();
        extended.push(0);
        for pattern in [&b""[..], &[0], &[255], b"ab", b"aaa", &extended] {
            assert_eq!(st.contains(pattern), contains(window, pattern), "query {} in {}", q(pattern), q(window));
        }
    }
}

fn check_internals(tree: &Snakree, seen: &HashSet<NodeId>) {
    check_sibling_links(tree, seen);
    check_primary_paths(tree, seen);
    let leaves = seen.iter().copied().filter(|&v| v != ROOT_ID && tree.n(v).first_child.is_nil()).collect();
    check_leaf_queue(tree, &leaves);
    check_storage(tree, seen);
}

fn check_sibling_links(tree: &Snakree, seen: &HashSet<NodeId>) {
    for &v in seen {
        let mut prev = NodeId::NIL;
        let mut child = tree.n(v).first_child;
        while !child.is_nil() {
            assert_eq!(tree.n(child).prev_sib, prev, "child {child:?} of node {v:?}: previous sibling");
            prev = child;
            child = tree.n(child).next_sib;
        }
    }
}

fn check_primary_paths(tree: &Snakree, seen: &HashSet<NodeId>) {
    let mut owners: HashMap<NodeId, NodeId> = HashMap::new();
    for &v in seen {
        let mut primary = 0;
        let mut child = tree.n(v).first_child;
        while !child.is_nil() {
            if tree.n(child).primary {
                primary += 1;
            }
            child = tree.n(child).next_sib;
        }
        assert!(tree.n(v).first_child.is_nil() || primary == 1, "node {v:?} has {primary} primary children");
        if !tree.n(v).primary {
            let mut endpoint = v;
            while !tree.n(endpoint).first_child.is_nil() {
                endpoint = tree.n(endpoint).first_child;
                while !tree.n(endpoint).primary {
                    endpoint = tree.n(endpoint).next_sib;
                }
            }
            assert!(
                tree.n(v).plp == endpoint && tree.n(endpoint).iplp == v,
                "invalid PLP/iplp at {v:?}: got {:?} want {endpoint:?}",
                tree.n(v).plp
            );
            assert!(owners.insert(endpoint, v).is_none(), "duplicate PLP owner");
        }
    }
}

fn check_leaf_queue(tree: &Snakree, leaves: &HashSet<NodeId>) {
    let mut remaining = leaves.clone();
    // The queue must hold consecutive suffix starts from begin onward.
    let (mut next_start, mut n) = (tree.begin(), 0);
    let mut tail = NodeId::NIL;
    let mut v = tree.leaves.head;
    while !v.is_nil() {
        n += 1;
        assert!(remaining.remove(&v) && tree.n(v).suffix_start == next_start, "invalid suffix leaf queue");
        next_start = tree.n(v).suffix_start + 1;
        tail = v;
        v = tree.n(v).leaf_next;
    }
    assert!(
        remaining.is_empty() && n == tree.len() - tree.implicit_suffix_count && tail == tree.leaves.tail,
        "missing suffix leaf"
    );
}

fn check_storage(tree: &Snakree, seen: &HashSet<NodeId>) {
    for &v in seen {
        assert!(tree.arena.is_live(v), "reachable node {v:?}: live=false, want true");
    }
    let mut free = HashSet::new();
    for &v in tree.arena.free_nodes() {
        assert!(v.0 >= 2 && !tree.arena.is_live(v) && free.insert(v), "invalid free list");
    }
    assert_eq!(
        seen.len() + free.len() + 1,
        tree.arena.slot_count(),
        "node slots: want allocated = reachable + free + 1 reserved slot"
    );
}
