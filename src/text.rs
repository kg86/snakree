use std::ops::Range;

/// The capacity allocated for the first byte appended to an empty tree.
const INITIAL_TEXT_CAPACITY: usize = 16;

/// The largest absolute stream position. `push` must reject an append at this
/// position because `end` cannot represent the resulting half-open endpoint.
pub(crate) const MAX_STREAM_POSITION: usize = usize::MAX;

/// Owns a growing circular buffer and the absolute bounds of its live window.
/// Buffer slots are indexed by absolute position modulo the buffer length.
#[derive(Default)]
pub(crate) struct WindowText {
    buffer: Vec<u8>,
    begin: usize,
    end: usize,
}

impl WindowText {
    /// Returns the absolute stream position of the first byte of the window.
    #[inline]
    pub(crate) fn begin(&self) -> usize {
        self.begin
    }

    /// Returns the absolute stream position just past the last byte of the
    /// window. Equals the number of bytes ever pushed.
    #[inline]
    pub(crate) fn end(&self) -> usize {
        self.end
    }

    /// Returns the window length, `end - begin`.
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.end - self.begin
    }

    /// Reports whether the window is empty.
    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns `T[i]`. Panics unless `begin <= i < end`.
    #[inline]
    pub(crate) fn at(&self, i: usize) -> u8 {
        if i < self.begin || i >= self.end {
            panic!("snakree: position outside window");
        }
        self.buffer[i % self.buffer.len()]
    }

    /// Appends a byte and advances the absolute end position. Overflow checks
    /// happen before any state is changed.
    pub(crate) fn push(&mut self, c: u8) {
        if self.end == MAX_STREAM_POSITION {
            panic!("snakree: stream position overflow");
        }
        self.ensure_capacity();
        let capacity = self.buffer.len();
        self.buffer[self.end % capacity] = c;
        self.end += 1;
    }

    /// Discards the first byte of a non-empty window without shrinking storage.
    pub(crate) fn discard_front(&mut self) {
        assert!(!self.is_empty(), "snakree: cannot discard from empty window");
        self.begin += 1;
    }

    /// Creates an empty window at an arbitrary position for overflow tests.
    #[cfg(test)]
    pub(crate) fn empty_at(position: usize) -> Self {
        Self { begin: position, end: position, ..Self::default() }
    }

    /// Returns the number of allocated byte slots for storage-bound tests.
    #[cfg(test)]
    pub(crate) fn capacity(&self) -> usize {
        self.buffer.len()
    }

    /// Appends `T[b..e)` to `dst`, for a range inside the window.
    pub(crate) fn append_text(&self, dst: &mut Vec<u8>, b: usize, e: usize) {
        let (first, second) = self.slices(b..e);
        dst.extend_from_slice(first);
        dst.extend_from_slice(second);
    }

    /// Returns the length of the longest common prefix of the window text in `range` and `pattern`.
    pub(crate) fn common_prefix(&self, range: Range<usize>, pattern: &[u8]) -> usize {
        let (first, second) = self.slices(range);
        common_prefix_split(first, second, pattern)
    }

    /// Borrows a range inside the window in stream order.
    pub(crate) fn slices(&self, range: Range<usize>) -> (&[u8], &[u8]) {
        assert!(
            self.begin <= range.start && range.start <= range.end && range.end <= self.end,
            "snakree: range outside window"
        );
        if range.is_empty() {
            return (&[], &[]);
        }
        let start = range.start % self.buffer.len();
        let length = range.end - range.start;
        let first_length = length.min(self.buffer.len() - start);
        (&self.buffer[start..start + first_length], &self.buffer[..length - first_length])
    }

    /// Ensures the buffer has room for the next byte. It is a ring buffer
    /// indexed by absolute stream position mod its length (see `at`); when the
    /// sliding window fills it completely, its capacity is doubled. Since every
    /// access re-derives its slot from the absolute position and the current
    /// buffer length, copying the live window into the larger buffer is enough
    /// to make it visible under the new length — no node needs its stored
    /// positions relabeled.
    fn ensure_capacity(&mut self) {
        if self.len() < self.buffer.len() {
            return;
        }
        let mut new_cap = self.buffer.len();
        if new_cap > MAX_STREAM_POSITION / 2 {
            panic!("snakree: buffer capacity overflow");
        }
        new_cap *= 2;
        if new_cap == 0 {
            new_cap = INITIAL_TEXT_CAPACITY;
        }
        let mut buf = vec![0; new_cap];
        let mut position = self.begin;
        while position < self.end {
            let source = position % self.buffer.len();
            let destination = position % new_cap;
            let length = (self.end - position).min(self.buffer.len() - source).min(new_cap - destination);
            buf[destination..destination + length].copy_from_slice(&self.buffer[source..source + length]);
            position += length;
        }
        self.buffer = buf;
    }
}

/// Returns the length of the longest common prefix of the concatenation `s1 ++ s2` and `b`, without materializing the concatenation.
fn common_prefix_split(s1: &[u8], s2: &[u8], b: &[u8]) -> usize {
    let m = common_prefix(s1, b);
    if m == s1.len() {
        m + common_prefix(s2, &b[m..])
    } else {
        m
    }
}

/// Returns the length of the longest common prefix of `a` and `b`.
fn common_prefix(a: &[u8], b: &[u8]) -> usize {
    let n = a.len().min(b.len());
    let (a, b) = (&a[..n], &b[..n]);
    let mut i = 0;
    while i + 8 <= n {
        let x =
            u64::from_le_bytes(a[i..i + 8].try_into().unwrap()) ^ u64::from_le_bytes(b[i..i + 8].try_into().unwrap());
        if x != 0 {
            return i + (x.trailing_zeros() / 8) as usize;
        }
        i += 8;
    }
    while i < n && a[i] == b[i] {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    use super::{common_prefix_split, WindowText};
    use crate::{Snakree, MAX_WINDOW_LEN};

    #[test]
    fn common_prefix_split_stops_inside_first_slice() {
        assert_eq!(common_prefix_split(b"abcdefghij", b"klm", b"abcdeXghij"), 5);
    }

    #[test]
    fn common_prefix_split_stops_inside_second_slice() {
        assert_eq!(common_prefix_split(b"abcdefghij", b"klmnop", b"abcdefghijklXnop"), 12);
    }

    #[test]
    fn common_prefix_split_consumes_both_slices() {
        assert_eq!(common_prefix_split(b"abc", b"def", b"abcdefgh"), 6);
        assert_eq!(common_prefix_split(b"abc", b"def", b"abcdef"), 6);
    }

    #[test]
    fn common_prefix_split_handles_short_pattern_and_empty_slices() {
        assert_eq!(common_prefix_split(b"abc", b"def", b"ab"), 2);
        assert_eq!(common_prefix_split(b"abc", b"def", b"abcd"), 4);
        assert_eq!(common_prefix_split(b"", b"def", b"dex"), 2);
        assert_eq!(common_prefix_split(b"abc", b"", b"abcx"), 3);
        assert_eq!(common_prefix_split(b"", b"", b"abc"), 0);
        assert_eq!(common_prefix_split(b"abc", b"def", b""), 0);
    }

    #[test]
    fn window_limit_rejects_push_before_mutation() {
        for length in [MAX_WINDOW_LEN, MAX_WINDOW_LEN + 1] {
            for input in [b"banana", b"aaaaaa"] {
                let mut tree = Snakree::new();
                tree.extend_from_slice(input);
                let active = tree.active;
                let implicit_suffix_count = tree.implicit_suffix_count;
                let nodes = tree.num_nodes();
                let slots = tree.arena.slot_count();
                let leaves = (tree.leaves.head, tree.leaves.tail);
                // Simulate the length boundary without a multi-gigabyte allocation.
                // Setting end to its limit also makes a missing window check fail
                // with a different panic before attempting any allocation.
                let text = std::mem::replace(
                    &mut tree.text,
                    WindowText { buffer: Vec::new(), begin: usize::MAX - length, end: usize::MAX },
                );
                let panic = catch_unwind(AssertUnwindSafe(|| tree.push(b'x'))).unwrap_err();
                assert_eq!(panic.downcast_ref::<&str>(), Some(&"snakree: maximum window length exceeded"));
                assert_eq!(tree.range(), usize::MAX - length..usize::MAX);
                assert_eq!(tree.text.capacity(), 0);
                assert_eq!(tree.active, active);
                assert_eq!(tree.implicit_suffix_count, implicit_suffix_count);
                assert_eq!(tree.num_nodes(), nodes);
                assert_eq!(tree.arena.slot_count(), slots);
                assert_eq!((tree.leaves.head, tree.leaves.tail), leaves);
                tree.text = text;
                let mut actual = Vec::new();
                tree.window(&mut actual);
                assert_eq!(actual, input);
                assert_eq!(tree.pop_front(), Some(input[0]));
                tree.push(b'x');
                assert!(tree.contains(&input[1..]));
                assert_eq!(tree.find(b"x"), Some(input.len()));
            }
        }
    }

    #[test]
    fn growth_preserves_every_wrap_alignment() {
        for capacity in [16, 32, 64] {
            for begin in 0..4 * capacity {
                let mut text = WindowText { buffer: vec![0; capacity], begin, end: begin + capacity };
                let expected: Vec<u8> = (begin..=text.end).map(|i| i as u8).collect();
                for i in begin..text.end {
                    text.buffer[i % capacity] = i as u8;
                }
                text.push(text.end as u8);
                assert_eq!(text.capacity(), 2 * capacity);
                for (offset, &byte) in expected.iter().enumerate() {
                    assert_eq!(text.at(begin + offset), byte);
                }
                let mut actual = Vec::new();
                text.append_text(&mut actual, begin, text.end);
                assert_eq!(actual, expected);
            }
        }
    }

    #[test]
    fn wrapped_growth_and_refill() {
        let mut text = WindowText::default();
        let input: Vec<u8> = (0..100).collect();
        for &c in &input[..16] {
            text.push(c);
        }
        for _ in 0..11 {
            text.discard_front();
        }
        // Wrap the live range, then grow while it straddles the buffer end.
        for &c in &input[16..] {
            text.push(c);
            assert_eq!((text.begin(), text.end(), text.len()), (11, c as usize + 1, c as usize - 10));
            let mut actual = vec![255];
            text.append_text(&mut actual, text.begin(), text.end());
            assert_eq!(&actual[1..], &input[11..=c as usize]);
            assert_eq!(actual[0], 255);
            for (i, &expected) in input.iter().enumerate().take(text.end()).skip(text.begin()) {
                assert_eq!(text.at(i), expected);
            }
        }
        let capacity = text.capacity();
        while !text.is_empty() {
            text.discard_front();
        }
        let mut actual = Vec::new();
        text.append_text(&mut actual, text.begin(), text.end());
        assert!(actual.is_empty());
        text.push(42);
        assert_eq!((text.begin(), text.end(), text.at(100)), (100, 101, 42));
        assert_eq!(text.capacity(), capacity);
    }
}
