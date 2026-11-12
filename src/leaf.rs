use crate::node::NodeArena;
use crate::point::NodeId;

/// Contains exactly the non-root leaves representing explicit (non-repeating) suffixes at operation boundaries.
/// Entries are node indices, linked through `TreeNode::leaf_next` in the arena.
/// Each entry's `suffix_start` is the represented suffix's start, and entries are ordered by increasing `suffix_start` (oldest to newest).
/// The head and tail are both NIL exactly when the window is empty.
/// The root is never enqueued, even though it has no children in an empty tree.
pub(crate) struct LeafQueue {
    /// The oldest leaf in the FIFO.
    pub(crate) head: NodeId,
    /// The newest leaf in the FIFO.
    pub(crate) tail: NodeId,
}

impl LeafQueue {
    /// Creates an empty queue.
    pub(crate) fn new() -> Self {
        Self { head: NodeId::NIL, tail: NodeId::NIL }
    }

    /// Appends leaf `v` to the tail so `pop_front` can find the oldest leaf to evict.
    pub(crate) fn enqueue(&mut self, arena: &mut NodeArena, v: NodeId) {
        if self.tail.is_nil() {
            self.head = v;
        } else {
            arena.get_mut(self.tail).leaf_next = v;
        }
        self.tail = v;
        arena.get_mut(v).leaf_next = NodeId::NIL;
    }

    /// Removes and returns the oldest live suffix leaf.
    /// The caller must have established that the window is non-empty, which guarantees a non-nil head.
    pub(crate) fn dequeue(&mut self, arena: &mut NodeArena) -> NodeId {
        let v = self.head;
        self.head = arena.get(v).leaf_next;
        if self.head.is_nil() {
            self.tail = NodeId::NIL;
        }
        arena.get_mut(v).leaf_next = NodeId::NIL;
        v
    }
}
