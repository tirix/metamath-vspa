//! This is a xi-rope data structure, which also holds information about proof step_starts
//  This is heavily based on xi_rope::Rope
use std::borrow::Cow;
use std::str::FromStr;
use std::string::ParseError;

use crate::rope_ext::{RopeExt, StringTreeBuilder};
use lsp_types::TextDocumentContentChangeEvent;
use memchr::{memchr, memrchr};
use std::cmp::{max, min, Ordering};
use xi_rope::engine::Error;
use xi_rope::rope::count_newlines;
use xi_rope::tree::Node;
use xi_rope::tree::NodeInfo;
use xi_rope::tree::TreeBuilder;
use xi_rope::tree::{DefaultMetric, Leaf};
use xi_rope::Interval;
use xi_rope::{Cursor, Delta, Metric};

/// Structure for a rope containing proof information
/// This structure allows to retrieve line number information, but also step number.
#[derive(Clone, Default)]
pub struct ProofRope(Node<StepsInfo>);

impl ProofRope {
    /// Creates a proof text from the provided reader
    pub fn from_reader<T: std::io::Read>(file: T) -> Result<Self, std::io::Error> {
        Ok(Self(crate::rope_ext::read_to_rope(file)?))
    }

    /// Apply the given transformation to this proof
    pub fn apply(&self, delta: ProofDelta) -> Self {
        ProofRope(delta.0.apply(&self.0))
    }
}

impl FromStr for ProofRope {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<ProofRope, Self::Err> {
        let mut b = TreeBuilder::new();
        b.push_string(s);
        Ok(ProofRope(b.build()))
    }
}

impl RopeExt<StepsInfo> for ProofRope {
    fn line_to_offset(&self, line_idx: usize) -> usize {
        let max_line = self.0.measure::<LinesMetric>() + 1;
        match line_idx.cmp(&max_line) {
            Ordering::Greater => {
                panic!("line number {} beyond last line {}", line_idx, max_line);
            }
            Ordering::Equal => self.0.len(),
            Ordering::Less => self.0.count_base_units::<LinesMetric>(line_idx),
        }
    }

    fn offset_to_line(&self, byte_idx: usize) -> usize {
        self.0.count::<LinesMetric>(byte_idx)
    }

    fn cow_for_range<T>(&self, range: T) -> std::borrow::Cow<str>
    where
        T: xi_rope::interval::IntervalBounds,
    {
        let Interval { start, end } = range.into_interval(self.0.len());
        let mut iter = ChunkIter {
            cursor: Cursor::new(&self.0, start),
            end,
        };
        let first = iter.next();
        let second = iter.next();

        match (first, second) {
            (None, None) => Cow::from(""),
            (Some(s), None) => Cow::from(s),
            (Some(one), Some(two)) => {
                let mut result = [one, two].concat();
                for chunk in iter {
                    result.push_str(chunk);
                }
                Cow::from(result)
            }
            (None, Some(_)) => unreachable!(),
        }
    }

    fn cursor_to_lsp_position(
        &self,
        _cursor: xi_rope::Cursor<StepsInfo>,
    ) -> Result<lsp_types::Position, xi_rope::engine::Error> {
        todo!()
    }

    fn lsp_position_to_cursor(
        &self,
        _position: lsp_types::Position,
    ) -> Result<xi_rope::Cursor<StepsInfo>, xi_rope::engine::Error> {
        todo!()
    }

    fn char_len(&self) -> usize {
        self.0.len()
    }
}

pub struct ChunkIter<'a> {
    cursor: Cursor<'a, StepsInfo>,
    end: usize,
}

impl<'a> Iterator for ChunkIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        if self.cursor.pos() >= self.end {
            return None;
        }
        let (leaf, start_pos) = self.cursor.get_leaf().unwrap();
        let len = min(self.end - self.cursor.pos(), leaf.len() - start_pos);
        self.cursor.next_leaf();
        Some(&leaf.text[start_pos..start_pos + len])
    }
}

const MIN_LEAF: usize = 511;
const MAX_LEAF: usize = 1024;

/// This `StepsInfo` structure is used as nodes in the `ProofRope`.
/// It stores both the number of lines and the number of step_starts at each node.
///
/// It is associated with the `StepsLeaf` leaf structure, which holds
/// the string at the leaf, as well as the corresponding step_starts.
#[derive(Clone)]
pub struct StepsInfo {
    lines: usize,
    step_starts: usize,
}

impl NodeInfo for StepsInfo {
    type L = StepsLeaf;

    /// Accumulate the provided node with this one.
    /// Simply adds up the numbers of lines and step_starts.
    fn accumulate(&mut self, other: &Self) {
        self.lines += other.lines;
        self.step_starts += other.step_starts;
    }

    /// Initial calculations for a leaf.
    fn compute_info(l: &Self::L) -> Self {
        StepsInfo {
            lines: count_newlines(&l.text),
            step_starts: l.step_starts.len(),
        }
    }
}

impl DefaultMetric for StepsInfo {
    type DefaultMetric = BaseMetric;
}

/// If there is any space character at the beginning of a line,
/// it is a follow-up of the previous line, and belongs to the same step.
fn is_followup_char(c: u8) -> bool {
    c == b' ' || c == b'\t' || c == b'\n'
}

fn find_leaf_split_for_bulk(s: &str) -> usize {
    find_leaf_split(s, MIN_LEAF)
}

fn find_leaf_split_for_merge(s: &str) -> usize {
    find_leaf_split(s, max(MIN_LEAF, s.len() - MAX_LEAF))
}

// Try to split at the first possible step boundary, if not, then split at codepoint
fn find_leaf_split(s: &str, minsplit: usize) -> usize {
    let splitpoint = min(MAX_LEAF, s.len() - MIN_LEAF);
    let bytes = s.as_bytes();
    let mut offset = minsplit - 1;
    while let Some(pos) = memrchr(b'\n', &bytes[offset..splitpoint]) {
        offset += pos + 1;
    }
    offset
}

/// This step_starts leaf stores 2 kinds of information:
#[derive(Clone, Default)]
pub struct StepsLeaf {
    /// content of this leaf
    text: String,
    /// step_starts
    step_starts: Vec<usize>,
}

impl From<&str> for StepsLeaf {
    fn from(s: &str) -> Self {
        let mut step_starts = vec![];
        let mut offset = 0;
        let bytes = s.as_bytes();
        while offset < s.len() {
            if let Some(pos) = memchr(b'\n', &bytes[offset..]) {
                offset += pos + 1;
                if offset < bytes.len() && !is_followup_char(bytes[offset]) {
                    step_starts.push(offset);
                }
            } else {
                break;
            }
        }
        Self {
            text: s.to_owned(),
            step_starts,
        }
    }
}

impl Leaf for StepsLeaf {
    fn len(&self) -> usize {
        self.text.len()
    }

    fn is_ok_child(&self) -> bool {
        self.len() >= MIN_LEAF
    }

    fn push_maybe_split(&mut self, other: &StepsLeaf, iv: Interval) -> Option<StepsLeaf> {
        //println!("push_maybe_split [{}] [{}] {:?}", self, other, iv);
        let (start, end) = iv.start_end();
        let len = self.len();
        self.text.push_str(&other.text[start..end]);
        for &v in &other.step_starts {
            if start < v && v <= end {
                self.step_starts.push(v - start + len);
            }
        }
        if self.text.len() <= MAX_LEAF {
            None
        } else {
            let splitpoint = find_leaf_split_for_merge(&self.text);
            let right_str = self.text[splitpoint..].to_owned();
            let split_index = self
                .step_starts
                .iter()
                .position(|index| *index > splitpoint)
                .unwrap_or(0);
            self.text.truncate(splitpoint);
            self.text.shrink_to_fit();
            Some(StepsLeaf {
                text: right_str,
                step_starts: self.step_starts.split_off(split_index),
            })
        }
    }

    fn subseq(&self, iv: Interval) -> Self {
        let mut result = Self::default();
        if result.push_maybe_split(self, iv).is_some() {
            panic!("unexpected split");
        }
        result
    }
}

/// Represents a proof text transformation
pub struct ProofDelta(Delta<StepsInfo>);

impl ProofDelta {
    pub fn from_lsp_change(
        source: &ProofRope,
        change: &TextDocumentContentChangeEvent,
    ) -> Result<Self, Error> {
        Ok(Self(source.change_event_to_rope_delta(change)?))
    }

    // /// Deleted ranges
    // pub fn deletions(&self) -> DeletionsIter<StepsInfo> {
    //     self.0.iter_deletions()
    // }

    // pub fn insertions(&self) -> InsertsIter<StepsInfo> {
    //     self.0.iter_inserts()
    // }
}

/// This metric let us walk utf8 text by code point.
///
/// `BaseMetric` implements the trait [Metric].  Both its _measured unit_ and
/// its _base unit_ are utf8 code unit.
#[derive(Clone, Copy)]
pub struct BaseMetric(usize); // number of chars

#[derive(Clone, Copy)]
pub struct LinesMetric(usize); // number of lines

#[derive(Clone, Copy)]
pub struct StepsMetric(usize); // number of step_starts

impl Metric<StepsInfo> for BaseMetric {
    fn measure(_: &StepsInfo, len: usize) -> usize {
        len
    }

    fn to_base_units(s: &StepsLeaf, in_measured_units: usize) -> usize {
        debug_assert!(s.text.is_char_boundary(in_measured_units));
        in_measured_units
    }

    fn from_base_units(s: &StepsLeaf, in_base_units: usize) -> usize {
        debug_assert!(s.text.is_char_boundary(in_base_units));
        in_base_units
    }

    fn is_boundary(s: &StepsLeaf, offset: usize) -> bool {
        s.text.is_char_boundary(offset)
    }

    fn prev(s: &StepsLeaf, offset: usize) -> Option<usize> {
        if offset == 0 {
            // I think it's a precondition that this will never be called
            // with offset == 0, but be defensive.
            None
        } else {
            let mut len = 1;
            while !s.text.is_char_boundary(offset - len) {
                len += 1;
            }
            Some(offset - len)
        }
    }

    fn next(s: &StepsLeaf, offset: usize) -> Option<usize> {
        if offset == s.len() {
            // I think it's a precondition that this will never be called
            // with offset == s.len(), but be defensive.
            None
        } else {
            let b = s.text.as_bytes()[offset];
            Some(offset + xi_rope::rope::len_utf8_from_first_byte(b))
        }
    }

    fn can_fragment() -> bool {
        false
    }
}

/// Measured unit is newline amount.
/// Base unit is byte.
/// Boundary is trailing and determined by a newline char.
impl Metric<StepsInfo> for LinesMetric {
    fn measure(info: &StepsInfo, _: usize) -> usize {
        info.lines
    }

    fn is_boundary(s: &StepsLeaf, offset: usize) -> bool {
        if offset == 0 {
            // shouldn't be called with this, but be defensive
            false
        } else {
            s.text.as_bytes()[offset - 1] == b'\n'
        }
    }

    fn to_base_units(s: &StepsLeaf, in_measured_units: usize) -> usize {
        let mut offset = 0;
        for _ in 0..in_measured_units {
            match memchr(b'\n', &s.text.as_bytes()[offset..]) {
                Some(pos) => offset += pos + 1,
                _ => panic!("to_base_units called with arg too large"),
            }
        }
        offset
    }

    fn from_base_units(s: &StepsLeaf, in_base_units: usize) -> usize {
        count_newlines(&s.text[..in_base_units])
    }

    fn prev(s: &StepsLeaf, offset: usize) -> Option<usize> {
        debug_assert!(offset > 0, "caller is responsible for validating input");
        memrchr(b'\n', &s.text.as_bytes()[..offset - 1]).map(|pos| pos + 1)
    }

    fn next(s: &StepsLeaf, offset: usize) -> Option<usize> {
        memchr(b'\n', &s.text.as_bytes()[offset..]).map(|pos| offset + pos + 1)
    }

    fn can_fragment() -> bool {
        true
    }
}

/// Measured unit is newline amount.
/// Base unit is utf8 code unit.
/// Boundary is trailing and determined by a newline char.
impl Metric<StepsInfo> for StepsMetric {
    fn measure(info: &StepsInfo, _: usize) -> usize {
        info.step_starts
    }

    ///
    fn is_boundary(s: &StepsLeaf, offset: usize) -> bool {
        if offset == 0 {
            // shouldn't be called with this, but be defensive
            false
        } else {
            s.text.as_bytes()[offset - 1] == b'\n' && !is_followup_char(s.text.as_bytes()[offset])
        }
    }

    /// Returns the smallest offset, in base units, for an offset in step_starts.
    fn to_base_units(l: &StepsLeaf, in_measured_units: usize) -> usize {
        if in_measured_units > l.step_starts.len() {
            l.len() + 1
        } else if in_measured_units == 0 {
            0
        } else {
            l.step_starts[in_measured_units - 1]
        }
    }

    /// Counts how many step starts there are in the given string
    fn from_base_units(l: &StepsLeaf, in_base_units: usize) -> usize {
        match l.step_starts.binary_search(&in_base_units) {
            Ok(n) => n + 1,
            Err(n) => n,
        }
    }

    fn prev(l: &StepsLeaf, offset: usize) -> Option<usize> {
        for i in 0..l.step_starts.len() {
            if offset <= l.step_starts[i] {
                if i == 0 {
                    return None;
                } else {
                    return Some(l.step_starts[i - 1]);
                }
            }
        }
        l.step_starts.last().cloned()
    }

    fn next(l: &StepsLeaf, offset: usize) -> Option<usize> {
        let n = match l.step_starts.binary_search(&offset) {
            Ok(n) => n + 1,
            Err(n) => n,
        };

        if n == l.step_starts.len() {
            None
        } else {
            Some(l.step_starts[n])
        }
    }

    fn can_fragment() -> bool {
        true
    }
}

impl StringTreeBuilder for TreeBuilder<StepsInfo> {
    /// Push a string on the accumulating tree in the naive way.
    ///
    /// Splits the provided string in chunks that fit in a leaf
    /// and pushes the leaves one by one onto the tree by calling
    /// `push_leaf` on the builder.
    fn push_string(&mut self, mut s: &str) {
        if s.len() <= MAX_LEAF {
            if !s.is_empty() {
                self.push_leaf(s.into());
            }
            return;
        }
        while !s.is_empty() {
            let splitpoint = if s.len() > MAX_LEAF {
                find_leaf_split_for_bulk(s)
            } else {
                s.len()
            };
            self.push_leaf(s[..splitpoint].into());
            s = &s[splitpoint..];
        }
    }
}

#[cfg(test)]
mod tests {
    use lsp_types::{Position, Range, TextDocumentContentChangeEvent};
    use std::{borrow::Cow, str::FromStr};

    use crate::{
        proof::{
            proof_rope::{BaseMetric, LinesMetric, ProofDelta, StepsMetric},
            ProofRope,
        },
        rope_ext::RopeExt,
    };

    #[test]
    fn empty() {
        let p = ProofRope::from_str("").unwrap();
        assert_eq!(0, p.char_len());
        assert_eq!(0, p.0.measure::<BaseMetric>());
        assert_eq!(0, p.0.measure::<LinesMetric>());
        assert_eq!(0, p.0.measure::<StepsMetric>());
    }

    #[test]
    fn single_char() {
        let p = ProofRope::from_str("a").unwrap();
        assert_eq!(1, p.char_len());
        assert_eq!(1, p.0.measure::<BaseMetric>());
        assert_eq!(0, p.0.measure::<LinesMetric>()); // Fix that!
        assert_eq!(0, p.0.measure::<StepsMetric>()); // Fix that!
    }

    #[test]
    fn single_line() {
        let p = ProofRope::from_str("a\n").unwrap();
        assert_eq!(2, p.char_len());
        assert_eq!(2, p.0.measure::<BaseMetric>());
        assert_eq!(1, p.0.measure::<LinesMetric>()); // Fix that!
        assert_eq!(0, p.0.measure::<StepsMetric>()); // Fix that!
    }

    #[test]
    fn two_lines() {
        let p = ProofRope::from_str("a\nb").unwrap();
        assert_eq!(3, p.char_len());
        assert_eq!(3, p.0.measure::<BaseMetric>());
        assert_eq!(1, p.0.measure::<LinesMetric>()); // Fix that!
        assert_eq!(1, p.0.measure::<StepsMetric>()); // Fix that!
    }

    #[test]
    fn two_lines_one_step() {
        let p = ProofRope::from_str("a\n ").unwrap();
        assert_eq!(3, p.char_len());
        assert_eq!(3, p.0.measure::<BaseMetric>());
        assert_eq!(1, p.0.measure::<LinesMetric>()); // Fix that!
        assert_eq!(0, p.0.measure::<StepsMetric>()); // Fix that!
    }

    #[test]
    fn three_lines_one_step() {
        let p = ProofRope::from_str("a\n \n ").unwrap();
        assert_eq!(5, p.char_len());
        assert_eq!(5, p.0.measure::<BaseMetric>());
        assert_eq!(2, p.0.measure::<LinesMetric>()); // Fix that!
        assert_eq!(0, p.0.measure::<StepsMetric>()); // Fix that!
    }

    #[test]
    fn long_line() {
        // 64 char long string, repeat it 33 times so it is longer than 1024 bytes
        let long_text =
            "1234567812345678123456781234567812345678123456781234567812345678".repeat(33);
        let p = ProofRope::from_str(&long_text).unwrap();
        assert_eq!(64 * 33, p.char_len());
        assert_eq!(64 * 33, p.0.measure::<BaseMetric>());
        assert_eq!(0, p.0.measure::<LinesMetric>()); // Fix that!
        assert_eq!(0, p.0.measure::<StepsMetric>()); // Fix that!
    }

    #[test]
    fn long_line_many_steps() {
        let long_text = "a\n".repeat(1000);
        let p = ProofRope::from_str(&long_text).unwrap();
        assert_eq!(2000, p.char_len());
        assert_eq!(2000, p.0.measure::<BaseMetric>());
        assert_eq!(1000, p.0.measure::<LinesMetric>());
        assert_eq!(998, p.0.measure::<StepsMetric>()); // Fix that!
    }

    #[test]
    fn long_line_ext_steps() {
        let long_text = "a\n \n".repeat(1000);
        let p = ProofRope::from_str(&long_text).unwrap();
        assert_eq!(4000, p.char_len());
        assert_eq!(4000, p.0.measure::<BaseMetric>());
        assert_eq!(2000, p.0.measure::<LinesMetric>());
        assert_eq!(996, p.0.measure::<StepsMetric>()); // Fix that!
    }

    #[test]
    fn cow_for_range_small_string() {
        let short_text = "hi, i'm a small piece of text.";

        let rope = ProofRope::from_str(short_text).unwrap();

        let cow = rope.cow_for_range(..);

        assert!(short_text.len() <= 1024);
        assert_eq!(cow, Cow::Borrowed(short_text) as Cow<str>);
    }

    #[test]
    fn cow_for_range_long_string_long_slice() {
        // 64 char long string, repeat it 33 times so it is longer than 1024 bytes
        let long_text =
            "1234567812345678123456781234567812345678123456781234567812345678".repeat(33);

        let rope = ProofRope::from_str(&long_text).unwrap();

        let cow = rope.cow_for_range(..);

        assert!(long_text.len() > 1024);
        assert_eq!(cow, Cow::Owned(long_text) as Cow<str>);
    }

    #[test]
    fn cow_for_range_long_string_short_slice() {
        // 64 char long string, repeat it 33 times so it is longer than 1024 bytes
        let long_text =
            "1234567812345678123456781234567812345678123456781234567812345678".repeat(33);

        let rope = ProofRope::from_str(&long_text).unwrap();

        let cow = rope.cow_for_range(..500);

        assert!(long_text.len() > 1024);
        assert_eq!(cow, Cow::Borrowed(&long_text[..500]));
    }

    #[test]
    fn change_insert() {
        let p = ProofRope::from_str(&"1000:: \n".repeat(1000)).unwrap();
        assert_eq!(8000, p.char_len());
        assert_eq!(8000, p.0.measure::<BaseMetric>());
        assert_eq!(1000, p.0.measure::<LinesMetric>());
        assert_eq!(992, p.0.measure::<StepsMetric>()); // Fix that!
        let change = TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 2,
                    character: 2,
                },
                end: Position {
                    line: 2,
                    character: 2,
                },
            }),
            range_length: None,
            text: "12345678".to_string(),
        };
        let delta = ProofDelta::from_lsp_change(&p, &change).unwrap();
        let p = p.apply(delta);
        assert_eq!(8008, p.char_len());
        assert_eq!(8008, p.0.measure::<BaseMetric>());
        assert_eq!(1000, p.0.measure::<LinesMetric>());
        assert_eq!(992, p.0.measure::<StepsMetric>()); // Fix that!
    }

    #[test]
    fn change_insert_new_steps() {
        let p = ProofRope::from_str(&"1000:: \n".repeat(1000)).unwrap();
        assert_eq!(8000, p.char_len());
        assert_eq!(8000, p.0.measure::<BaseMetric>());
        assert_eq!(1000, p.0.measure::<LinesMetric>());
        assert_eq!(992, p.0.measure::<StepsMetric>()); // Fix that!
        let change = TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 2,
                    character: 2,
                },
                end: Position {
                    line: 2,
                    character: 2,
                },
            }),
            range_length: None,
            text: "200\n300\n".to_string(),
        };
        let delta = ProofDelta::from_lsp_change(&p, &change).unwrap();
        let p = p.apply(delta);
        assert_eq!(8008, p.char_len());
        assert_eq!(8008, p.0.measure::<BaseMetric>());
        assert_eq!(1002, p.0.measure::<LinesMetric>());
        assert_eq!(993, p.0.measure::<StepsMetric>()); // Fix that!
    }
}
