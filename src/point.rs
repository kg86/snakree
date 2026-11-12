use crate::snakree::Snakree;

/// Identifies a node of a [`Snakree`]. IDs are opaque handles that are only
/// meaningful for the tree that produced them, and they may be reused after
/// the node they refer to has been removed by `pop_front`. Because a [`Point`]
/// borrows its tree while a `NodeId` does not, an ID is valid only until the
/// next `push` or `pop_front` on its tree by contract rather than by the type
/// system, and must be reacquired afterward.
///
/// Accessors reject out-of-range and released IDs, but cannot detect an ID
/// from another tree or a stale ID whose slot has been reused.
///
/// Its `u32` width bounds the number of addressable nodes while keeping node
/// references compact in `Snakree`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct NodeId(pub(crate) u32);

/// The largest index representable by `NodeId`, as a `usize` so it can be
/// compared directly with a vector length before converting it.
pub(crate) const MAX_NODE_ID: usize = u32::MAX as usize;

impl NodeId {
    /// The reserved ID that stands for "no node" in the stored references;
    /// valid nodes are numbered from 1. It never leaves the crate: public
    /// accessors return `Option<NodeId>` instead.
    pub(crate) const NIL: NodeId = NodeId(0);

    /// Reports whether this is [`NodeId::NIL`].
    #[inline]
    pub(crate) fn is_nil(self) -> bool {
        self == NodeId::NIL
    }

    /// Converts NIL to `None` for the public API.
    #[inline]
    pub(crate) fn into_option(self) -> Option<NodeId> {
        if self.is_nil() {
            None
        } else {
            Some(self)
        }
    }
}

/// A locus in a tree: a position on some root-to-leaf path, which is not
/// necessarily an explicit node. It plays the role of the canonical
/// reference pair in Ukkonen's algorithm.
///
/// A `Point` denotes the first `offset()` bytes of the label of the edge
/// entering `node()`, so the string it spells out is
///
/// ```text
/// path(parent(node())) + label(node())[..offset()]
/// ```
///
/// The invariant is
///
/// ```text
/// 0 < offset() <= edge_len(node()), or node() == root() && offset() == 0
/// ```
///
/// so every locus has exactly one representation. `offset() ==
/// edge_len(node())` means the point sits exactly on `node()`. A `Point`
/// borrows its tree, so the borrow checker guarantees that it is not used
/// after the next `push` or `pop_front`.
///
/// Obtain a `Point` from [`Snakree::descend`], [`Snakree::root_point`],
/// [`Snakree::point_at`] or [`Point::descend`].
#[derive(Clone, Copy)]
pub struct Point<'a> {
    pub(crate) tree: &'a Snakree,
    pub(crate) node: NodeId,
    pub(crate) offset: usize,
}

/// The node and offset of a [`Point`] without its tree, under the same
/// invariant. The tree keeps its own loci, such as the active point, in this
/// form so that it never holds a reference to itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Locus {
    pub(crate) node: NodeId,
    pub(crate) offset: usize,
}

impl<'a> Point<'a> {
    /// Returns the destination node of the edge containing the point. If
    /// `on_node` reports true, this is the explicit node at the point.
    #[inline]
    pub fn node(&self) -> NodeId {
        self.node
    }

    /// Returns the number of bytes consumed along the edge entering `node()`.
    /// The root has offset zero.
    #[inline]
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Reports whether the point sits exactly on an explicit node. If it
    /// does, `node()` is that node.
    pub fn on_node(&self) -> bool {
        self.offset == self.tree.raw_edge_len(self.node)
    }

    /// Returns the string depth, i.e. the length of the string the point
    /// spells out.
    pub fn depth(&self) -> usize {
        self.tree.node_depth(self.node) - self.tree.raw_edge_len(self.node) + self.offset
    }

    /// Moves the point one byte down along `c`, or returns `None` if `c` does
    /// not occur there.
    pub fn descend(&self, c: u8) -> Option<Point<'a>> {
        let (b, e) = self.tree.raw_edge(self.node);
        if self.offset < e - b {
            if self.tree.at(b + self.offset) != c {
                return None;
            }
            return Some(Point { offset: self.offset + 1, ..*self });
        }
        let child = self.tree.raw_child(self.node, c).into_option()?;
        Some(Point { tree: self.tree, node: child, offset: 1 })
    }

    /// Returns the absolute start position of an occurrence of the string
    /// spelled out by the point in the window. The occurrence occupies
    /// `[pos, pos + depth())`. Which occurrence is returned is unspecified;
    /// the root yields `begin`.
    pub fn occurrence_start(&self) -> usize {
        self.tree.occurrence_start(self.node)
    }

    /// Appends the string spelled out by the point to `dst`.
    pub fn path(&self, dst: &mut Vec<u8>) {
        let k = self.occurrence_start();
        self.tree.append_text(dst, k, k + self.depth());
    }
}
