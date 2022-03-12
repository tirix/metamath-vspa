//! This is an enhanced xi-rope data structure, which also holds information about proof steps

use std::str::FromStr;
use std::string::ParseError;

use crate::proof::step::Step;
use crate::rope_ext::StringTreeBuilder;
use std::cmp::{max, min};
use std::sync::Arc;
use xi_rope::Metric;
use xi_rope::Interval;
use xi_rope::tree::Leaf;
use xi_rope::tree::Node;
use xi_rope::tree::NodeInfo;
use xi_rope::tree::TreeBuilder;
use xi_rope::rope::count_newlines;
use memchr::{memchr, memrchr};

/// Structure for a rope containing proof information
/// This structure allows to retrieve line number information, but also step number.
#[derive(Clone, Default)]
pub struct ProofRope(Node<StepsInfo>);

impl ProofRope {
    pub fn from_reader<T: std::io::Read>(file: T) -> Result<Self, std::io::Error> {
        Ok(Self(crate::rope_ext::read_to_rope(file)?))
    }
}

impl FromStr for ProofRope {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<ProofRope, Self::Err> {
        let mut b = TreeBuilder::new();
        if s.len() <= MAX_LEAF {
            if !s.is_empty() {
                b.push_leaf(s.into());
            }
        }
        while !s.is_empty() {
            let splitpoint = if s.len() > MAX_LEAF { find_leaf_split_for_bulk(s) } else { s.len() };
            b.push_leaf(s[..splitpoint].into());
            s = &s[splitpoint..];
        }
        Ok(ProofRope(b.build()))
    }
}


const MIN_LEAF: usize = 511;
const MAX_LEAF: usize = 1024;

/// This `StepsInfo` structure is used as nodes in the `ProofRope`.
/// It stores both the number of lines and the number of steps at each node.
/// 
/// It is associated with the `StepsLeaf` leaf structure, which holds
/// the string at the leaf, as well as the corresponding steps.
#[derive(Clone)]
struct StepsInfo {
    lines: usize,
    steps: usize,
}

impl NodeInfo for StepsInfo {
    type L = StepsLeaf;

    /// Accumulate the provided node with this one.
    /// Simply adds up the numbers of lines and steps.
    fn accumulate(&mut self, other: &Self) {
        self.lines += other.lines;
        self.steps += other.steps;
    }

    /// Initial calculations for a leaf.
    fn compute_info(l: &Self::L) -> Self {
        StepsInfo {
            lines: count_newlines(&l.text),
            steps: l.steps.len(),
        }
    }
}

/// Checks whether the line is a follow-up of the previous line
/// A line starting with a space to tab shall simply be concatenated with the previous one
#[inline]
fn is_followup_line(source: &ProofRope, line_idx: usize) -> bool {
    matches!(
        source.char(source.line_to_char(line_idx)),
        ' ' | '\t' | '\n'
    )
}

/// Counts how many step starts there are in the given string
pub fn count_step_starts(s: &str) -> usize {
    todo!();
    bytecount::count(s.as_bytes(), b'\n')
}

fn find_leaf_split_for_bulk(s: &str) -> usize {
    find_leaf_split(s, MIN_LEAF)
}

fn find_leaf_split_for_merge(s: &str) -> usize {
    find_leaf_split(s, max(MIN_LEAF, s.len() - MAX_LEAF))
}

// Try to split at newline boundary (leaning left), if not, then split at codepoint
fn find_leaf_split(s: &str, minsplit: usize) -> usize {
    let mut splitpoint = min(MAX_LEAF, s.len() - MIN_LEAF);
    match memrchr(b'\n', &s.as_bytes()[minsplit - 1..splitpoint]) {
        Some(pos) => minsplit + pos,
        None => {
            while !s.is_char_boundary(splitpoint) {
                splitpoint -= 1;
            }
            splitpoint
        }
    }
}

/// This steps leaf stores 2 kinds of information:
#[derive(Clone, Default)]
struct StepsLeaf {
    /// content of this leaf
    text: String,
    /// steps
    steps: Vec<Arc<Step>>,
}

impl From<&str> for StepsLeaf {
    fn from(_: &str) -> Self {
        todo!()
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
        self.text.push_str(&other.text[start..end]);
        for &v in &other.steps {
            if start < v.start && v.start <= end {
                self.steps.push(v);
            }
        }
        if self.text.len() <= MAX_LEAF {
            None
        } else {
            let splitpoint = find_leaf_split_for_merge(&self.text);
            let right_str = self.text[splitpoint..].to_owned();
            self.text.truncate(splitpoint);
            self.text.shrink_to_fit();
            let split_index = todo!();
            Some(StepsLeaf{
                text: right_str,
                steps: self.steps.split_off(split_index),
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

#[derive(Clone, Copy)]
pub struct LinesMetric(usize); // number of lines

#[derive(Clone, Copy)]
pub struct StepsMetric(usize); // number of steps

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
        info.steps
    }

    /// 
    fn is_boundary(s: &StepsLeaf, offset: usize) -> bool {
        if offset == 0 {
            // shouldn't be called with this, but be defensive
            false
        } else {
            let first_char = s.text.as_bytes()[offset];
            s.text.as_bytes()[offset - 1] == b'\n'
                && first_char != b' '
                && first_char != b'\t'
                && first_char != b'\n'
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
        count_step_starts(&s.text[..in_base_units])
    }

    fn prev(s: &StepsLeaf, offset: usize) -> Option<usize> {
        debug_assert!(offset > 0, "caller is responsible for validating input");
        todo!();
        memrchr(b'\n', &s.text.as_bytes()[..offset - 1]).map(|pos| pos + 1)
    }

    fn next(s: &StepsLeaf, offset: usize) -> Option<usize> {
        todo!();
        memchr(b'\n', &s.text.as_bytes()[offset..]).map(|pos| offset + pos + 1)
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
            let splitpoint = if s.len() > MAX_LEAF { find_leaf_split_for_bulk(s) } else { s.len() };
            self.push_leaf(s[..splitpoint].into());
            s = &s[splitpoint..];
        }
    }
}