//! A test-only O(d²) reference implementation for differential testing.

use std::collections::{BTreeMap, HashMap};

use crate::point::NodeId;
use crate::testutil::{index_of, path_of, TestTree};
use crate::text::MAX_STREAM_POSITION;

/// Rebuilds a compacted suffix trie after each update in O(d²) time, where d
/// is the window length. It is an independent reference for differential
/// testing.
pub(crate) struct NaiveTree {
    text: Vec<u8>,
    pub(crate) window_begin: usize,
    pub(crate) window_end: usize,
    nodes: Vec<NaiveNode>,
    // The earliest earlier occurrence and the length of the longest repeated
    // suffix, found by direct search in rebuild.
    repeated_pos: usize,
    repeated_len: usize,
}

#[derive(Default)]
struct NaiveNode {
    parent: Option<NodeId>,
    suffix: Option<NodeId>,
    children: BTreeMap<u8, NodeId>,
    b: usize,
    e: usize,
    depth: usize,
}

impl NaiveTree {
    /// Creates an empty reference tree.
    pub(crate) fn new() -> Self {
        let mut t = NaiveTree {
            text: Vec::new(),
            window_begin: 0,
            window_end: 0,
            nodes: Vec::new(),
            repeated_pos: 0,
            repeated_len: 0,
        };
        t.rebuild();
        t
    }

    /// Creates a new leaf child of `parent` with edge label `[b, e)` and
    /// returns its id.
    fn add_leaf(&mut self, parent: NodeId, b: usize, e: usize) -> NodeId {
        let v = NodeId(u32::try_from(self.nodes.len()).expect("snakree: node ID overflow"));
        let depth = self.nodes[parent.0 as usize].depth + e - b;
        self.nodes.push(NaiveNode { parent: Some(parent), b, e, depth, ..Default::default() });
        let key = TestTree::at(self, b);
        self.nodes[parent.0 as usize].children.insert(key, v);
        v
    }

    /// Discards the current tree and reconstructs it from scratch by
    /// inserting every suffix of the window, then derives suffix links from
    /// the resulting topology and the longest repeated suffix by direct
    /// search.
    pub(crate) fn rebuild(&mut self) {
        self.nodes =
            vec![NaiveNode::default(), NaiveNode { b: self.window_begin, e: self.window_begin, ..Default::default() }];
        let end = self.window_end;
        // Insert longer suffixes first. A shorter suffix ending inside an
        // edge needs no explicit endpoint in an implicit suffix tree.
        for start in self.window_begin..end {
            let (mut parent, mut pos) = (NodeId(1), start);
            while pos < end {
                let key = TestTree::at(self, pos);
                let Some(&child) = self.nodes[parent.0 as usize].children.get(&key) else {
                    self.add_leaf(parent, pos, end);
                    break;
                };
                let (b, e) = (self.nodes[child.0 as usize].b, self.nodes[child.0 as usize].e);
                let mut m = 0;
                while b + m < e && pos + m < end && TestTree::at(self, b + m) == TestTree::at(self, pos + m) {
                    m += 1;
                }
                pos += m;
                if b + m == e {
                    parent = child;
                    continue;
                }
                if pos == end {
                    break;
                }
                let branch = self.add_leaf(parent, b, b + m);
                self.nodes[child.0 as usize].b += m;
                self.nodes[child.0 as usize].parent = Some(branch);
                let key = TestTree::at(self, b + m);
                self.nodes[branch.0 as usize].children.insert(key, child);
                self.add_leaf(branch, pos, end);
                break;
            }
        }
        // Resolve suffix links from path strings, independently of Ukkonen.
        let mut paths = HashMap::new();
        for i in 1..self.nodes.len() {
            let v = NodeId(i as u32);
            if !self.nodes[i].children.is_empty() || i == 1 {
                paths.insert(path_of(self, v), v);
            }
        }
        for (s, &v) in &paths {
            if !s.is_empty() {
                self.nodes[v.0 as usize].suffix = paths.get(&s[1..]).copied();
            }
        }
        // Find the longest suffix of text that appears in text[..len - 1]
        // (text with the last byte excluded).
        self.repeated_pos = 0;
        self.repeated_len = 0;
        let n = self.text.len();
        for k in (1..n).rev() {
            if let Some(i) = index_of(&self.text[..n - 1], &self.text[n - k..]) {
                self.repeated_pos = self.window_begin + i;
                self.repeated_len = k;
                break;
            }
        }
    }

    /// Returns the node data for `v`, panicking if `v` is not a valid node id.
    fn node(&self, v: NodeId) -> &NaiveNode {
        if v.is_nil() || v.0 as usize >= self.nodes.len() {
            panic!("snakree: invalid node");
        }
        &self.nodes[v.0 as usize]
    }
}

impl TestTree for NaiveTree {
    fn name(&self) -> &'static str {
        "NaiveTree"
    }
    /// Appends `c` to the window and rebuilds the whole tree from scratch.
    fn push(&mut self, c: u8) {
        if self.window_end == MAX_STREAM_POSITION {
            panic!("snakree: stream position overflow");
        }
        self.text.push(c);
        self.window_end += 1;
        self.rebuild();
    }
    /// Drops the leftmost byte of the window and rebuilds the whole tree from
    /// scratch.
    fn pop_front(&mut self) -> Option<u8> {
        if self.text.is_empty() {
            return None;
        }
        let c = self.text.remove(0);
        self.window_begin += 1;
        self.rebuild();
        Some(c)
    }
    fn begin(&self) -> usize {
        self.window_begin
    }
    fn end(&self) -> usize {
        self.window_end
    }
    fn len(&self) -> usize {
        self.text.len()
    }
    fn at(&self, i: usize) -> u8 {
        if i < self.window_begin || i >= self.window_end {
            panic!("snakree: position outside window");
        }
        self.text[i - self.window_begin]
    }
    fn root(&self) -> NodeId {
        NodeId(1)
    }
    fn num_nodes(&self) -> usize {
        self.nodes.len() - 1
    }
    fn parent(&self, v: NodeId) -> Option<NodeId> {
        self.node(v).parent
    }
    fn child(&self, v: NodeId, c: u8) -> Option<NodeId> {
        self.node(v).children.get(&c).copied()
    }
    fn num_children(&self, v: NodeId) -> usize {
        self.node(v).children.len()
    }
    fn suffix_link(&self, v: NodeId) -> Option<NodeId> {
        self.node(v).suffix
    }
    fn is_leaf(&self, v: NodeId) -> bool {
        self.node(v).children.is_empty()
    }
    fn edge(&self, v: NodeId) -> (usize, usize) {
        let n = self.node(v);
        (n.b, n.e)
    }
    fn depth(&self, v: NodeId) -> usize {
        self.node(v).depth
    }
    fn repeated_suffix(&self) -> (usize, usize) {
        (self.repeated_pos, self.repeated_len)
    }
}
