//! A collection of utilities for ropes.
//! Takes care of reading Xi Ropes from files, adapting byte indices to LSP text positions, and providing lines
use lsp_types::*;
use std::borrow::Cow;
use std::io::Error as IoError;
use std::io::ErrorKind;
use xi_rope::engine::Error;
use xi_rope::interval::IntervalBounds;
use xi_rope::tree::Node;
use xi_rope::tree::NodeInfo;
use xi_rope::tree::TreeBuilder;
use xi_rope::Cursor;
use xi_rope::Interval;
use xi_rope::Rope;
use xi_rope::RopeDelta;
use xi_rope::RopeInfo;

pub trait StringTreeBuilder {
    fn push_string(&mut self, s: &str);
}

impl StringTreeBuilder for TreeBuilder<RopeInfo> {
    fn push_string(&mut self, s: &str) {
        self.push_str_stacked(s);
    }
}

pub trait RopeExt<I>
where
    I: NodeInfo,
    TreeBuilder<I>: StringTreeBuilder,
{
    fn line_to_offset(&self, line_idx: usize) -> usize;
    fn offset_to_line(&self, byte_idx: usize) -> usize;
    fn cow_for_range<T>(&self, range: T) -> Cow<str>
    where
        T: IntervalBounds;

    fn byte_to_lsp_position(&self, byte_idx: usize) -> Position {
        let line_idx = self.offset_to_line(byte_idx);
        let start_line_idx = self.line_to_offset(line_idx);
        Position::new(line_idx as u32, (byte_idx - start_line_idx) as u32)
    }

    fn lsp_position_to_byte(&self, position: Position) -> usize {
        let start_line_idx = self.line_to_offset(position.line as usize);
        start_line_idx + position.character as usize
    }

    fn line(&self, line_idx: u32) -> Cow<str> {
        let start_byte_idx = self.line_to_offset(line_idx as usize);
        let end_byte_idx = self.line_to_offset((line_idx + 1) as usize as usize);
        self.cow_for_range(start_byte_idx..end_byte_idx)
    }

    fn cursor_to_lsp_position(&self, cursor: Cursor<I>) -> Result<Position, Error>;
    fn lsp_position_to_cursor(&self, position: Position) -> Result<Cursor<I>, Error>;
    fn change_event_to_rope_delta(
        &self,
        change: &TextDocumentContentChangeEvent,
    ) -> Result<RopeDelta, Error> {
        let text = change.text.as_str();
        let text_bytes = text.as_bytes();
        let text_end_byte_idx = text_bytes.len();

        let interval = if let Some(range) = change.range {
            Interval::new(
                self.lsp_position_to_byte(range.start),
                self.lsp_position_to_byte(range.end),
            )
        } else {
            Interval::new(0, text_end_byte_idx)
        };

        let new_text = Rope::from(text);
        Ok(RopeDelta::simple_edit(interval, new_text, text.len()))
    }
}

pub fn read_to_rope<T: std::io::Read, I: NodeInfo>(mut reader: T) -> Result<Node<I>, IoError>
where
    TreeBuilder<I>: StringTreeBuilder,
{
    // Note: this method is based on Ropey's `from_reader`, adapted to Xi ropes
    const BUFFER_SIZE: usize = 4096;
    let mut builder = TreeBuilder::new();
    let mut buffer = [0u8; BUFFER_SIZE];
    let mut fill_idx = 0; // How much `buffer` is currently filled with valid data
    loop {
        match reader.read(&mut buffer[fill_idx..]) {
            Ok(read_count) => {
                fill_idx += read_count;

                // Determine how much of the buffer is valid utf8.
                let valid_count = match std::str::from_utf8(&buffer[..fill_idx]) {
                    Ok(_) => fill_idx,
                    Err(e) => e.valid_up_to(),
                };

                // Append the valid part of the buffer to the rope.
                if valid_count > 0 {
                    builder.push_string(
                        std::str::from_utf8(&buffer[..valid_count])
                            .map_err(|e| IoError::new(ErrorKind::InvalidData, e))?,
                    );
                }

                // Shift the un-read part of the buffer to the beginning.
                if valid_count < fill_idx {
                    buffer.copy_within(valid_count..fill_idx, 0);
                }
                fill_idx -= valid_count;

                if fill_idx == BUFFER_SIZE {
                    // Buffer is full and none of it could be consumed.  Utf8
                    // codepoints don't get that large, so it's clearly not
                    // valid text.
                    return Err(IoError::new(
                        ErrorKind::InvalidData,
                        "stream did not contain valid UTF-8",
                    ));
                }

                // If we're done reading
                if read_count == 0 {
                    if fill_idx > 0 {
                        // We couldn't consume all data.
                        return Err(IoError::new(
                            ErrorKind::InvalidData,
                            "stream contained invalid UTF-8",
                        ));
                    } else {
                        return Ok(builder.build());
                    }
                }
            }

            Err(e) => {
                // Read error
                return Err(e);
            }
        }
    }
}

impl RopeExt<RopeInfo> for Rope {
    fn line_to_offset(&self, line_idx: usize) -> usize {
        self.offset_of_line(line_idx)
    }

    fn offset_to_line(&self, byte_idx: usize) -> usize {
        self.line_of_offset(byte_idx)
    }

    fn cow_for_range<T>(&self, range: T) -> Cow<str>
    where
        T: IntervalBounds,
    {
        self.slice_to_cow(range)
    }

    fn cursor_to_lsp_position(
        &self,
        _cursor: xi_rope::Cursor<RopeInfo>,
    ) -> std::result::Result<lsp_types::Position, xi_rope::engine::Error> {
        todo!()
    }

    fn lsp_position_to_cursor(&self, _position: Position) -> Result<Cursor<RopeInfo>, Error> {
        todo!()
    }
}
