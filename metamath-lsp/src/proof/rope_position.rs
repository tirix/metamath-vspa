//! A data structure for marking boundaries in a rope.
//  This is heavily based on xi_rope::Cursor


/// A data structure for marking boundaries in a rope.
pub struct RopePosition<N: NodeInfo> {
    /// The tree being traversed by this cursor.
    root: Arc<Node<N>>,
    /// The cache holds the tail of the path from the root to the current leaf.
    ///
    /// Each entry is a reference to the parent node and the index of the child. It
    /// is stored bottom-up; `cache[0]` is the parent of the leaf and the index of
    /// the leaf within that parent.
    ///
    /// The main motivation for this being a fixed-size array is to keep the cursor
    /// an allocation-free data structure.
    cache: [Option<(&'a Node<N>, usize)>; CURSOR_CACHE_SIZE],
    /// The leaf containing the current position, when the cursor is valid.
    ///
    /// The position is only at the end of the leaf when it is at the end of the tree.
    leaf: Arc<N::L>,
    /// The offset of `leaf` within the tree.
    offset_of_leaf: usize,
}

impl<'a, N: NodeInfo> Cursor<'a, N> {
}