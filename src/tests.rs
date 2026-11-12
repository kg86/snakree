//! Differential test cases against `NaiveTree`.

mod harness;

use harness::{q, CheckContext, TestHarness};

use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::naive::NaiveTree;
use crate::node::ROOT_ID;
use crate::point::{Locus, NodeId};
use crate::snakree::Snakree;
use crate::testutil::{contains, path_of, window_of, Rng, TestTree};
use crate::text::{WindowText, MAX_STREAM_POSITION};

/// Returns the paper's Section 4 deletion case that the next `pop_front` will
/// exercise. An empty result means `pop_front` will reuse the oldest leaf for
/// the active suffix instead of calling `delete_leaf`.
fn paper_deletion_case(tree: &Snakree) -> &'static str {
    let u = tree.leaves.head;
    if u.is_nil() || tree.active.node == u {
        return "";
    }
    let w = tree.n(u).parent;
    if w == ROOT_ID {
        if !tree.n(u).primary {
            return "3-3";
        }
        if tree.n(w).child_count == 1 {
            return "3-1";
        }
        return "3-2";
    }
    if tree.n(u).primary {
        if tree.n(w).child_count > 2 {
            return "4-1";
        }
        if tree.n(w).primary {
            return "4-2";
        }
        return "4-3";
    }
    if tree.n(w).child_count > 2 {
        return "5-1";
    }
    if tree.n(w).primary {
        return "5-2";
    }
    "5-3"
}

#[test]
fn leaf_deletion_cases() {
    let tests = [
        ("3-1", "a", 0),
        ("3-2", "ab", 0),
        ("3-3", "aaaaba", 4),
        ("4-1", "aaaaabaaaac", 0),
        ("4-2", "aab", 0),
        ("4-3", "aababba", 4),
        ("5-1", "aabaac", 1),
        ("5-2", "aabaa", 1),
        ("5-3", "aaaabba", 4),
    ];
    for (paper_case, input, contract_first) in tests {
        let mut h = TestHarness::new();
        h.extend_from_slice(input.as_bytes());
        for _ in 0..contract_first {
            h.pop_front();
            h.check();
        }
        assert_eq!(paper_deletion_case(&h.st), paper_case, "window {}", q(&h.window));
        h.pop_front();
        h.check();
    }
}

/// Bounds the cost of rebuilding and checking the reference tree after each
/// operation.
const MAX_OPERATION_WINDOW: usize = 80;

/// An update independent of the byte values stored in the window.
#[derive(Clone, Copy, Debug)]
enum Operation {
    Push(u8),
    Contract,
}

/// Applies updates and checks both trees after every step. At
/// `MAX_OPERATION_WINDOW` bytes, replaces the requested update with a
/// contraction to bound the cost of the reference implementation.
fn run_operations(ops: &[Operation]) {
    let mut h = TestHarness::new();
    h.check();
    for &op in ops {
        let op = if h.window.len() >= MAX_OPERATION_WINDOW { Operation::Contract } else { op };
        match op {
            Operation::Contract => h.pop_front(),
            Operation::Push(c) => h.push(c),
        }
        h.check();
    }
}

#[test]
fn trees_examples() {
    for s in [
        &b""[..],
        b"a",
        b"banana",
        b"mississippi",
        b"aaaaaaaaaaaaaaaaab",
        b"ababababababac",
        b"axazaz",
        b"abczabcyyabcyyz",
        b"abcabxabcd",
        b"\x00\xff\x00\xff\x01",
        "日本語日本語".as_bytes(),
    ] {
        let mut h = TestHarness::new();
        h.check();
        for &c in s {
            h.push(c);
            h.check();
        }
        for _ in s {
            h.pop_front();
            h.check();
        }
    }
}

/// `descend` on a missing pattern returns the locus of the longest prefix of
/// the pattern that occurs in the window.
#[test]
fn locate_missing_pattern() {
    let windows = ["", "a", "abab", "banana", "mississippi", "aaaaab", "abcabxabcd"];
    let patterns = ["", "x", "abx", "bab", "abababab", "nan", "nab", "issip", "ississ", "aaaaaa", "abcabc", "zzz"];
    for w in windows {
        let mut st = Snakree::new();
        st.extend_from_slice(w.as_bytes());
        for pat in patterns {
            let pat = pat.as_bytes();
            let mut want = 0;
            while want < pat.len() && contains(w.as_bytes(), &pat[..=want]) {
                want += 1;
            }
            let (p, ok) = st.descend(pat);
            assert_eq!(ok, want == pat.len(), "descend({}) in {w:?}", q(pat));
            let mut path = Vec::new();
            p.path(&mut path);
            assert!(p.depth() == want && path == pat[..want], "descend({}) in {w:?}: got {}", q(pat), q(&path));
        }
    }
}

/// `occurrence_start` locates an occurrence of every node's path label inside
/// the window, and the suffix of every leaf.
#[test]
fn occurrence_start() {
    fn walk(st: &Snakree, v: NodeId) {
        let (pos, depth) = (st.occurrence_start(v), st.depth(v));
        assert!(pos >= st.begin() && pos + depth <= st.end(), "occurrence of node {v:?} outside window");
        let got: Vec<u8> = (pos..pos + depth).map(|i| st.at(i)).collect();
        assert_eq!(got, path_of(st, v), "text at {pos}");
        assert!(!(v != st.root() && st.is_leaf(v) && pos + depth != st.end()), "leaf {v:?} does not end at end");
        for (_, child) in st.children(v) {
            walk(st, child);
        }
    }
    for s in ["", "a", "banana", "mississippi", "aaaaaaab", "abcabxabcd"] {
        let mut st = Snakree::new();
        st.extend_from_slice(s.as_bytes());
        loop {
            walk(&st, st.root());
            if st.pop_front().is_none() {
                break;
            }
        }
    }
}

#[test]
fn trees_exhaustive() {
    // All interleavings of append(0x00), append(0xff), and deletion through
    // length 8, covering both ends of the byte range.
    fn enumerate(ops: &mut Vec<Operation>) {
        if ops.len() == 8 {
            run_operations(ops);
            return;
        }
        for op in [Operation::Contract, Operation::Push(0x00), Operation::Push(0xff)] {
            ops.push(op);
            enumerate(ops);
            ops.pop();
        }
    }
    enumerate(&mut Vec::new());
}

#[test]
fn trees_random() {
    for seed in 0..12u64 {
        let mut rng = Rng(seed);
        let ops: Vec<Operation> = (0..600)
            .map(|_| {
                if rng.below(100) >= 43 {
                    Operation::Push(rng.below(1 + seed as usize * 2) as u8)
                } else {
                    Operation::Contract
                }
            })
            .collect();
        run_operations(&ops);
    }
}

/// Exercises random bytes with both variable and fixed window widths.
/// Contraction choices are independent of the alphabet and byte values.
#[test]
fn random_raw_bytes() {
    let mut rng = Rng(2024);
    for _ in 0..40 {
        let n = rng.below(512);
        let alphabet = [2, 3, 4, 256][rng.below(4)];
        let text: Vec<u8> = (0..n).map(|_| rng.below(alphabet) as u8).collect();
        let ops: Vec<Operation> =
            text.iter().map(|&c| if rng.below(100) >= 43 { Operation::Push(c) } else { Operation::Contract }).collect();
        run_operations(&ops);
        check_sliding_bytes(&text, 17);
    }
}

#[test]
fn trees_terminator() {
    // Three symbols exercise branches with more than two children. A fresh
    // terminator forces every pending suffix extension to create a leaf.
    fn enumerate(text: &mut Vec<u8>) {
        if text.len() < 6 {
            for c in [0, 128, 255] {
                text.push(c);
                enumerate(text);
                text.pop();
            }
            return;
        }
        let mut h = TestHarness::new();
        for &c in text.iter() {
            h.push(c);
            h.check();
        }
        h.push(1);
        loop {
            h.check();
            let st = &h.st;
            assert!(
                st.active == Locus { node: ROOT_ID, offset: 0 } && st.implicit_suffix_count == 0,
                "pending suffix after terminator in {}",
                q(&h.window)
            );
            let mut leaves = 0;
            let mut v = st.leaves.head;
            while !v.is_nil() {
                leaves += 1;
                v = st.n(v).leaf_next;
            }
            assert_eq!(leaves, h.window.len(), "leaves for terminated window {}", q(&h.window));
            if h.window.is_empty() {
                break;
            }
            h.pop_front();
        }
    }
    enumerate(&mut Vec::new());
}

fn check_sliding_bytes(text: &[u8], width: usize) {
    let mut h = TestHarness::new();
    for &c in text {
        if h.window.len() == width {
            h.pop_front();
            h.check();
        }
        h.push(c);
        h.check();
    }
}

#[test]
fn trees_sliding_bytes() {
    // Repeated binary patterns exercise splits and merges involving 0xff
    // across fixed window widths and buffer capacity boundaries.
    let text = [0, 255, 0, 128, 255, 0, 255, 128, 0].repeat(40);
    for width in [1, 2, 3, 7, 15, 16, 17, 31, 32, 33] {
        check_sliding_bytes(&text, width);
    }
}

fn must_panic(f: impl FnOnce()) {
    assert!(catch_unwind(AssertUnwindSafe(f)).is_err(), "expected panic");
}

#[test]
fn trees_empty_and_bounds() {
    let trees: [Box<dyn TestTree>; 2] = [Box::new(Snakree::new()), Box::new(NaiveTree::new())];
    for mut tree in trees {
        let (pos, length) = tree.repeated_suffix();
        assert!(
            tree.parent(tree.root()).is_none()
                && tree.num_nodes() == 1
                && tree.depth(tree.root()) == 0
                && pos == 0
                && length == 0,
            "invalid empty {}",
            tree.name()
        );
        assert!(tree.pop_front().is_none(), "empty contraction succeeded");
        for pos in [0, 1] {
            must_panic(|| {
                tree.at(pos);
            });
        }
        for c in [0, 255, 0] {
            tree.push(c);
        }
        assert_eq!(tree.at(1), 255, "binary byte not stored");
        must_panic(|| {
            tree.at(3);
        });
        for _ in 0..3 {
            tree.pop_front();
        }
        must_panic(|| {
            tree.at(0);
        });
        tree.push(b'x');
        assert!(tree.begin() == 3 && tree.end() == 4 && tree.at(3) == b'x', "refill positions");
        for id in [NodeId(0), NodeId(u32::MAX)] {
            must_panic(|| {
                tree.parent(id);
            });
        }
    }
}

#[test]
fn position_overflow() {
    let limit = MAX_STREAM_POSITION;
    let (mut st, mut rf) = (Snakree::new(), NaiveTree::new());
    st.text = WindowText::empty_at(limit - 3);
    (rf.window_begin, rf.window_end) = (limit - 3, limit - 3);
    rf.rebuild();
    for &c in b"aba" {
        st.push(c);
        TestTree::push(&mut rf, c);
    }
    CheckContext { st: &st, rf: &rf, window: b"aba", begin: limit - 3, end: limit }.check();
    must_panic(|| st.push(b'x'));
    must_panic(|| TestTree::push(&mut rf, b'x'));
    CheckContext { st: &st, rf: &rf, window: b"aba", begin: limit - 3, end: limit }.check();
    for _ in 0..3 {
        st.pop_front();
        TestTree::pop_front(&mut rf);
    }
    CheckContext { st: &st, rf: &rf, window: &[], begin: limit, end: limit }.check();
}

/// The repeated suffix "aba" sits exactly on an internal node, so the
/// reported occurrence must come from a leaf below it rather than the suffix
/// itself.
#[test]
fn repeated_suffix_explicit_point() {
    let mut st = Snakree::new();
    st.extend_from_slice(b"abacabadaba");
    assert!(st.on_node(st.active) && st.implicit_suffix_count != 0, "test requires a non-root explicit active point");
    let mut rf = NaiveTree::new();
    for &c in b"abacabadaba" {
        TestTree::push(&mut rf, c);
    }
    for tree in [&st as &dyn TestTree, &rf as &dyn TestTree] {
        let (pos, length) = tree.repeated_suffix();
        let window = window_of(tree);
        assert!(
            length == 3 && pos + length < tree.end() && window[pos..pos + length] == *b"aba",
            "{} repeated suffix = ({pos}, {length})",
            tree.name()
        );
    }
}

#[test]
fn batch_updates_and_reuse() {
    // Avoid accessors between updates: querying the tree must not be
    // necessary for the next update to work.
    let mut h = TestHarness::new();
    let mut rng = Rng(4193);
    for _ in 0..100 {
        for _ in 0..100 {
            if h.window.len() == 96 || (!h.window.is_empty() && rng.below(2) == 0) {
                h.pop_front();
            } else {
                h.push(rng.below(5) as u8);
            }
        }
        h.check();
    }
    assert!(h.st.arena.slot_count() <= 2 * 96 + 1 && h.st.text.capacity() <= 128, "storage grew with stream length");
}

#[test]
fn long_repetition() {
    const N: usize = 20000;
    let mut tree = Snakree::new();
    for _ in 0..N {
        tree.push(b'a');
    }
    assert!(tree.num_nodes() == 2 && tree.implicit_suffix_count == N - 1, "repeated input");
    tree.push(b'b'); // a single phase creates a linear number of branches
    assert!(tree.num_nodes() == 2 * N + 1 && tree.active == Locus { node: ROOT_ID, offset: 0 }, "branching phase");
    for _ in 0..N {
        assert_eq!(tree.pop_front(), Some(b'a'), "eviction");
    }
    let mut w = Vec::new();
    tree.window(&mut w);
    assert!(tree.num_nodes() == 2 && w == b"b", "contraction");
    for _ in 0..N {
        tree.push(b'a');
        tree.pop_front();
    }
    assert!(tree.arena.slot_count() == 2 * N + 2 && tree.num_nodes() == 2, "node reuse");
}

#[test]
fn full_alphabet() {
    let mut h = TestHarness::new();
    for c in 0..=255 {
        h.push(c);
    }
    h.check();
    for _ in 0..256 {
        h.pop_front();
    }
    h.check();
}

#[test]
fn occurrence_start_example() {
    let mut tree = Snakree::new();
    tree.extend_from_slice(b"banana");
    let (p, ok) = tree.descend(b"nan");
    assert!(ok);
    let start = p.occurrence_start();
    assert_eq!((start, start + p.depth()), (2, 5));
}

#[test]
fn text_ranges_across_wrap_growth_and_empty_windows() {
    fn check(tree: &Snakree, input: &[u8], begin: usize, end: usize) {
        assert_eq!(tree.range(), begin..end);
        for start in begin..=end {
            for stop in start..=end {
                let expected = &input[start..stop];
                let (first, second) = tree.slices(start..stop);
                assert_eq!([first, second].concat(), expected);
                let mut copied = vec![255];
                tree.copy_range(start..stop, &mut copied);
                assert_eq!(&copied[1..], expected);
                assert_eq!(copied[0], 255);
            }
        }
    }

    let input: Vec<u8> = (0..50).collect();
    let mut tree = Snakree::new();
    check(&tree, &input, 0, 0);
    tree.extend_from_slice(&input[..16]);
    check(&tree, &input, 0, 16);
    for _ in 0..11 {
        tree.pop_front();
    }
    tree.extend_from_slice(&input[16..24]);
    check(&tree, &input, 11, 24);
    let (first, second) = tree.slices(tree.range());
    assert_eq!(first, &input[11..16]);
    assert_eq!(second, &input[16..24]);
    tree.extend_from_slice(&input[24..40]);
    check(&tree, &input, 11, 40);
    while tree.pop_front().is_some() {}
    check(&tree, &input, 40, 40);
    tree.extend_from_slice(&input[40..]);
    check(&tree, &input, 40, 50);
}

#[test]
fn text_ranges_reject_invalid_bounds_before_copying() {
    let mut tree = Snakree::new();
    tree.extend_from_slice(b"banana");
    tree.pop_front();
    for (start, end) in [(0, 0), (0, 2), (2, 1), (1, 7), (7, 7), (usize::MAX, usize::MAX)] {
        assert!(catch_unwind(|| tree.slices(start..end)).is_err());
        let mut dst = vec![42];
        assert!(catch_unwind(AssertUnwindSafe(|| tree.copy_range(start..end, &mut dst))).is_err());
        assert_eq!(dst, [42]);
    }
    let empty = Snakree::new();
    assert!(catch_unwind(|| empty.slices(0..1)).is_err());
    let mut tree = Snakree::new();
    tree.text = WindowText::empty_at(MAX_STREAM_POSITION);
    assert_eq!(tree.range(), MAX_STREAM_POSITION..MAX_STREAM_POSITION);
    assert_eq!(tree.slices(tree.range()), (&[][..], &[][..]));
    let mut dst = vec![42];
    tree.copy_range(tree.range(), &mut dst);
    assert_eq!(dst, [42]);
}

#[test]
fn find_returns_a_live_occurrence_and_context() {
    let mut tree = Snakree::new();
    assert_eq!(tree.find(b""), Some(0));
    assert_eq!(tree.find(b"a"), None);
    tree.extend_from_slice(b"discard:12345banana67890");
    for _ in 0..8 {
        tree.pop_front();
    }
    assert_eq!(tree.find(b""), Some(8));
    assert_eq!(tree.find(b"bananas"), None);
    assert_eq!(tree.find(b"discard"), None);
    for pattern in [&b"ana"[..], b"banana", b"90"] {
        let pos = tree.find(pattern).unwrap();
        let mut actual = Vec::new();
        tree.copy_range(pos..pos + pattern.len(), &mut actual);
        assert_eq!(actual, pattern);
    }
    let pos = tree.find(b"banana").unwrap();
    let start = pos.saturating_sub(5).max(tree.begin());
    let end = (pos + b"banana".len()).saturating_add(5).min(tree.end());
    let mut context = Vec::new();
    tree.copy_range(start..end, &mut context);
    assert_eq!(context, b"12345banana67890");
    while tree.pop_front().is_some() {}
    assert_eq!(tree.find(b""), Some(tree.end()));
    assert_eq!(tree.find(b"banana"), None);
}
