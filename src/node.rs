use crate::point::{NodeId, MAX_NODE_ID};

/// The permanently allocated root, immediately after the reserved NIL slot.
pub(crate) const ROOT_ID: NodeId = NodeId(1);

/// Holds all per-node state for `Snakree`, indexed by node. The struct is 64
/// bytes, matching a typical cache line.
#[derive(Clone, Copy)]
pub(crate) struct TreeNode {
    /// The stored string depth of an internal node; always zero for the root.
    /// Unused for non-root leaves, whose depth is `end() - suffix_start`.
    pub(crate) depth: usize,
    /// The absolute stream position where the represented suffix begins;
    /// only meaningful for non-root leaves.
    pub(crate) suffix_start: usize,
    /// A node's number of children.
    pub(crate) child_count: usize,
    /// A node's parent.
    pub(crate) parent: NodeId,
    /// A node's suffix link target.
    pub(crate) suffix_link: NodeId,
    /// A node's first child.
    pub(crate) first_child: NodeId,
    /// A node's next sibling.
    pub(crate) next_sib: NodeId,
    /// A node's previous sibling.
    pub(crate) prev_sib: NodeId,
    /// The endpoint of this secondary node's primary path; unused for primary
    /// nodes. A secondary leaf points to itself, as does the root of an empty
    /// tree.
    pub(crate) plp: NodeId,
    /// Maps a PLP endpoint leaf back to the secondary node whose PLP points
    /// to it; only meaningful when this node is a leaf.
    pub(crate) iplp: NodeId,
    /// Links leaves into the FIFO queue used to evict them in creation order;
    /// only meaningful when the node is a leaf.
    pub(crate) leaf_next: NodeId,
    /// Reports whether this node is its parent's primary child. The root is
    /// always secondary; PLP ownership belongs to secondary nodes.
    pub(crate) primary: bool,
    /// Marks a node index as currently allocated.
    live: bool,
    /// The first byte of the edge label from a node's parent.
    pub(crate) key: u8,
}

impl Default for TreeNode {
    fn default() -> Self {
        Self {
            depth: 0,
            suffix_start: 0,
            child_count: 0,
            parent: NodeId::NIL,
            suffix_link: NodeId::NIL,
            first_child: NodeId::NIL,
            next_sib: NodeId::NIL,
            prev_sib: NodeId::NIL,
            plp: NodeId::NIL,
            iplp: NodeId::NIL,
            leaf_next: NodeId::NIL,
            primary: false,
            live: false,
            key: 0,
        }
    }
}

/// Owns node slots, their free list, and the live-node count.
/// Tree topology and leaf-pointer updates belong to `Snakree`.
pub(crate) struct NodeArena {
    nodes: Vec<TreeNode>,
    free_list: Vec<NodeId>,
    num_nodes: usize,
}

impl NodeArena {
    /// Reserves the NIL slot and allocates the root without using the free list.
    /// The caller initializes the root's tree-specific state.
    pub(crate) fn new() -> Self {
        Self {
            nodes: vec![TreeNode::default(), TreeNode { live: true, ..TreeNode::default() }],
            free_list: Vec::new(),
            num_nodes: 1,
        }
    }

    /// Returns the number of live nodes, including the root.
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.num_nodes
    }

    /// Returns a slot without validating whether its ID is live.
    #[inline(always)]
    pub(crate) fn get(&self, v: NodeId) -> &TreeNode {
        &self.nodes[v.0 as usize]
    }

    /// Returns a mutable slot without validating whether its ID is live.
    #[inline(always)]
    pub(crate) fn get_mut(&mut self, v: NodeId) -> &mut TreeNode {
        &mut self.nodes[v.0 as usize]
    }

    /// Panics if `v` is nil, out of range, or refers to a released node.
    pub(crate) fn validate(&self, v: NodeId) {
        if v.is_nil() || v.0 as usize >= self.nodes.len() || !self.get(v).live {
            panic!("snakree: invalid node");
        }
    }

    /// Returns a recycled node index if one is free, otherwise grows `nodes`
    /// by one, and marks the result live.
    pub(crate) fn acquire(&mut self) -> NodeId {
        let v = match self.free_list.pop() {
            Some(v) => v,
            None => {
                if self.nodes.len() > MAX_NODE_ID {
                    panic!("snakree: node ID overflow");
                }
                let v = NodeId(self.nodes.len() as u32);
                self.nodes.push(TreeNode::default());
                v
            }
        };
        self.get_mut(v).live = true;
        self.num_nodes += 1;
        v
    }

    /// Clears `v` and returns it to the free list for reuse by
    /// `acquire`.
    pub(crate) fn release(&mut self, v: NodeId) {
        debug_assert!(v != ROOT_ID && !v.is_nil() && self.get(v).live);
        *self.get_mut(v) = TreeNode::default();
        self.free_list.push(v);
        self.num_nodes -= 1;
    }

    /// Returns the number of stored slots, including NIL, for storage-bound tests.
    #[cfg(test)]
    pub(crate) fn slot_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the released IDs for storage-invariant tests.
    #[cfg(test)]
    pub(crate) fn free_nodes(&self) -> &[NodeId] {
        &self.free_list
    }

    /// Reports whether a stored slot is live for storage-invariant tests.
    #[cfg(test)]
    pub(crate) fn is_live(&self, v: NodeId) -> bool {
        self.get(v).live
    }
}

#[cfg(test)]
mod tests {
    use super::{NodeArena, NodeId, ROOT_ID};

    #[test]
    fn reserved_slots_and_reuse() {
        let mut arena = NodeArena::new();
        assert_eq!(arena.len(), 1);
        assert_eq!(arena.slot_count(), 2);
        arena.validate(ROOT_ID);
        assert!(!arena.is_live(NodeId::NIL));
        for id in [NodeId::NIL, NodeId(2), NodeId(u32::MAX)] {
            assert!(std::panic::catch_unwind(|| arena.validate(id)).is_err());
        }

        let first = arena.acquire();
        let second = arena.acquire();
        assert_eq!((first, second), (NodeId(2), NodeId(3)));
        arena.get_mut(first).suffix_start = 123;
        arena.get_mut(first).parent = ROOT_ID;
        arena.get_mut(first).primary = true;
        arena.release(first);
        assert_eq!(arena.len(), 2);
        assert!(std::panic::catch_unwind(|| arena.validate(first)).is_err());
        arena.validate(second);
        arena.validate(ROOT_ID);

        assert_eq!(arena.acquire(), first);
        arena.validate(first);
        assert_eq!(arena.len(), 3);
        assert_eq!(arena.slot_count(), 4);
        assert_eq!(arena.get(first).suffix_start, 0);
        assert_eq!(arena.get(first).parent, NodeId::NIL);
        assert!(!arena.get(first).primary);
        assert!(arena.free_nodes().is_empty());
    }
}
