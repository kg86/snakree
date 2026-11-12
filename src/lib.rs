//! Sliding-window suffix trees based on Ukkonen's online construction
//! algorithm.
//!
//! [`Snakree::new`] returns the crate's sliding suffix tree: an online
//! implementation using primary leaf pointers and `u32`-backed [`NodeId`]
//! values throughout. [`Snakree::push`] appends a byte to the right end, and
//! [`Snakree::pop_front`] evicts the leftmost byte.
//! [`Snakree::repeated_suffix`] reports an earlier occurrence of the longest
//! repeated suffix. Generic index widths and pattern occurrence counting are
//! not supported.
//!
//! Positions, lengths, depths and offsets use `usize` and are measured in
//! bytes.
//! The window length cannot exceed [`MAX_WINDOW_LEN`] bytes.
//! [`Snakree::push`] rejects an append beyond this limit before changing the tree.
//!
//! # Model
//!
//! Conceptually the tree indexes a stream `T[0..end)`. The window is the
//! half-open range `T[begin..end)`, and the tree is the (implicit) suffix tree
//! of that window. All positions exposed by the API are absolute stream
//! positions, so they keep increasing as the window slides; only positions
//! inside `[begin, end)` may be dereferenced with [`Snakree::at`].
//!
//! # Implicit suffix tree
//!
//! The tree is implicit: a suffix of the window that is also a prefix of
//! another suffix, that is, a suffix that occurs earlier in the window, ends at
//! an internal node or in the middle of an edge and has no leaf of its own.
//! Only the suffixes that occur once are leaves. Consequently the leaves below
//! a locus do not enumerate every occurrence of its string, and their number
//! is not an occurrence count. [`Snakree::contains`] and [`Snakree::descend`]
//! are unaffected, since every substring of the window has a locus.
//!
//! # Example
//!
//! ```
//! let mut tree = snakree::Snakree::new();
//! tree.extend_from_slice(b"banana");
//!
//! // A failed full match still returns the longest matching prefix.
//! let (p, matched) = tree.descend(b"band");
//! let mut path = Vec::new();
//! p.path(&mut path);
//! assert!(!matched);
//! assert_eq!((path.as_slice(), p.depth()), (&b"ban"[..], 3));
//! ```
//!
//! # References
//!
//! Laurentius Leonard, Shunsuke Inenaga, Hideo Bannai, Takuya Mieno,
//! "Sliding Suffix Trees Revisited", <https://arxiv.org/abs/2307.01412>
//!
//! Esko Ukkonen, "On-line construction of suffix trees", Algorithmica 14(3),
//! 249–260, 1995. <https://www.cs.helsinki.fi/u/ukkonen/SuffixT1withFigs.pdf>

mod leaf;
mod node;
mod point;
mod query;
mod snakree;
mod text;

#[cfg(test)]
mod naive;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod testutil;

pub use crate::point::{NodeId, Point};
pub use crate::query::Children;
pub use crate::snakree::{Snakree, MAX_WINDOW_LEN};
